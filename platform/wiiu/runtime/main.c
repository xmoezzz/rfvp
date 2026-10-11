/* The Wii U runtime: wut owns the process (ProcUI, through libwhb), the GamePad, sound (AX) and
 * the GPU (GX2, set up by libwhb); the rfvp engine (Rust, linked in as librfvp_wiiu.a) runs one
 * frame per call and draws through the functions below. The frame's draws are recorded, then
 * played on the TV and again on the GamePad.
 *
 * The game is read from sd:/wiiu/apps/rfvp/game; saves go to sd:/wiiu/apps/rfvp/saves. */

#include <dirent.h>
#include <malloc.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>

#include <coreinit/cache.h>
#include <coreinit/debug.h>
#include <coreinit/thread.h>
#include <coreinit/time.h>
#include <sndcore2/core.h>
#include <sndcore2/device.h>
#include <sndcore2/voice.h>
#include <gx2/draw.h>
#include <gx2/event.h>
#include <gx2/mem.h>
#include <gx2/registers.h>
#include <gx2/sampler.h>
#include <gx2/shaders.h>
#include <gx2/state.h>
#include <gx2/surface.h>
#include <gx2/texture.h>
#include <gx2/utils.h>
#include <vpad/input.h>
#include <whb/gfx.h>
#include <whb/proc.h>

#include "shaders/textureShader.inc"

/* The space pointer positions arrive in (the GamePad's touch screen, calibrated to 720p); the
 * picture itself is drawn at whatever size the video mode has. */
#define TV_WIDTH 1280
#define TV_HEIGHT 720

static const char* const AppRoot = "fs:/vol/external01/wiiu/apps/rfvp";
static const char* const GameRoot = "fs:/vol/external01/wiiu/apps/rfvp/game";

/* `struct WiiuInput` in event.rs (the Switch port's layout). */
typedef struct {
    uint64_t buttons;
    int32_t stick_x;
    int32_t stick_y;
    uint32_t touch_count;
    uint32_t touch_id;
    int32_t touch_x;
    int32_t touch_y;
} RfvpWiiuInput;

/* `struct CWallTime` in ffi.rs. */
typedef struct {
    uint16_t year;
    uint8_t month;
    uint8_t day;
    uint8_t day_of_week;
    uint8_t hour;
    uint8_t minute;
} RfvpWiiuWallTime;

/* The engine (platform/wiiu/host/src/ffi.rs). */
extern void* rfvp_wiiu_start(const char* game_root);
extern int rfvp_wiiu_frame(void* app, const RfvpWiiuInput* input);
extern void rfvp_wiiu_audio_render(void* app, int16_t* dst, size_t frames);
extern void rfvp_wiiu_stop(void* app);

/* ---- Services for the engine ---------------------------------------------------------------- */

void rfvp_wiiu_log(const char* message, size_t len) {
    /* Cemu shows these in its log (with "Coreinit logging" on); on a console they go to the
     * debug output. */
    char line[512];
    size_t n = len < sizeof(line) - 1 ? len : sizeof(line) - 1;
    memcpy(line, message, n);
    line[n] = 0;
    OSReport("%s", line);
}

uint64_t rfvp_wiiu_ticks_us(void) {
    return (uint64_t) OSTicksToMicroseconds(OSGetSystemTime());
}

bool rfvp_wiiu_wall_time(RfvpWiiuWallTime* out) {
    OSCalendarTime t;
    OSTicksToCalendarTime(OSGetTime(), &t);
    out->year = (uint16_t) t.tm_year;
    out->month = (uint8_t) (t.tm_mon + 1);
    out->day = (uint8_t) t.tm_mday;
    out->day_of_week = (uint8_t) t.tm_wday;
    out->hour = (uint8_t) t.tm_hour;
    out->minute = (uint8_t) t.tm_min;
    return true;
}

typedef void (*RfvpDirVisitor)(void* ctx, const char* name, bool is_dir, uint64_t size);

