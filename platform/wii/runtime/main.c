/* The Wii runtime: libogc owns the process, the controllers (WPAD, PAD), sound (ASND), the SD
 * card (libfat) and the GPU (GX); the rfvp engine (Rust, linked in as librfvp_wii.a) runs one
 * frame per call and draws through the functions below.
 *
 * The game is read from sd:/apps/rfvp/game; saves go to sd:/apps/rfvp/saves. */

#include <dirent.h>
#include <malloc.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>

#include <asndlib.h>
#include <fat.h>
#include <gccore.h>
#include <ogc/lwp_watchdog.h>
#include <wiiuse/wpad.h>

#define SCREEN_WIDTH 640
#define SCREEN_HEIGHT 480

static const char* const GameRoot = "sd:/apps/rfvp/game";
static const char* const SaveRoot = "sd:/apps/rfvp";

/* `struct WiiInput` in event.rs. */
typedef struct {
    uint32_t buttons;
    uint8_t stick_x;
    uint8_t stick_y;
    uint8_t pointer_valid;
    uint8_t pad;
    int16_t pointer_x;
    int16_t pointer_y;
} RfvpWiiInput;

/* event.rs's merged buttons. */
enum {
    IN_A = 1 << 0, IN_B = 1 << 1, IN_ONE = 1 << 2, IN_TWO = 1 << 3, IN_PLUS = 1 << 4,
    IN_MINUS = 1 << 5, IN_HOME = 1 << 6, IN_UP = 1 << 7, IN_DOWN = 1 << 8, IN_LEFT = 1 << 9,
    IN_RIGHT = 1 << 10, IN_SKIP = 1 << 11,
};

/* `struct CWallTime` in ffi.rs. */
typedef struct {
    uint16_t year;
    uint8_t month;
    uint8_t day;
    uint8_t day_of_week;
    uint8_t hour;
    uint8_t minute;
} RfvpWiiWallTime;

typedef struct Session Session;

/* The engine (platform/wii/host/src/ffi.rs). */
void rfvp_wii_heap_init(void* base, size_t size);
Session* rfvp_wii_start(const char* game_root, const char* save_root);
int rfvp_wii_frame(Session* session, const RfvpWiiInput* input);
bool rfvp_wii_cursor(Session* session, int32_t* x, int32_t* y);
void rfvp_wii_audio_render(Session* session, int16_t* dst, size_t frames);
void rfvp_wii_stop(Session* session);

/* ---- Services for the engine ---------------------------------------------------------------- */

static FILE* log_file;

void rfvp_wii_log(const char* message, size_t len) {
    /* Dolphin and a USB Gecko show stdout; with debug.txt it also goes to log.txt. */
    printf("%.*s", (int) len, message);
    if (log_file != NULL) {
        fwrite(message, 1, len, log_file);
        fflush(log_file);
    }
}

uint64_t rfvp_wii_ticks_us(void) {
    return ticks_to_microsecs(gettime());
}

/* The CPU's time base, for timing short stretches of code (see rfvp::perf). */
uint32_t rfvp_wii_cycles(void) {
    return (uint32_t) gettime();
}

