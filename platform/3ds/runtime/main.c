/* The 3DS runtime: libctru owns the process, the buttons, sound (ndsp) and the screens; the rfvp
 * engine (Rust, linked in as librfvp_3ds.a) runs one frame per call and draws through the
 * functions below, on the GPU with citro3d. The bottom screen shows the log.
 *
 * The game is read from sdmc:/3ds/rfvp/game; saves go to sdmc:/3ds/rfvp/saves. */

#include <dirent.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>

#include <3ds.h>
#include <citro3d.h>

#include "sprite_shbin.h"

#define SCREEN_WIDTH 400
#define SCREEN_HEIGHT 240

/* libctru's default main-thread stack is 32 KiB; the engine needs more. */
u32 __stacksize__ = 1024 * 1024;

static const char* const GameRoot = "sdmc:/3ds/rfvp/game";
static const char* const SaveRoot = "sdmc:/3ds/rfvp";

/* `struct CtrInput` in event.rs. */
typedef struct {
    uint32_t buttons;
    uint8_t stick_x;
    uint8_t stick_y;
} RfvpCtrInput;

/* `struct CWallTime` in ffi.rs. */
typedef struct {
    uint16_t year;
    uint8_t month;
    uint8_t day;
    uint8_t day_of_week;
    uint8_t hour;
    uint8_t minute;
} RfvpCtrWallTime;

typedef struct Session Session;

/* The engine (platform/3ds/host/src/ffi.rs). */
Session* rfvp_3ds_start(const char* game_root, const char* save_root);
int rfvp_3ds_frame(Session* session, const RfvpCtrInput* input);
bool rfvp_3ds_cursor(Session* session, int32_t* x, int32_t* y);
void rfvp_3ds_audio_render(Session* session, int16_t* dst, size_t frames);
void rfvp_3ds_stop(Session* session);

/* ---- Services for the engine ---------------------------------------------------------------- */

void rfvp_3ds_log(const char* message, size_t len) {
    /* The bottom screen, and the emulator's log. */
    printf("%.*s", (int) len, message);
    svcOutputDebugString(message, (s32) len);
}

uint64_t rfvp_3ds_ticks_us(void) {
    return svcGetSystemTick() * 1000000ull / SYSCLOCK_ARM11;
}

/* The CPU cycle counter, for timing short stretches of code (see rfvp::perf). */
uint32_t rfvp_3ds_cycles(void) {
    return (uint32_t) svcGetSystemTick();
}

bool rfvp_3ds_wall_time(RfvpCtrWallTime* out) {
    time_t now = time(NULL);
    struct tm* local = gmtime(&now); /* the 3DS clock is local time already */
    if (local == NULL) return false;
    out->year = (uint16_t) (local->tm_year + 1900);
    out->month = (uint8_t) (local->tm_mon + 1);
    out->day = (uint8_t) local->tm_mday;
    out->day_of_week = (uint8_t) local->tm_wday;
    out->hour = (uint8_t) local->tm_hour;
    out->minute = (uint8_t) local->tm_min;
    return true;
}

typedef void (*RfvpDirVisitor)(void* ctx, const char* name, bool is_dir, uint64_t size);

/* List a directory; Rust does not know newlib's dirent and stat layouts. */
int rfvp_3ds_list_dir(const char* path, RfvpDirVisitor visit, void* ctx) {
    DIR* dir = opendir(path);
    if (dir == NULL) return -1;
    char child[512];
    struct dirent* entry;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) continue;
        if (entry->d_type == DT_DIR || entry->d_type == DT_REG) {
            /* Known from the listing: no stat per file (an asset folder holds thousands). */
            visit(ctx, entry->d_name, entry->d_type == DT_DIR, 0);
            continue;
        }
        snprintf(child, sizeof(child), "%s/%s", path, entry->d_name);
        struct stat st;
        if (stat(child, &st) != 0) continue;
        visit(ctx, entry->d_name, S_ISDIR(st.st_mode), (uint64_t) st.st_size);
    }
    closedir(dir);
    return 0;
}

/* 1 for a file, 2 for a directory, 0 if nothing is there. */
int rfvp_3ds_path_kind(const char* path, uint64_t* size) {
    struct stat st;
    if (stat(path, &st) != 0) return 0;
    if (size != NULL) *size = (uint64_t) st.st_size;
    return S_ISDIR(st.st_mode) ? 2 : 1;
}

/* ---- Drawing -------------------------------------------------------------------------------- */