/* List a directory; Rust does not know newlib's dirent and stat layouts. */
int rfvp_wiiu_list_dir(const char* path, RfvpDirVisitor visit, void* ctx) {
    DIR* dir = opendir(path);
    if (dir == NULL) return -1;
    char child[512];
    struct dirent* entry;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) continue;
        snprintf(child, sizeof(child), "%s/%s", path, entry->d_name);
        struct stat st;
        if (stat(child, &st) != 0) continue;
        visit(ctx, entry->d_name, S_ISDIR(st.st_mode), (uint64_t) st.st_size);
    }
    closedir(dir);
    return 0;
}

/* 1 for a file, 2 for a directory, 0 if nothing is there. */
int rfvp_wiiu_path_kind(const char* path, uint64_t* size) {
    struct stat st;
    if (stat(path, &st) != 0) return 0;
    if (size != NULL) *size = (uint64_t) st.st_size;
    return S_ISDIR(st.st_mode) ? 2 : 1;
}

/* ---- Drawing ------------------------------------------------------------------------------- */

typedef struct {
    GX2Texture tex;
    GX2Sampler sampler;
} GpuTexture;

/* One corner: position in the game's virtual frame, color, texture coordinates. */
typedef struct {
    float x, y;
    uint8_t r, g, b, a;
    float u, v;
} Vertex;

/* A recorded draw: four vertices (a triangle strip) in the frame's vertex memory. */
typedef struct {
    GpuTexture* texture;
    uint32_t first;
    uint8_t additive, clipped;
    float clip[4]; /* x0, y0, x1, y1 in virtual coordinates */
} Draw;

enum { MaxDraws = 16384, VertexArenas = 3 };
static WHBGfxShaderGroup shader;
static GpuTexture* white;
static Vertex* arenas[VertexArenas];
static unsigned arena;
static Draw* draws;
static uint32_t draw_count;
static float virtual_w = 1920, virtual_h = 1080;
static bool frame_open;
static uint64_t gpu_frame;

/* Textures the engine dropped while the GPU may still read them: freed a few frames later. */
typedef struct { GpuTexture* texture; uint64_t frame; } DeadTexture;
static DeadTexture dead[1024];
static int dead_count;

void rfvp_wiiu_gpu_screen_size(uint32_t* width, uint32_t* height) {
    *width = TV_WIDTH;
    *height = TV_HEIGHT;
}

/* Write RGBA8 rows (w x h at x, y) into a linear texture; GX2's R8_G8_B8_A8 is these bytes. */
static void copy_pixels(GpuTexture* t, uint32_t x, uint32_t y, uint32_t w, uint32_t h,
                        const uint8_t* rgba) {
    uint8_t* image = t->tex.surface.image;
    uint32_t pitch = t->tex.surface.pitch * 4;
    for (uint32_t row = 0; row < h; row++) {
        memcpy(image + (size_t) (y + row) * pitch + x * 4, rgba + (size_t) row * w * 4, w * 4);
    }
    GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, image, t->tex.surface.imageSize);
}