bool rfvp_wii_wall_time(RfvpWiiWallTime* out) {
    time_t now = time(NULL);
    struct tm* local = localtime(&now);
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
int rfvp_wii_list_dir(const char* path, RfvpDirVisitor visit, void* ctx) {
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
int rfvp_wii_path_kind(const char* path, uint64_t* size) {
    struct stat st;
    if (stat(path, &st) != 0) return 0;
    if (size != NULL) *size = (uint64_t) st.st_size;
    return S_ISDIR(st.st_mode) ? 2 : 1;
}

/* ---- Drawing -------------------------------------------------------------------------------- */

typedef struct {
    GXTexObj obj;
    void* data;
    uint32_t width, height; /* the picture; rows and columns are padded to the 4x4 tiles */
    uint32_t size;
} GpuTexture;

#define FIFO_SIZE (256 * 1024)
static GXRModeObj* rmode;
static void* xfb[2];
static int fb;
static GpuTexture* white;
static bool frame_open, textures_dirty, widescreen;
static float efb_scale_y = 1.0f; /* EFB rows per picture row (528/480 in 50 Hz PAL) */

/* Textures the engine dropped during a frame: the GPU may still read them until it is done. */
static GpuTexture* dead_textures[512];
static int dead_count;

void rfvp_wii_gpu_screen_size(uint32_t* width, uint32_t* height) {
    *width = SCREEN_WIDTH;
    *height = SCREEN_HEIGHT;
}

/* Whether the console is set to 16:9 (the TV then stretches the 640x480 picture). */
bool rfvp_wii_gpu_widescreen(void) {
    return widescreen;
}

/* Write RGBA8 rows (w x h at x, y) into GX's RGBA8 layout: 4x4 tiles of 64 bytes, the 16
 * pixels' alpha and red first, then their green and blue. */
static void copy_pixels(GpuTexture* t, uint32_t x, uint32_t y, uint32_t w, uint32_t h,
                        const uint8_t* rgba) {
    uint8_t* dst = t->data;
    uint32_t tiles_per_row = (t->width + 3) / 4;
    for (uint32_t row = 0; row < h; row++) {
        uint32_t ty = y + row;
        const uint8_t* src = rgba + (size_t) row * w * 4;
        for (uint32_t col = 0; col < w; col++) {
            uint32_t tx = x + col;
            uint8_t* tile = dst + ((ty / 4) * tiles_per_row + tx / 4) * 64;
            uint32_t at = ((ty & 3) * 4 + (tx & 3)) * 2;
            const uint8_t* p = src + col * 4;
            tile[at] = p[3];
            tile[at + 1] = p[0];
            tile[at + 32] = p[1];
            tile[at + 33] = p[2];
        }
    }
    DCFlushRange(t->data, t->size);
    textures_dirty = true;
}

void* rfvp_wii_gpu_create_texture(uint32_t width, uint32_t height, const uint8_t* rgba) {
    GpuTexture* t = calloc(1, sizeof(GpuTexture));
    if (t == NULL) return NULL;
    t->width = width;
    t->height = height;
    t->size = ((width + 3) / 4) * ((height + 3) / 4) * 64;
    t->data = memalign(32, t->size);
    if (t->data == NULL) {
        free(t);
        return NULL;
    }
    memset(t->data, 0, t->size);
    if (rgba != NULL) {
        copy_pixels(t, 0, 0, width, height, rgba);
    } else {
        DCFlushRange(t->data, t->size);
    }
    GX_InitTexObj(&t->obj, t->data, (u16) width, (u16) height, GX_TF_RGBA8, GX_CLAMP, GX_CLAMP,
                  GX_FALSE);
    GX_InitTexObjLOD(&t->obj, GX_LINEAR, GX_LINEAR, 0.0f, 0.0f, 0.0f, GX_FALSE, GX_FALSE,
                     GX_ANISO_1);
    return t;
}

void rfvp_wii_gpu_update_texture(void* texture, uint32_t x, uint32_t y, uint32_t w, uint32_t h,
                                 const uint8_t* rgba) {
    copy_pixels((GpuTexture*) texture, x, y, w, h, rgba);
}

static void free_texture(GpuTexture* t) {
    free(t->data);
    free(t);
}

void rfvp_wii_gpu_destroy_texture(void* texture) {
    if (!frame_open) {
        free_texture(texture);
    } else if (dead_count < (int) (sizeof(dead_textures) / sizeof(dead_textures[0]))) {
        dead_textures[dead_count++] = texture;
    } else {
        GX_DrawDone();
        free_texture(texture);
    }
}

static void set_blend(int additive) {
    GX_SetBlendMode(GX_BM_BLEND, GX_BL_SRCALPHA, additive ? GX_BL_ONE : GX_BL_INVSRCALPHA,
                    GX_LO_CLEAR);
}

void rfvp_wii_gpu_begin_frame(void) {
    frame_open = true;
    GX_SetViewport(0, 0, rmode->fbWidth, rmode->efbHeight, 0, 1);
    GX_SetScissor(0, 0, rmode->fbWidth, rmode->efbHeight);
}

void rfvp_wii_gpu_set_clip(int32_t x0, int32_t y0, int32_t x1, int32_t y1) {
    if (x1 <= x0 || y1 <= y0) {
        x0 = y0 = 0;
        x1 = y1 = 1;
    }
    uint32_t ey0 = (uint32_t) (y0 * efb_scale_y), ey1 = (uint32_t) (y1 * efb_scale_y + 0.5f);
    GX_SetScissor((u32) x0, ey0, (u32) (x1 - x0), ey1 - ey0);
}

/* `c`: screen x, y and texel u, v for each corner, in triangle-strip order. */
void rfvp_wii_gpu_draw_sprite(void* texture, const float* c, uint32_t rgba, int additive) {
    GpuTexture* t = texture;
    if (textures_dirty) {
        GX_InvalidateTexAll();
        textures_dirty = false;
    }
    GX_LoadTexObj(&t->obj, GX_TEXMAP0);
    set_blend(additive);
    float sw = 1.0f / t->width, sh = 1.0f / t->height;
    uint8_t r = rgba, g = rgba >> 8, b = rgba >> 16, a = rgba >> 24;
    GX_Begin(GX_TRIANGLESTRIP, GX_VTXFMT0, 4);
    for (int i = 0; i < 4; i++) {
        GX_Position2f32(c[i * 4], c[i * 4 + 1]);
        GX_Color4u8(r, g, b, a);
        GX_TexCoord2f32(c[i * 4 + 2] * sw, c[i * 4 + 3] * sh);
    }
    GX_End();
}

void rfvp_wii_gpu_draw_rect(int32_t x0, int32_t y0, int32_t x1, int32_t y1, uint32_t rgba,
                            int additive) {
    float c[16] = { x0, y0, 0, 0, x0, y1, 0, 8, x1, y0, 8, 0, x1, y1, 8, 8 };
    rfvp_wii_gpu_draw_sprite(white, c, rgba, additive);
}

void rfvp_wii_gpu_end_frame(void) {
}

/* The pointer, when shown: a small white arrow with a dark outline. */
static void draw_cursor(Session* session) {
    int32_t x, y;
    if (!frame_open || !rfvp_wii_cursor(session, &x, &y)) return;
    float fx = (float) x, fy = (float) y;
    rfvp_wii_gpu_set_clip(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT);
    GX_LoadTexObj(&white->obj, GX_TEXMAP0);
    set_blend(0);
    const float shapes[2][6] = {
        { fx - 2, fy - 3, fx - 2, fy + 17, fx + 14, fy + 13 },
        { fx, fy, fx, fy + 13, fx + 10, fy + 10 },
    };
    const uint8_t shade[2] = { 0, 255 };
    for (int s = 0; s < 2; s++) {
        GX_Begin(GX_TRIANGLES, GX_VTXFMT0, 3);
        for (int v = 0; v < 3; v++) {
            GX_Position2f32(shapes[s][v * 2], shapes[s][v * 2 + 1]);
            GX_Color4u8(shade[s], shade[s], shade[s], 255);
            GX_TexCoord2f32(0.5f, 0.5f);
        }
        GX_End();
    }
}

static bool want_shot;
static void save_screenshot(void);

/* Show the frame: copy the EFB to the next XFB (clearing it) and wait for the display. */
static void present(void) {
    GX_DrawDone();
    if (want_shot) {
        save_screenshot();
        want_shot = false;
    }
    for (int i = 0; i < dead_count; i++) free_texture(dead_textures[i]);
    dead_count = 0;
    fb ^= 1;
    GX_SetZMode(GX_FALSE, GX_ALWAYS, GX_FALSE);
    GX_SetColorUpdate(GX_TRUE);
    GX_CopyDisp(xfb[fb], GX_TRUE);
    GX_DrawDone();
    VIDEO_SetNextFramebuffer(xfb[fb]);
    VIDEO_Flush();
    VIDEO_WaitVSync();
}

static void gpu_init(void) {
    VIDEO_Init();
    rmode = VIDEO_GetPreferredMode(NULL);
    widescreen = CONF_GetAspectRatio() == CONF_ASPECT_16_9;
    for (int i = 0; i < 2; i++) xfb[i] = MEM_K0_TO_K1(SYS_AllocateFramebuffer(rmode));
    VIDEO_Configure(rmode);
    VIDEO_SetNextFramebuffer(xfb[0]);
    VIDEO_SetBlack(FALSE);
    VIDEO_Flush();
    VIDEO_WaitVSync();
    if (rmode->viTVMode & VI_NON_INTERLACE) VIDEO_WaitVSync();

    void* fifo = memalign(32, FIFO_SIZE);
    memset(fifo, 0, FIFO_SIZE);
    GX_Init(fifo, FIFO_SIZE);
    GXColor black = { 0, 0, 0, 255 };
    GX_SetCopyClear(black, 0x00ffffff);
    GX_SetViewport(0, 0, rmode->fbWidth, rmode->efbHeight, 0, 1);
    f32 yscale = GX_GetYScaleFactor(rmode->efbHeight, rmode->xfbHeight);
    u32 xfb_height = GX_SetDispCopyYScale(yscale);
    GX_SetScissor(0, 0, rmode->fbWidth, rmode->efbHeight);
    GX_SetDispCopySrc(0, 0, rmode->fbWidth, rmode->efbHeight);
    GX_SetDispCopyDst(rmode->fbWidth, xfb_height);
    GX_SetCopyFilter(rmode->aa, rmode->sample_pattern, GX_TRUE, rmode->vfilter);
    GX_SetFieldMode(rmode->field_rendering,
                    (rmode->viHeight == 2 * rmode->xfbHeight) ? GX_ENABLE : GX_DISABLE);
    GX_SetPixelFmt(GX_PF_RGB8_Z24, GX_ZC_LINEAR);
    GX_SetCullMode(GX_CULL_NONE);
    GX_CopyDisp(xfb[fb], GX_TRUE);
    GX_SetDispCopyGamma(GX_GM_1_0);
    efb_scale_y = (float) rmode->efbHeight / SCREEN_HEIGHT;

    GX_ClearVtxDesc();
    GX_SetVtxDesc(GX_VA_POS, GX_DIRECT);
    GX_SetVtxDesc(GX_VA_CLR0, GX_DIRECT);
    GX_SetVtxDesc(GX_VA_TEX0, GX_DIRECT);
    GX_SetVtxAttrFmt(GX_VTXFMT0, GX_VA_POS, GX_POS_XY, GX_F32, 0);
    GX_SetVtxAttrFmt(GX_VTXFMT0, GX_VA_CLR0, GX_CLR_RGBA, GX_RGBA8, 0);
    GX_SetVtxAttrFmt(GX_VTXFMT0, GX_VA_TEX0, GX_TEX_ST, GX_F32, 0);
    GX_SetNumChans(1);
    GX_SetChanCtrl(GX_COLOR0A0, GX_DISABLE, GX_SRC_REG, GX_SRC_VTX, GX_LIGHTNULL, GX_DF_NONE,
                   GX_AF_NONE);
    GX_SetNumTexGens(1);
    GX_SetTexCoordGen(GX_TEXCOORD0, GX_TG_MTX2x4, GX_TG_TEX0, GX_IDENTITY);
    GX_SetNumTevStages(1);
    GX_SetTevOrder(GX_TEVSTAGE0, GX_TEXCOORD0, GX_TEXMAP0, GX_COLOR0A0);
    GX_SetTevOp(GX_TEVSTAGE0, GX_MODULATE);
    GX_SetZMode(GX_FALSE, GX_ALWAYS, GX_FALSE);
    GX_SetAlphaUpdate(GX_TRUE);
    GX_SetColorUpdate(GX_TRUE);

    /* Picture pixels (640x480), whatever the EFB's height. The quads (z = 0) are moved to
     * z = -100, inside the depth range: at z = 0 they would sit on the near plane and be
     * clipped. */
    Mtx44 ortho;
    guOrtho(ortho, 0, SCREEN_HEIGHT, 0, SCREEN_WIDTH, 0, 1000.0f);
    GX_LoadProjectionMtx(ortho, GX_ORTHOGRAPHIC);
    Mtx model;
    guMtxIdentity(model);
    guMtxTransApply(model, model, 0.0f, 0.0f, -100.0f);
    GX_LoadPosMtxImm(model, GX_PNMTX0);
    GX_SetCurrentMtx(GX_PNMTX0);

    static uint8_t white_pixels[8 * 8 * 4];
    memset(white_pixels, 0xff, sizeof(white_pixels));
    white = rfvp_wii_gpu_create_texture(8, 8, white_pixels);
}

/* ---- Sound ---------------------------------------------------------------------------------- */

/* One ASND voice of 48 kHz 16-bit stereo, fed two buffers at a time from the mixer. */
enum { AudioBufferFrames = 2048, AudioBufferCount = 3 };
static int16_t* audio_buffers[AudioBufferCount];
static int audio_next;
static bool audio_ready, audio_started, audio_muted;

static void audio_init(void) {
    ASND_Init();
    ASND_Pause(0);
    for (int i = 0; i < AudioBufferCount; i++) {
        audio_buffers[i] = memalign(32, AudioBufferFrames * 4);
        if (audio_buffers[i] == NULL) return;
        memset(audio_buffers[i], 0, AudioBufferFrames * 4);
    }
    audio_ready = true;
}

static int16_t* audio_fill(Session* session) {
    int16_t* buffer = audio_buffers[audio_next];
    audio_next = (audio_next + 1) % AudioBufferCount;
    rfvp_wii_audio_render(session, buffer, AudioBufferFrames);
    DCFlushRange(buffer, AudioBufferFrames * 4);
    return buffer;
}

/* Keep the voice fed: it plays one buffer and holds the next. */
static void audio_pump(Session* session) {
    if (!audio_ready || audio_muted) return;
    if (!audio_started || ASND_StatusVoice(0) == SND_UNUSED) {
        ASND_SetVoice(0, VOICE_STEREO_16BIT, 48000, 0, audio_fill(session), AudioBufferFrames * 4,
                      MAX_VOLUME, MAX_VOLUME, NULL);
        audio_started = true;
    }
    /* At most one buffer a frame (2048 frames outlast a 60 Hz frame): the voice reports ready
     * until it has taken the buffer, and a refused one must not spin the loop. */
    if (ASND_TestVoiceBufferReady(0)) {
        ASND_AddVoice(0, audio_fill(session), AudioBufferFrames * 4);
    }
}

/* ---- Input ---------------------------------------------------------------------------------- */

/* Debug: scripted input in debug.txt, for testing without hands: `press=a@600` presses a button
 * for 4 frames, `point=213,357@400` points the Wii Remote there from frame 400 on (`point=off@N`
 * stops), `shot=600` saves the picture, `quit=900` ends the program (so an emulator writes its
 * SD folder back). */
#define MAX_PRESSES 64
static struct { uint32_t button; unsigned frame; } presses[MAX_PRESSES];
static int press_count;
#define MAX_POINTS 16
static struct { int x, y; unsigned frame; } points[MAX_POINTS];
static int point_count;
static unsigned shot_frame = 0xffffffffu, quit_frame = 0xffffffffu;
static bool debug_on;
static volatile bool power_pressed;

static void read_debug_file(void) {
    static const struct { const char* name; uint32_t bit; } names[] = {
        { "a", IN_A }, { "b", IN_B }, { "one", IN_ONE }, { "two", IN_TWO }, { "plus", IN_PLUS },
        { "minus", IN_MINUS }, { "home", IN_HOME }, { "up", IN_UP }, { "down", IN_DOWN },
        { "left", IN_LEFT }, { "right", IN_RIGHT }, { "skip", IN_SKIP },
    };
    char path[128];
    snprintf(path, sizeof(path), "%s/debug.txt", SaveRoot);
    FILE* f = fopen(path, "r");
    if (f == NULL) return;
    debug_on = true;
    char line[64], name[16];
    unsigned frame;
    int x, y;
    while (fgets(line, sizeof(line), f) != NULL) {
        if (sscanf(line, "shot=%u", &frame) == 1) shot_frame = frame;
        if (sscanf(line, "quit=%u", &frame) == 1) quit_frame = frame;
        if (strncmp(line, "mute", 4) == 0) audio_muted = true;
        if (point_count < MAX_POINTS && sscanf(line, "point=%d,%d@%u", &x, &y, &frame) == 3) {
            points[point_count].x = x;
            points[point_count].y = y;
            points[point_count].frame = frame;
            point_count++;
        }
        if (point_count < MAX_POINTS && sscanf(line, "point=off@%u", &frame) == 1) {
            points[point_count].x = -1;
            points[point_count].frame = frame;
            point_count++;
        }
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

static void read_input(RfvpWiiInput* input, unsigned frame) {
    memset(input, 0, sizeof(*input));
    input->stick_x = input->stick_y = 128;

    WPAD_ScanPads();
    uint32_t w = WPAD_ButtonsHeld(WPAD_CHAN_0);
    static const struct { uint32_t wpad, in; } wmap[] = {
        { WPAD_BUTTON_A, IN_A }, { WPAD_BUTTON_B, IN_B }, { WPAD_BUTTON_1, IN_ONE },
        { WPAD_BUTTON_2, IN_TWO }, { WPAD_BUTTON_PLUS, IN_PLUS }, { WPAD_BUTTON_MINUS, IN_MINUS },
        { WPAD_BUTTON_HOME, IN_HOME }, { WPAD_BUTTON_UP, IN_UP }, { WPAD_BUTTON_DOWN, IN_DOWN },
        { WPAD_BUTTON_LEFT, IN_LEFT }, { WPAD_BUTTON_RIGHT, IN_RIGHT },
        /* Nunchuk Z and the Classic Controller. */
        { WPAD_NUNCHUK_BUTTON_Z, IN_SKIP }, { WPAD_CLASSIC_BUTTON_A, IN_A },
        { WPAD_CLASSIC_BUTTON_B, IN_B }, { WPAD_CLASSIC_BUTTON_X, IN_ONE },
        { WPAD_CLASSIC_BUTTON_Y, IN_TWO }, { WPAD_CLASSIC_BUTTON_PLUS, IN_PLUS },
        { WPAD_CLASSIC_BUTTON_MINUS, IN_MINUS }, { WPAD_CLASSIC_BUTTON_HOME, IN_HOME },
        { WPAD_CLASSIC_BUTTON_UP, IN_UP }, { WPAD_CLASSIC_BUTTON_DOWN, IN_DOWN },
        { WPAD_CLASSIC_BUTTON_LEFT, IN_LEFT }, { WPAD_CLASSIC_BUTTON_RIGHT, IN_RIGHT },
        { WPAD_CLASSIC_BUTTON_ZL | WPAD_CLASSIC_BUTTON_ZR | WPAD_CLASSIC_BUTTON_FULL_L |
              WPAD_CLASSIC_BUTTON_FULL_R, IN_SKIP },
    };
    for (unsigned i = 0; i < sizeof(wmap) / sizeof(wmap[0]); i++) {
        if (w & wmap[i].wpad) input->buttons |= wmap[i].in;
    }
    WPADData* data = WPAD_Data(WPAD_CHAN_0);
    if (data != NULL && data->ir.valid) {
        input->pointer_valid = 1;
        input->pointer_x = (int16_t) data->ir.x;
        input->pointer_y = (int16_t) data->ir.y;
    }

    PAD_ScanPads();
    uint32_t p = PAD_ButtonsHeld(0);
    static const struct { uint32_t pad, in; } pmap[] = {
        { PAD_BUTTON_A, IN_A }, { PAD_BUTTON_B, IN_B }, { PAD_BUTTON_X, IN_ONE },
        { PAD_BUTTON_Y, IN_TWO }, { PAD_BUTTON_START, IN_PLUS }, { PAD_BUTTON_UP, IN_UP },
        { PAD_BUTTON_DOWN, IN_DOWN }, { PAD_BUTTON_LEFT, IN_LEFT },
        { PAD_BUTTON_RIGHT, IN_RIGHT }, { PAD_TRIGGER_L | PAD_TRIGGER_R | PAD_TRIGGER_Z, IN_SKIP },
    };
    for (unsigned i = 0; i < sizeof(pmap) / sizeof(pmap[0]); i++) {
        if (p & pmap[i].pad) input->buttons |= pmap[i].in;
    }
    /* -128..127 with up positive: to 0..255 with up low, as event.rs wants. */
    int sx = 128 + PAD_StickX(0), sy = 128 - PAD_StickY(0);
    input->stick_x = (uint8_t) (sx < 0 ? 0 : sx > 255 ? 255 : sx);
    input->stick_y = (uint8_t) (sy < 0 ? 0 : sy > 255 ? 255 : sy);

    for (int i = 0; i < press_count; i++) {
        if (frame >= presses[i].frame && frame < presses[i].frame + 4) input->buttons |= presses[i].button;
    }
    for (int i = 0; i < point_count; i++) {
        if (frame < points[i].frame) continue;
        input->pointer_valid = points[i].x >= 0;
        input->pointer_x = (int16_t) points[i].x;
        input->pointer_y = (int16_t) points[i].y;
    }
    if (power_pressed) input->buttons |= IN_HOME;
}

/* Debug: the EFB as it is about to be shown, to debug_screen.bmp next to the game. */
static void save_screenshot(void) {
    const uint32_t w = rmode->fbWidth, h = rmode->efbHeight;
    uint32_t size = ((w + 3) / 4) * ((h + 3) / 4) * 64;
    uint8_t* tex = memalign(32, size);
    if (tex == NULL) return;
    GX_SetTexCopySrc(0, 0, w, h);
    GX_SetTexCopyDst(w, h, GX_TF_RGBA8, GX_FALSE);
    DCInvalidateRange(tex, size);
    GX_CopyTex(tex, GX_FALSE);
    GX_PixModeSync();
    GX_DrawDone();
    DCInvalidateRange(tex, size);

    char path[128];
    snprintf(path, sizeof(path), "%s/debug_screen.bmp", SaveRoot);
    FILE* f = fopen(path, "wb");
    if (f != NULL) {
        const uint32_t row = w * 3, file_size = 54 + row * h;
        uint8_t header[54] = { 'B', 'M' };
        for (int i = 0; i < 4; i++) {
            header[2 + i] = (uint8_t) (file_size >> (8 * i));
            header[18 + i] = (uint8_t) (w >> (8 * i));
            header[22 + i] = (uint8_t) (h >> (8 * i));
        }
        header[10] = 54;
        header[14] = 40;
        header[26] = 1;
        header[28] = 24;
        fwrite(header, 1, sizeof(header), f);
        uint8_t* line = malloc(row);
        uint32_t tiles_per_row = (w + 3) / 4;
        for (int y = (int) h - 1; y >= 0 && line != NULL; y--) {
            for (uint32_t x = 0; x < w; x++) {
                const uint8_t* tile = tex + ((y / 4) * tiles_per_row + x / 4) * 64;
                uint32_t at = ((y & 3) * 4 + (x & 3)) * 2;
                line[x * 3] = tile[at + 33];    /* blue */
                line[x * 3 + 1] = tile[at + 32]; /* green */
                line[x * 3 + 2] = tile[at + 1];  /* red */
            }
            fwrite(line, 1, row, f);
        }
        free(line);
        fclose(f);
    }
    free(tex);
}

/* ---- Main ----------------------------------------------------------------------------------- */

static void on_power(void) {
    power_pressed = true;
}

static void on_reset(u32 irq, void* ctx) {
    (void) irq;
    (void) ctx;
    power_pressed = true;
}

/* The engine runs on its own thread: libogc's main thread has a small stack. */
static void* engine_thread(void* arg) {
    (void) arg;
    /* This thread feeds the GPU's FIFO: libogc must pause this one (not the thread that ran
     * GX_Init) when the FIFO fills up, or the FIFO overruns and the GPU hangs. */
    GX_SetCurrentGXThread();
    Session* session = rfvp_wii_start(GameRoot, SaveRoot);
    if (session == NULL) {
        printf("rfvp: the game could not be started; see %s/last_error.txt\n", SaveRoot);
        return NULL;
    }
    unsigned frames = 0;
    for (;;) {
        RfvpWiiInput input;
        read_input(&input, frames);
        frame_open = false;
        int status = rfvp_wii_frame(session, &input);
        draw_cursor(session);
        want_shot = frames == shot_frame;
        if (frame_open) present();
        frame_open = false;
        audio_pump(session);
        frames++;
        if (status != 0 || frames > quit_frame) break;
    }
    rfvp_wii_stop(session);
    return NULL;
}

int main(void) {
    gpu_init();
    WPAD_Init();
    WPAD_SetDataFormat(WPAD_CHAN_0, WPAD_FMT_BTNS_ACC_IR);
    WPAD_SetVRes(WPAD_CHAN_0, SCREEN_WIDTH, SCREEN_HEIGHT);
    PAD_Init();
    SYS_SetPowerCallback(on_power);
    SYS_SetResetCallback(on_reset);
    audio_init();
    if (!fatInitDefault()) printf("rfvp: the SD card could not be mounted\n");
    read_debug_file();
    if (debug_on) {
        char path[128];
        snprintf(path, sizeof(path), "%s/log.txt", SaveRoot);
        log_file = fopen(path, "w");
    }
    printf("rfvp: starting\n");

    /* From MEM2, but for 8 MiB left to libogc (IOS buffers, libfat): the engine thread's stack
     * (libogc's main thread has 128 KiB, which the engine outgrows), then its heap. */
    enum { EngineStack = 4 * 1024 * 1024 };
    uint8_t* lo = (uint8_t*) (((uintptr_t) SYS_GetArena2Lo() + 31) & ~31u);
    uint8_t* hi = (uint8_t*) SYS_GetArena2Hi();
    uint8_t* stack = lo;
    lo += EngineStack;
    size_t heap = (size_t) (hi - lo) - 8 * 1024 * 1024;
    SYS_SetArena2Lo(lo + heap);
    rfvp_wii_heap_init(lo, heap);
    printf("rfvp: engine heap %u KiB in MEM2\n", (unsigned) (heap / 1024));

    lwp_t thread;
    if (LWP_CreateThread(&thread, engine_thread, NULL, stack, EngineStack, 64) == 0) {
        LWP_JoinThread(thread, NULL);
    }

    if (log_file != NULL) fclose(log_file);
    ASND_End();
    fatUnmount("sd");
    return 0;
}