#define DISPLAY_TRANSFER_FLAGS                                                                   \
    (GX_TRANSFER_FLIP_VERT(0) | GX_TRANSFER_OUT_TILED(0) | GX_TRANSFER_RAW_COPY(0) |              \
     GX_TRANSFER_IN_FORMAT(GX_TRANSFER_FMT_RGBA8) | GX_TRANSFER_OUT_FORMAT(GX_TRANSFER_FMT_RGB8) | \
     GX_TRANSFER_SCALING(GX_TRANSFER_SCALE_NO))

typedef struct {
    C3D_Tex tex;
    uint32_t width, height; /* the picture; the texture is rounded up to powers of two */
} GpuTexture;

static C3D_RenderTarget* top_target;
static DVLB_s* sprite_dvlb;
static shaderProgram_s sprite_program;
static int projection_location;
static C3D_Mtx projection;
static GpuTexture* white;
static bool frame_open;

/* Textures the engine dropped while the GPU may still draw with them: freed once it is done. */
static GpuTexture* dead_textures[256];
static int dead_count;

static uint32_t pow2_at_least(uint32_t v) {
    uint32_t p = 8; /* the GPU's smallest texture side */
    while (p < v) p <<= 1;
    return p;
}

/* Where texel (x, y) of a tiled texture `w` wide sits: 8x8 tiles, Morton order inside one. */
static inline uint32_t tiled_index(uint32_t x, uint32_t y, uint32_t w) {
    uint32_t tile = (y >> 3) * (w >> 3) + (x >> 3);
    uint32_t morton = (x & 1) | ((y & 1) << 1) | ((x & 2) << 1) | ((y & 2) << 2) | ((x & 4) << 2) |
                      ((y & 4) << 3);
    return (tile << 6) | morton;
}

/* Write RGBA8 rows into the texture at (x, y) of the picture. Tiled row 0 is sampled at t = 1:
 * texture rows run bottom-up in t, top-down in memory. */
static void copy_pixels(GpuTexture* t, uint32_t x, uint32_t y, uint32_t w, uint32_t h,
                        const uint8_t* rgba) {
    uint32_t* texels = (uint32_t*) t->tex.data;
    uint32_t tw = t->tex.width;
    for (uint32_t row = 0; row < h; row++) {
        uint32_t ty = y + row;
        const uint8_t* src = rgba + (size_t) row * w * 4;
        for (uint32_t col = 0; col < w; col++) {
            const uint8_t* p = src + col * 4;
            /* GPU_RGBA8 is 0xRRGGBBAA. */
            texels[tiled_index(x + col, ty, tw)] =
                ((uint32_t) p[0] << 24) | ((uint32_t) p[1] << 16) | ((uint32_t) p[2] << 8) | p[3];
        }
    }
    GSPGPU_FlushDataCache(t->tex.data, t->tex.size);
}

void rfvp_3ds_gpu_screen_size(uint32_t* width, uint32_t* height) {
    *width = SCREEN_WIDTH;
    *height = SCREEN_HEIGHT;
}

void* rfvp_3ds_gpu_create_texture(uint32_t width, uint32_t height, const uint8_t* rgba) {
    GpuTexture* t = calloc(1, sizeof(GpuTexture));
    if (t == NULL) return NULL;
    if (!C3D_TexInit(&t->tex, pow2_at_least(width), pow2_at_least(height), GPU_RGBA8)) {
        free(t);
        return NULL;
    }
    t->width = width;
    t->height = height;
    memset(t->tex.data, 0, t->tex.size);
    C3D_TexSetFilter(&t->tex, GPU_LINEAR, GPU_LINEAR);
    C3D_TexSetWrap(&t->tex, GPU_CLAMP_TO_EDGE, GPU_CLAMP_TO_EDGE);
    copy_pixels(t, 0, 0, width, height, rgba);
    return t;
}

void rfvp_3ds_gpu_update_texture(void* texture, uint32_t x, uint32_t y, uint32_t w, uint32_t h,
                                 const uint8_t* rgba) {
    copy_pixels((GpuTexture*) texture, x, y, w, h, rgba);
}

static void free_texture(GpuTexture* t) {
    C3D_TexDelete(&t->tex);
    free(t);
}

void rfvp_3ds_gpu_destroy_texture(void* texture) {
    if (dead_count < (int) (sizeof(dead_textures) / sizeof(dead_textures[0]))) {
        dead_textures[dead_count++] = (GpuTexture*) texture;
    } else {
        C3D_FrameSync();
        free_texture((GpuTexture*) texture);
    }
}