void* rfvp_wiiu_gpu_create_texture(uint32_t width, uint32_t height, const uint8_t* rgba) {
    GpuTexture* t = calloc(1, sizeof(GpuTexture));
    if (t == NULL) return NULL;
    GX2Surface* s = &t->tex.surface;
    s->width = width;
    s->height = height;
    s->depth = 1;
    s->mipLevels = 1;
    s->format = GX2_SURFACE_FORMAT_UNORM_R8_G8_B8_A8;
    s->dim = GX2_SURFACE_DIM_TEXTURE_2D;
    s->tileMode = GX2_TILE_MODE_LINEAR_ALIGNED;
    s->use = GX2_SURFACE_USE_TEXTURE;
    t->tex.viewNumMips = 1;
    t->tex.viewNumSlices = 1;
    t->tex.compMap = GX2_COMP_MAP(GX2_SQ_SEL_R, GX2_SQ_SEL_G, GX2_SQ_SEL_B, GX2_SQ_SEL_A);
    GX2CalcSurfaceSizeAndAlignment(s);
    GX2InitTextureRegs(&t->tex);
    s->image = memalign(s->alignment, s->imageSize);
    if (s->image == NULL) {
        free(t);
        return NULL;
    }
    memset(s->image, 0, s->imageSize);
    GX2InitSampler(&t->sampler, GX2_TEX_CLAMP_MODE_CLAMP, GX2_TEX_XY_FILTER_MODE_LINEAR);
    if (rgba != NULL) {
        copy_pixels(t, 0, 0, width, height, rgba);
    } else {
        GX2Invalidate(GX2_INVALIDATE_MODE_CPU_TEXTURE, s->image, s->imageSize);
    }
    return t;
}

void rfvp_wiiu_gpu_update_texture(void* texture, uint32_t x, uint32_t y, uint32_t w, uint32_t h,
                                  const uint8_t* rgba) {
    copy_pixels((GpuTexture*) texture, x, y, w, h, rgba);
}

static void free_texture(GpuTexture* t) {
    free(t->tex.surface.image);
    free(t);
}

void rfvp_wiiu_gpu_destroy_texture(void* texture) {
    if (dead_count == (int) (sizeof(dead) / sizeof(dead[0]))) {
        GX2DrawDone();
        for (int i = 0; i < dead_count; i++) free_texture(dead[i].texture);
        dead_count = 0;
    }
    dead[dead_count].texture = texture;
    dead[dead_count].frame = gpu_frame;
    dead_count++;
}

/* A new frame of `width` x `height` virtual pixels. */
void rfvp_wiiu_gpu_begin_frame(float width, float height) {
    virtual_w = width;
    virtual_h = height;
    draw_count = 0;
    arena = (arena + 1) % VertexArenas;
    frame_open = true;
}

/* A quad: `c` holds virtual x, y and normalized u, v for each corner, in triangle-strip order;
 * `clip` (x0, y0, x1, y1, virtual) or null. A null texture draws the color. */
void rfvp_wiiu_gpu_draw(void* texture, const float* c, uint32_t rgba, int additive,
                        const float* clip) {
    if (!frame_open || draw_count == MaxDraws) return;
    Draw* d = &draws[draw_count];
    d->texture = texture != NULL ? texture : white;
    d->first = draw_count * 4;
    d->additive = (uint8_t) additive;
    d->clipped = clip != NULL;
    if (clip != NULL) memcpy(d->clip, clip, sizeof(d->clip));
    Vertex* v = arenas[arena] + d->first;
    for (int i = 0; i < 4; i++) {
        v[i].x = c[i * 4];
        v[i].y = c[i * 4 + 1];
        v[i].u = c[i * 4 + 2];
        v[i].v = c[i * 4 + 3];
        v[i].r = (uint8_t) rgba;
        v[i].g = (uint8_t) (rgba >> 8);
        v[i].b = (uint8_t) (rgba >> 16);
        v[i].a = (uint8_t) (rgba >> 24);
    }
    draw_count++;
}

void rfvp_wiiu_gpu_end_frame(void) {
}

/* Play the frame's draws on a target of `width` x `height`, the game letterboxed in it. */
static void play_draws(uint32_t width, uint32_t height) {
    float scale = (float) width / virtual_w;
    if ((float) height / virtual_h < scale) scale = (float) height / virtual_h;
    float vw = virtual_w * scale, vh = virtual_h * scale;
    float ox = ((float) width - vw) * 0.5f, oy = ((float) height - vh) * 0.5f;
    GX2SetViewport(ox, oy, vw, vh, 0.0f, 1.0f);

    GX2SetFetchShader(&shader.fetchShader);
    GX2SetVertexShader(shader.vertexShader);
    GX2SetPixelShader(shader.pixelShader);
    GX2SetDepthOnlyControl(FALSE, FALSE, GX2_COMPARE_FUNC_ALWAYS);
    GX2SetCullOnlyControl(GX2_FRONT_FACE_CCW, FALSE, FALSE);
    GX2SetColorControl(GX2_LOGIC_OP_COPY, 0xFF, FALSE, TRUE);
    /* Virtual pixels to clip space: x' = 2x/w - 1, y' = 1 - 2y/h (u_projection's columns). */
    float projection[16] = {
        2.0f / virtual_w, 0, 0, 0,
        0, -2.0f / virtual_h, 0, 0,
        0, 0, 1, 0,
        -1.0f, 1.0f, 0, 1,
    };
    GX2SetVertexUniformReg(shader.vertexShader->uniformVars[0].offset, 16, (uint32_t*) projection);

    int blend = -1;
    GpuTexture* bound = NULL;
    Vertex* vertices = arenas[arena];
    for (uint32_t i = 0; i < draw_count; i++) {
        Draw* d = &draws[i];
        if (d->additive != blend) {
            blend = d->additive;
            GX2SetBlendControl(GX2_RENDER_TARGET_0, GX2_BLEND_MODE_SRC_ALPHA,
                               blend ? GX2_BLEND_MODE_ONE : GX2_BLEND_MODE_INV_SRC_ALPHA,
                               GX2_BLEND_COMBINE_MODE_ADD, TRUE, GX2_BLEND_MODE_ONE,
                               GX2_BLEND_MODE_INV_SRC_ALPHA, GX2_BLEND_COMBINE_MODE_ADD);
        }
        if (d->texture != bound) {
            bound = d->texture;
            uint32_t location = shader.pixelShader->samplerVars[0].location;
            GX2SetPixelTexture(&bound->tex, location);
            GX2SetPixelSampler(&bound->sampler, location);
        }
        float x0 = ox, y0 = oy, x1 = ox + vw, y1 = oy + vh;
        if (d->clipped) {
            float cx0 = ox + d->clip[0] * scale, cy0 = oy + d->clip[1] * scale;
            float cx1 = ox + d->clip[2] * scale, cy1 = oy + d->clip[3] * scale;
            if (cx0 > x0) x0 = cx0;
            if (cy0 > y0) y0 = cy0;
            if (cx1 < x1) x1 = cx1;
            if (cy1 < y1) y1 = cy1;
        }
        if (x1 <= x0 || y1 <= y0) continue;
        GX2SetScissor((uint32_t) (x0 + 0.5f), (uint32_t) (y0 + 0.5f),
                      (uint32_t) (x1 - x0 + 0.5f), (uint32_t) (y1 - y0 + 0.5f));
        GX2SetAttribBuffer(0, 4 * sizeof(Vertex), sizeof(Vertex), vertices + d->first);
        GX2DrawEx(GX2_PRIMITIVE_MODE_TRIANGLE_STRIP, 4, 0, 1);
    }
}

static bool want_shot;
static void save_screenshot(void);

/* Show the frame: on the TV, then on the GamePad. */
static void present(void) {
    if (draw_count > 0) {
        GX2Invalidate(GX2_INVALIDATE_MODE_CPU_ATTRIBUTE_BUFFER, arenas[arena],
                      draw_count * 4 * sizeof(Vertex));
    }
    WHBGfxBeginRender();
    WHBGfxBeginRenderTV();
    WHBGfxClearColor(0.0f, 0.0f, 0.0f, 1.0f);
    /* The colour buffers are as large as the video mode (720p or 1080p on the TV). */
    GX2Surface* tv = &WHBGfxGetTVColourBuffer()->surface;
    play_draws(tv->width, tv->height);
    if (want_shot) {
        save_screenshot();
        want_shot = false;
    }
    WHBGfxFinishRenderTV();
    WHBGfxBeginRenderDRC();
    WHBGfxClearColor(0.0f, 0.0f, 0.0f, 1.0f);
    GX2Surface* drc = &WHBGfxGetDRCColourBuffer()->surface;
    play_draws(drc->width, drc->height);
    WHBGfxFinishRenderDRC();
    WHBGfxFinishRender();
    gpu_frame++;
    /* Textures dropped three frames ago are out of the GPU's hands. */
    int kept = 0;
    for (int i = 0; i < dead_count; i++) {
        if (gpu_frame - dead[i].frame > VertexArenas) {
            free_texture(dead[i].texture);
        } else {
            dead[kept++] = dead[i];
        }
    }
    dead_count = kept;
}