static void set_blend(int additive) {
    if (additive) {
        C3D_AlphaBlend(GPU_BLEND_ADD, GPU_BLEND_ADD, GPU_SRC_ALPHA, GPU_ONE, GPU_ZERO, GPU_ONE);
    } else {
        C3D_AlphaBlend(GPU_BLEND_ADD, GPU_BLEND_ADD, GPU_SRC_ALPHA, GPU_ONE_MINUS_SRC_ALPHA,
                       GPU_ONE, GPU_ONE_MINUS_SRC_ALPHA);
    }
}

void rfvp_3ds_gpu_begin_frame(void) {
    C3D_FrameBegin(C3D_FRAME_SYNCDRAW);
    /* The previous frame is done: the textures dropped during it can go. */
    for (int i = 0; i < dead_count; i++) free_texture(dead_textures[i]);
    dead_count = 0;
    C3D_RenderTargetClear(top_target, C3D_CLEAR_ALL, 0x000000FF, 0);
    C3D_FrameDrawOn(top_target);
    C3D_BindProgram(&sprite_program);
    C3D_FVUnifMtx4x4(GPU_VERTEX_SHADER, projection_location, &projection);
    frame_open = true;
}

void rfvp_3ds_gpu_set_clip(int32_t x0, int32_t y0, int32_t x1, int32_t y1) {
    if (x1 <= x0 || y1 <= y0) {
        x0 = y0 = 0;
        x1 = y1 = 1;
    }
    /* The top screen's frame buffer is 240x400, turned a quarter. */
    C3D_SetScissor(GPU_SCISSOR_NORMAL, SCREEN_HEIGHT - y1, SCREEN_WIDTH - x1, SCREEN_HEIGHT - y0,
                   SCREEN_WIDTH - x0);
}

static void send_vertex(float x, float y, float s, float t, uint32_t rgba) {
    C3D_ImmSendAttrib(x, y, 0.5f, 0.0f);
    C3D_ImmSendAttrib(s, t, 0.0f, 0.0f);
    C3D_ImmSendAttrib((rgba & 0xff) / 255.0f, ((rgba >> 8) & 0xff) / 255.0f,
                      ((rgba >> 16) & 0xff) / 255.0f, (rgba >> 24) / 255.0f);
}

/* `c`: screen x, y and texel u, v for each corner, in triangle-strip order. */
void rfvp_3ds_gpu_draw_sprite(void* texture, const float* c, uint32_t rgba, int additive) {
    GpuTexture* t = texture;
    float sw = 1.0f / t->tex.width, sh = 1.0f / t->tex.height;
    C3D_TexBind(0, &t->tex);
    set_blend(additive);
    C3D_ImmDrawBegin(GPU_TRIANGLE_STRIP);
    for (int i = 0; i < 4; i++) {
        /* t runs bottom-up (see copy_pixels). */
        send_vertex(c[i * 4], c[i * 4 + 1], c[i * 4 + 2] * sw,
                    1.0f - c[i * 4 + 3] * sh, rgba);
    }
    C3D_ImmDrawEnd();
}

void rfvp_3ds_gpu_draw_rect(int32_t x0, int32_t y0, int32_t x1, int32_t y1, uint32_t rgba,
                            int additive) {
    float c[16] = { x0, y0, 0, 0, x0, y1, 0, 1, x1, y0, 1, 0, x1, y1, 1, 1 };
    rfvp_3ds_gpu_draw_sprite(white, c, rgba, additive);
}

void rfvp_3ds_gpu_end_frame(void) {
}

/* The pointer, when the circle pad has shown it: a small white arrow with a dark outline. */
static void draw_cursor(Session* session) {
    int32_t x, y;
    if (!frame_open || !rfvp_3ds_cursor(session, &x, &y)) return;
    float fx = (float) x, fy = (float) y;
    rfvp_3ds_gpu_set_clip(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT);
    C3D_TexBind(0, &white->tex);
    set_blend(0);
    const float shapes[2][6] = {
        { fx - 1, fy - 2, fx - 1, fy + 12, fx + 10, fy + 9 },
        { fx, fy, fx, fy + 9, fx + 7, fy + 7 },
    };
    const uint32_t colors[2] = { 0xff000000u, 0xffffffffu };
    for (int s = 0; s < 2; s++) {
        C3D_ImmDrawBegin(GPU_TRIANGLES);
        for (int v = 0; v < 3; v++) {
            send_vertex(shapes[s][v * 2], shapes[s][v * 2 + 1], 0.5f, 0.5f, colors[s]);
        }
        C3D_ImmDrawEnd();
    }
}