static bool gpu_init(void) {
    if (!WHBGfxInit()) return false;
    if (!WHBGfxLoadGFDShaderGroup(&shader, 0, textureShader_gsh)) return false;
    WHBGfxInitShaderAttribute(&shader, "a_position", 0, offsetof(Vertex, x), GX2_ATTRIB_FORMAT_FLOAT_32_32);
    WHBGfxInitShaderAttribute(&shader, "a_color", 0, offsetof(Vertex, r), GX2_ATTRIB_FORMAT_UNORM_8_8_8_8);
    WHBGfxInitShaderAttribute(&shader, "a_texcoord", 0, offsetof(Vertex, u), GX2_ATTRIB_FORMAT_FLOAT_32_32);
    if (!WHBGfxInitFetchShader(&shader)) return false;
    for (int i = 0; i < VertexArenas; i++) {
        arenas[i] = memalign(GX2_VERTEX_BUFFER_ALIGNMENT, MaxDraws * 4 * sizeof(Vertex));
        if (arenas[i] == NULL) return false;
    }
    draws = malloc(MaxDraws * sizeof(Draw));
    static uint8_t white_pixels[8 * 8 * 4];
    memset(white_pixels, 0xff, sizeof(white_pixels));
    white = rfvp_wiiu_gpu_create_texture(8, 8, white_pixels);
    return draws != NULL && white != NULL;
}

/* ---- Sound ---------------------------------------------------------------------------------- */

/* Two AX voices (left, right) loop over rings of 16-bit samples at 48 kHz; each frame the part
 * that has played is refilled from the mixer, in its blocks of 1024 frames (audio.rs). */
enum { AudioRing = 8192, AudioBlock = 1024, AudioAhead = 4096 };
static AXVoice* voices[2];
static int16_t* rings[2];
static uint32_t write_pos; /* in frames, modulo AudioRing */
static bool audio_ready;
static int16_t mix_block[AudioBlock * 2];
/* With debug.txt: the loudest sample of each second goes to the log, to tell sound from silence. */
static bool audio_debug;
static int audio_peak;
static uint32_t audio_blocks;

static void audio_init(void) {
    AXInitParams params = { AX_INIT_RENDERER_48KHZ, 0, AX_INIT_PIPELINE_SINGLE };
    AXInitWithParams(&params);
    for (int c = 0; c < 2; c++) {
        rings[c] = memalign(64, AudioRing * sizeof(int16_t));
        if (rings[c] == NULL) return;
        memset(rings[c], 0, AudioRing * sizeof(int16_t));
        DCFlushRange(rings[c], AudioRing * sizeof(int16_t));
        voices[c] = AXAcquireVoice(31, NULL, NULL);
        if (voices[c] == NULL) return;
    }
    for (int c = 0; c < 2; c++) {
        AXVoice* v = voices[c];
        AXVoiceBegin(v);
        AXSetVoiceType(v, 0);
        AXVoiceVeData ve = { 0x8000, 0 };
        AXSetVoiceVe(v, &ve);
        /* Left to the left channel, right to the right, on the TV and on the GamePad. */
        AXVoiceDeviceMixData mix[6];
        memset(mix, 0, sizeof(mix));
        mix[c].bus[0].volume = 0x8000;
        AXSetVoiceDeviceMix(v, AX_DEVICE_TYPE_TV, 0, mix);
        AXSetVoiceDeviceMix(v, AX_DEVICE_TYPE_DRC, 0, mix);
        AXVoiceOffsets offsets = {
            .dataType = AX_VOICE_FORMAT_LPCM16,
            .loopingEnabled = AX_VOICE_LOOP_ENABLED,
            .loopOffset = 0,
            .endOffset = AudioRing - 1,
            .currentOffset = 0,
            .data = rings[c],
        };
        AXSetVoiceOffsets(v, &offsets);
        AXSetVoiceSrcType(v, AX_VOICE_SRC_TYPE_NONE);
        AXSetVoiceSrcRatio(v, 1.0f);
        AXVoiceEnd(v);
    }
    /* Start both at once, so they stay in step. */
    AXVoiceBegin(voices[0]);
    AXVoiceBegin(voices[1]);
    AXSetVoiceState(voices[0], AX_VOICE_STATE_PLAYING);
    AXSetVoiceState(voices[1], AX_VOICE_STATE_PLAYING);
    AXVoiceEnd(voices[1]);
    AXVoiceEnd(voices[0]);
    audio_ready = true;
}

/* Keep the rings filled up to AudioAhead frames past where the voices play. */
static void audio_pump(void* app) {
    if (!audio_ready) return;
    AXVoiceOffsets offsets;
    AXGetVoiceOffsets(voices[0], &offsets);
    uint32_t play = offsets.currentOffset % AudioRing;
    for (;;) {
        uint32_t ahead = (write_pos + AudioRing - play) % AudioRing;
        if (ahead >= AudioAhead && ahead < AudioRing - AudioBlock) break;
        rfvp_wiiu_audio_render(app, mix_block, AudioBlock);
        if (audio_debug) {
            for (int i = 0; i < AudioBlock * 2; i++) {
                int v = mix_block[i] < 0 ? -mix_block[i] : mix_block[i];
                if (v > audio_peak) audio_peak = v;
            }
            if (++audio_blocks % 47 == 0) {
                OSReport("rfvp: audio peak %d\n", audio_peak);
                audio_peak = 0;
            }
        }
        for (int i = 0; i < AudioBlock; i++) {
            uint32_t at = (write_pos + i) % AudioRing;
            rings[0][at] = mix_block[i * 2];
            rings[1][at] = mix_block[i * 2 + 1];
        }
        /* AudioRing is a multiple of AudioBlock: a block never wraps. */
        DCFlushRange(rings[0] + write_pos, AudioBlock * sizeof(int16_t));
        DCFlushRange(rings[1] + write_pos, AudioBlock * sizeof(int16_t));
        write_pos = (write_pos + AudioBlock) % AudioRing;
    }
}

static void audio_exit(void) {
    for (int c = 0; c < 2; c++) {
        if (voices[c] != NULL) AXFreeVoice(voices[c]);
    }
    AXQuit();
}

/* ---- Input ---------------------------------------------------------------------------------- */

/* libnx HidNpadButton bits (event.rs). */
enum {
    NX_A = 1 << 0, NX_B = 1 << 1, NX_X = 1 << 2, NX_Y = 1 << 3, NX_L = 1 << 6, NX_R = 1 << 7,
    NX_ZL = 1 << 8, NX_ZR = 1 << 9, NX_PLUS = 1 << 10, NX_MINUS = 1 << 11, NX_LEFT = 1 << 12,
    NX_UP = 1 << 13, NX_RIGHT = 1 << 14, NX_DOWN = 1 << 15,
};

/* Debug: button presses scripted in debug.txt (`press=a@600`), and a screenshot
 * (`shot=600`), for testing without hands. */
#define MAX_PRESSES 64
static struct { uint64_t button; unsigned frame; } presses[MAX_PRESSES];
static int press_count;
static unsigned shot_frame = 0xffffffffu;