static void gpu_init(void) {
    C3D_Init(C3D_DEFAULT_CMDBUF_SIZE);
    top_target = C3D_RenderTargetCreate(SCREEN_HEIGHT, SCREEN_WIDTH, GPU_RB_RGBA8, GPU_RB_DEPTH24_STENCIL8);
    C3D_RenderTargetSetOutput(top_target, GFX_TOP, GFX_LEFT, DISPLAY_TRANSFER_FLAGS);

    sprite_dvlb = DVLB_ParseFile((u32*) sprite_shbin, sprite_shbin_size);
    shaderProgramInit(&sprite_program);
    shaderProgramSetVsh(&sprite_program, &sprite_dvlb->DVLE[0]);
    C3D_BindProgram(&sprite_program);
    projection_location = shaderInstanceGetUniformLocation(sprite_program.vertexShader, "projection");

    C3D_AttrInfo* attr = C3D_GetAttrInfo();
    AttrInfo_Init(attr);
    AttrInfo_AddLoader(attr, 0, GPU_FLOAT, 3); /* position */
    AttrInfo_AddLoader(attr, 1, GPU_FLOAT, 2); /* texture coordinates */
    AttrInfo_AddLoader(attr, 2, GPU_FLOAT, 4); /* color */

    /* Screen pixels, origin at the top left (the screen is turned a quarter). */
    Mtx_OrthoTilt(&projection, 0.0f, SCREEN_WIDTH, SCREEN_HEIGHT, 0.0f, 0.0f, 1.0f, true);

    C3D_TexEnv* env = C3D_GetTexEnv(0);
    C3D_TexEnvInit(env);
    C3D_TexEnvSrc(env, C3D_Both, GPU_TEXTURE0, GPU_PRIMARY_COLOR, 0);
    C3D_TexEnvFunc(env, C3D_Both, GPU_MODULATE);

    C3D_DepthTest(false, GPU_ALWAYS, GPU_WRITE_COLOR);
    C3D_CullFace(GPU_CULL_NONE);

    static uint8_t white_pixels[8 * 8 * 4];
    memset(white_pixels, 0xff, sizeof(white_pixels));
    white = rfvp_3ds_gpu_create_texture(8, 8, white_pixels);
}

/* ---- Sound ---------------------------------------------------------------------------------- */

enum { AudioBufferFrames = 1024, AudioBufferCount = 4 };
static ndspWaveBuf wave_bufs[AudioBufferCount];
static int16_t* audio_memory;
static bool audio_ready;

static void audio_init(void) {
    if (R_FAILED(ndspInit())) {
        printf("rfvp: ndsp did not start (no dspfirm.cdc on a console?): no sound\n");
        return;
    }
    audio_memory = linearAlloc(AudioBufferFrames * 2 * sizeof(int16_t) * AudioBufferCount);
    if (audio_memory == NULL) {
        ndspExit();
        return;
    }
    memset(audio_memory, 0, AudioBufferFrames * 2 * sizeof(int16_t) * AudioBufferCount);
    ndspSetOutputMode(NDSP_OUTPUT_STEREO);
    ndspChnReset(0);
    ndspChnSetInterp(0, NDSP_INTERP_LINEAR);
    ndspChnSetRate(0, 48000.0f); /* the mixer's rate (audio.rs); the DSP resamples */
    ndspChnSetFormat(0, NDSP_FORMAT_STEREO_PCM16);
    memset(wave_bufs, 0, sizeof(wave_bufs));
    for (int i = 0; i < AudioBufferCount; i++) {
        wave_bufs[i].data_vaddr = audio_memory + i * AudioBufferFrames * 2;
        wave_bufs[i].nsamples = AudioBufferFrames;
        wave_bufs[i].status = NDSP_WBUF_DONE;
    }
    audio_ready = true;
}

/* Refill the buffers that have played out. */
static void audio_pump(Session* session) {
    if (!audio_ready) return;
    for (int i = 0; i < AudioBufferCount; i++) {
        if (wave_bufs[i].status != NDSP_WBUF_DONE) continue;
        int16_t* data = (int16_t*) wave_bufs[i].data_vaddr;
        rfvp_3ds_audio_render(session, data, AudioBufferFrames);
        DSP_FlushDataCache(data, AudioBufferFrames * 2 * sizeof(int16_t));
        ndspChnWaveBufAdd(0, &wave_bufs[i]);
    }
}

/* ---- Input ---------------------------------------------------------------------------------- */

/* Debug: button presses scripted in debug.txt (`press=a@600`), for testing without hands. */
#define MAX_PRESSES 32
static struct { uint32_t button; unsigned frame; } presses[MAX_PRESSES];
static int press_count;
static unsigned shot_frame = 0xffffffff;