static void read_debug_file(void) {
    static const struct { const char* name; uint64_t bit; } names[] = {
        { "a", NX_A }, { "b", NX_B }, { "x", NX_X }, { "y", NX_Y }, { "up", NX_UP },
        { "down", NX_DOWN }, { "left", NX_LEFT }, { "right", NX_RIGHT }, { "plus", NX_PLUS },
        { "minus", NX_MINUS }, { "l", NX_L }, { "r", NX_R },
    };
    char path[160];
    snprintf(path, sizeof(path), "%s/debug.txt", AppRoot);
    FILE* f = fopen(path, "r");
    if (f == NULL) return;
    audio_debug = true;
    char line[64], name[16];
    unsigned frame;
    while (fgets(line, sizeof(line), f) != NULL) {
        if (sscanf(line, "shot=%u", &frame) == 1) shot_frame = frame;
        if (press_count >= MAX_PRESSES || sscanf(line, "press=%15[a-z]@%u", name, &frame) != 2) continue;
        for (unsigned i = 0; i < sizeof(names) / sizeof(names[0]); i++) {
            if (strcmp(name, names[i].name) == 0) {
                presses[press_count].button = names[i].bit;
                presses[press_count].frame = frame;
                press_count++;
            }
        }
    }
    fclose(f);
}

static void read_input(RfvpWiiuInput* input, unsigned frame) {
    memset(input, 0, sizeof(*input));
    VPADStatus status;
    VPADReadError error = VPAD_READ_SUCCESS;
    if (VPADRead(VPAD_CHAN_0, &status, 1, &error) > 0 && error == VPAD_READ_SUCCESS) {
        static const struct { uint32_t vpad; uint64_t nx; } map[] = {
            { VPAD_BUTTON_A, NX_A }, { VPAD_BUTTON_B, NX_B }, { VPAD_BUTTON_X, NX_X },
            { VPAD_BUTTON_Y, NX_Y }, { VPAD_BUTTON_L, NX_L }, { VPAD_BUTTON_R, NX_R },
            { VPAD_BUTTON_ZL, NX_ZL }, { VPAD_BUTTON_ZR, NX_ZR }, { VPAD_BUTTON_PLUS, NX_PLUS },
            { VPAD_BUTTON_MINUS, NX_MINUS }, { VPAD_BUTTON_LEFT, NX_LEFT },
            { VPAD_BUTTON_UP, NX_UP }, { VPAD_BUTTON_RIGHT, NX_RIGHT },
            { VPAD_BUTTON_DOWN, NX_DOWN },
        };
        for (unsigned i = 0; i < sizeof(map) / sizeof(map[0]); i++) {
            if (status.hold & map[i].vpad) input->buttons |= map[i].nx;
        }
        input->stick_x = (int32_t) (status.leftStick.x * 32767.0f);
        input->stick_y = (int32_t) (status.leftStick.y * 32767.0f);
        if (status.tpNormal.touched && status.tpNormal.validity == VPAD_VALID) {
            VPADTouchData point;
            VPADGetTPCalibratedPointEx(VPAD_CHAN_0, VPAD_TP_1280X720, &point, &status.tpNormal);
            input->touch_count = 1;
            input->touch_x = point.x;
            input->touch_y = point.y;
        }
    }
    for (int i = 0; i < press_count; i++) {
        if (frame >= presses[i].frame && frame < presses[i].frame + 4) input->buttons |= presses[i].button;
    }
}

/* Debug: the TV picture, to debug_screen.bmp next to the game. Called while the TV frame is
 * open: the tiled colour buffer is copied into a linear surface the CPU can read. */