static void read_debug_file(void) {
    static const struct { const char* name; uint32_t bit; } names[] = {
        { "a", KEY_A }, { "b", KEY_B }, { "x", KEY_X }, { "y", KEY_Y }, { "up", KEY_DUP },
        { "down", KEY_DDOWN }, { "left", KEY_DLEFT }, { "right", KEY_DRIGHT },
        { "start", KEY_START }, { "select", KEY_SELECT }, { "l", KEY_L }, { "r", KEY_R },
    };
    char path[128];
    snprintf(path, sizeof(path), "%s/debug.txt", GameRoot);
    FILE* f = fopen(path, "r");
    if (f == NULL) return;
    char line[64], name[16];
    unsigned frame;
    while (fgets(line, sizeof(line), f) != NULL && press_count < MAX_PRESSES) {
        if (sscanf(line, "shot=%u", &frame) == 1) shot_frame = frame;
        if (sscanf(line, "press=%15[a-z]@%u", name, &frame) != 2) continue;
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

static void read_input(RfvpCtrInput* input, unsigned frame) {
    hidScanInput();
    input->buttons = hidKeysHeld();
    for (int i = 0; i < press_count; i++) {
        if (frame >= presses[i].frame && frame < presses[i].frame + 4) input->buttons |= presses[i].button;
    }
    circlePosition pos;
    hidCircleRead(&pos);
    /* -156..156, up positive: to 0..255 with up low, as the other ports' sticks. */
    int x = 128 + pos.dx * 127 / 156;
    int y = 128 - pos.dy * 127 / 156;
    input->stick_x = (uint8_t) (x < 0 ? 0 : x > 255 ? 255 : x);
    input->stick_y = (uint8_t) (y < 0 ? 0 : y > 255 ? 255 : y);
}

/* Debug: the top screen as it is shown, to debug_screen.bmp in the save folder. */
static void save_screenshot(void) {
    u16 fw, fh;
    const u8* fb = gfxGetFramebuffer(GFX_TOP, GFX_LEFT, &fw, &fh); /* 240x400, BGR, turned */
    char path[128];
    snprintf(path, sizeof(path), "%s/debug_screen.bmp", SaveRoot);
    FILE* f = fopen(path, "wb");
    if (f == NULL) return;
    const uint32_t w = fh, h = fw, row = w * 3, size = 54 + row * h;
    uint8_t header[54] = { 'B', 'M' };
    memcpy(header + 2, &size, 4);
    header[10] = 54;
    header[14] = 40;
    memcpy(header + 18, &w, 4);
    memcpy(header + 22, &h, 4);
    header[26] = 1;
    header[28] = 24;
    fwrite(header, 1, sizeof(header), f);
    static uint8_t line[400 * 3];
    /* BMP rows go bottom up; frame buffer column x of the screen holds its pixels bottom up. */
    for (uint32_t y = 0; y < h; y++) {
        for (uint32_t x = 0; x < w; x++) memcpy(line + x * 3, fb + (x * fw + y) * 3, 3);
        fwrite(line, 1, row, f);
    }
    fclose(f);
}

/* ---- Main ----------------------------------------------------------------------------------- */

int main(void) {
    gfxInitDefault();
    consoleInit(GFX_BOTTOM, NULL);
    osSetSpeedupEnable(true); /* the New 3DS's faster clock */
    printf("rfvp: starting\n");

    gpu_init();
    audio_init();
    read_debug_file();

    Session* session = rfvp_3ds_start(GameRoot, SaveRoot);
    if (session == NULL) {
        printf("rfvp: the game could not be started;\nsee %s/last_error.txt\n\nSTART: quit\n", SaveRoot);
        while (aptMainLoop()) {
            hidScanInput();
            if (hidKeysDown() & KEY_START) break;
            gspWaitForVBlank();
        }
    } else {
        unsigned frames = 0;
        while (aptMainLoop()) {
            RfvpCtrInput input;
            read_input(&input, frames);
            frame_open = false;
            int status = rfvp_3ds_frame(session, &input);
            draw_cursor(session);
            if (frame_open) C3D_FrameEnd(0);
            if (frames == shot_frame) {
                gspWaitForVBlank();
                gspWaitForVBlank();
                save_screenshot();
            }
            audio_pump(session);
            frames++;
            if (status != 0) break;
        }
        rfvp_3ds_stop(session);
    }

    if (audio_ready) {
        ndspExit();
        linearFree(audio_memory);
    }
    C3D_Fini();
    gfxExit();
    return 0;
}