static void save_screenshot(void) {
    GX2ColorBuffer* cb = WHBGfxGetTVColourBuffer();
    GX2Surface linear = cb->surface;
    linear.tileMode = GX2_TILE_MODE_LINEAR_ALIGNED;
    linear.aa = GX2_AA_MODE1X;
    linear.use = GX2_SURFACE_USE_TEXTURE;
    linear.image = NULL;
    linear.mipmaps = NULL;
    GX2CalcSurfaceSizeAndAlignment(&linear);
    linear.image = memalign(linear.alignment, linear.imageSize);
    if (linear.image == NULL) return;
    GX2Invalidate(GX2_INVALIDATE_MODE_CPU, linear.image, linear.imageSize);
    GX2CopySurface(&cb->surface, 0, 0, &linear, 0, 0);
    GX2DrawDone();
    DCInvalidateRange(linear.image, linear.imageSize);
    GX2SetContextState(WHBGfxGetTVContextState());

    char path[160];
    snprintf(path, sizeof(path), "%s/debug_screen.bmp", AppRoot);
    FILE* f = fopen(path, "wb");
    if (f != NULL) {
        const uint32_t w = linear.width, h = linear.height, row = w * 3, size = 54 + row * h;
        uint8_t header[54] = { 'B', 'M' };
        for (int i = 0; i < 4; i++) {
            header[2 + i] = (uint8_t) (size >> (8 * i));
            header[18 + i] = (uint8_t) (w >> (8 * i));
            header[22 + i] = (uint8_t) (h >> (8 * i));
        }
        header[10] = 54;
        header[14] = 40;
        header[26] = 1;
        header[28] = 24;
        fwrite(header, 1, sizeof(header), f);
        uint8_t* line = malloc(row);
        const uint8_t* image = linear.image;
        for (int y = (int) h - 1; y >= 0 && line != NULL; y--) {
            const uint8_t* src = image + (size_t) y * linear.pitch * 4; /* r, g, b, a */
            for (uint32_t x = 0; x < w; x++) {
                line[x * 3] = src[x * 4 + 2];
                line[x * 3 + 1] = src[x * 4 + 1];
                line[x * 3 + 2] = src[x * 4];
            }
            fwrite(line, 1, row, f);
        }
        free(line);
        fclose(f);
    }
    free(linear.image);
}

/* ---- Main loop ------------------------------------------------------------------------------ */

int main(int argc, char** argv) {
    (void) argc;
    (void) argv;
    WHBProcInit();
    VPADInit();
    OSReport("rfvp: starting\n");

    if (!gpu_init()) {
        OSReport("rfvp: the GPU could not be set up\n");
        WHBProcShutdown();
        return 1;
    }
    audio_init();
    read_debug_file();

    void* app = rfvp_wiiu_start(GameRoot);
    if (app == NULL) {
        /* The reason is in sd:/wiiu/apps/rfvp/last_error.txt; the screens stay dark red. */
        OSReport("rfvp: the game could not be started; see %s/last_error.txt\n", AppRoot);
        while (WHBProcIsRunning()) {
            WHBGfxBeginRender();
            WHBGfxBeginRenderTV();
            WHBGfxClearColor(0.3f, 0.0f, 0.0f, 1.0f);
            WHBGfxFinishRenderTV();
            WHBGfxBeginRenderDRC();
            WHBGfxClearColor(0.3f, 0.0f, 0.0f, 1.0f);
            WHBGfxFinishRenderDRC();
            WHBGfxFinishRender();
        }
    } else {
        unsigned frames = 0;
        bool running = true;
        while (running && WHBProcIsRunning()) {
            RfvpWiiuInput input;
            read_input(&input, frames);
            frame_open = false;
            int status = rfvp_wiiu_frame(app, &input);
            want_shot = frames == shot_frame;
            /* Presenting waits for the display, which paces the loop at 60 fps. */
            if (frame_open) present();
            audio_pump(app);
            frames++;
            if (status != 0) running = false;
        }
        rfvp_wiiu_stop(app);
    }

    audio_exit();
    GX2DrawDone();
    WHBGfxShutdown();
    VPADShutdown();
    WHBProcShutdown();
    return 0;
}
