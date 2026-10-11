/* The PS3 runtime: PSL1GHT owns the process, the pad, sound (the audio port) and the video
 * output; the rfvp engine (Rust, linked in as librfvp_ps3.a) runs one frame per call and draws
 * it through the functions below, on the RSX with two small shaders (sprite.vp, sprite.fp).
 *
 * The game is read from the first of GameRoots that exists; saves go next to it. */

#include <malloc.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

#include <audio/audio.h>
#include <io/pad.h>
#include <lv2/sysfs.h>
#include <rsx/rsx.h>
#include <lv2/systime.h>
#include <sys/process.h>
#include <sys/stat.h>
#include <sysutil/sysutil.h>
#include <sysutil/video.h>

#include "sprite_fpo.h"
#include "sprite_vpo.h"

/* The engine needs more than the default 64 KiB of stack. */
SYS_PROCESS_PARAM(1001, 0x100000);

#define SCREEN_WIDTH 1280
#define SCREEN_HEIGHT 720

/* Where the game may be: installed as a package, next to the ELF (RPCS3 maps /app_home to the
 * folder of the ELF it boots), or on a USB drive. */
static const char* const GameRoots[] = {
    "/dev_hdd0/game/RFVP00001/USRDIR/game",
    "/app_home/game",
    "/dev_usb000/rfvp/game",
};
static char game_root[256];
static char save_root[256];

/* `struct Ps3Input` in event.rs. */
typedef struct {
    uint32_t buttons;
    uint8_t stick_x;
    uint8_t stick_y;
} RfvpPs3Input;

/* Its button bits. */
enum {
    BUTTON_A = 1 << 0, /* Cross */
    BUTTON_B = 1 << 1, /* Circle */
    BUTTON_SELECT = 1 << 2,
    BUTTON_START = 1 << 3,
    BUTTON_RIGHT = 1 << 4,
    BUTTON_LEFT = 1 << 5,
    BUTTON_UP = 1 << 6,
    BUTTON_DOWN = 1 << 7,
    BUTTON_R = 1 << 8,  /* R1 */
    BUTTON_L = 1 << 9,  /* L1 */
    BUTTON_X = 1 << 10, /* Triangle */
    BUTTON_Y = 1 << 11, /* Square */
    BUTTON_ZL = 1 << 14, /* L2 */
    BUTTON_ZR = 1 << 15, /* R2 */
};

/* `struct CWallTime` in ffi.rs. */
typedef struct {
    uint16_t year;
    uint8_t month;
    uint8_t day;
    uint8_t day_of_week;
    uint8_t hour;
    uint8_t minute;
} RfvpPs3WallTime;

typedef struct Session Session;

/* The engine (platform/ps3/host/src/ffi.rs). */
Session* rfvp_ps3_start(const char* game_root, const char* save_root);
int rfvp_ps3_frame(Session* session, const RfvpPs3Input* input);
bool rfvp_ps3_cursor(Session* session, int32_t* x, int32_t* y);
void rfvp_ps3_audio_render(Session* session, int16_t* dst, size_t frames);
void rfvp_ps3_stop(Session* session);

/* ---- Services for the engine ---------------------------------------------------------------- */

void rfvp_ps3_log(const char* message, size_t len) {
    /* The TTY: RPCS3's TTY.log, or a debug console. */
    fwrite(message, 1, len, stdout);
    fflush(stdout);
}

uint64_t rfvp_ps3_ticks_us(void) {
    return sysGetSystemTime();
}

/* The time base (79.8 MHz), for timing short stretches of code (see rfvp::perf). */
uint32_t rfvp_ps3_cycles(void) {
    uint64_t tb;
    __asm__ volatile("mftb %0" : "=r"(tb));
    return (uint32_t) tb;
}

bool rfvp_ps3_wall_time(RfvpPs3WallTime* out) {
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

/* lv2's directory entry types. */
enum { LV2_DIRECTORY = 1, LV2_REGULAR = 2 };

typedef void (*RfvpDirVisitor)(void* ctx, const char* name, bool is_dir, uint64_t size);

/* List a directory through lv2 (Rust does not know the dirent layout). */
int rfvp_ps3_list_dir(const char* path, RfvpDirVisitor visit, void* ctx) {
    s32 fd;
    if (sysFsOpendir(path, &fd) != 0) return -1;
    sysFSDirent entry;
    u64 read;
    while (sysFsReaddir(fd, &entry, &read) == 0 && read != 0) {
        if (strcmp(entry.d_name, ".") == 0 || strcmp(entry.d_name, "..") == 0) continue;
        /* Known from the listing: no stat per file (an asset folder holds thousands). */
        if (entry.d_type == LV2_DIRECTORY || entry.d_type == LV2_REGULAR) {
            visit(ctx, entry.d_name, entry.d_type == LV2_DIRECTORY, 0);
        }
    }
    sysFsClosedir(fd);
    return 0;
}

/* 1 for a file, 2 for a directory, 0 if nothing is there. */
int rfvp_ps3_path_kind(const char* path, uint64_t* size) {
    sysFSStat st;
    if (sysFsStat(path, &st) != 0) return 0;
    if (size != NULL) *size = (uint64_t) st.st_size;
    return (st.st_mode & S_IFDIR) ? 2 : 1;
}

/* Make a directory through lv2 (newlib's mkdir is not wired to it). */
int rfvp_ps3_mkdir(const char* path) {
    return sysFsMkdir(path, 0755);
}

/* ---- Drawing -------------------------------------------------------------------------------- */

#define COMMAND_BUFFER_SIZE 0x100000
#define HOST_BUFFER_SIZE (32 * 1024 * 1024)
/* Vertices of one frame: 4 per sprite, 32 bytes each. */
#define VERTEX_BUFFER_SIZE (8 * 1024 * 1024)

typedef struct {
    float x, y; /* clip space */
    float s, t; /* normalized texture coordinates */
    float r, g, b, a;
} Vertex;

typedef struct {
    uint32_t* pixels; /* in RSX memory */
    uint32_t offset;
    uint32_t width, height, pitch; /* pitch in bytes */
} GpuTexture;

static gcmContextData* context;
static uint32_t* color_buffers[2];
static uint32_t color_offsets[2];
static uint32_t* depth_buffer;
static uint32_t depth_offset;
static int current; /* the buffer drawn into; the other one is shown */
static uint32_t clear_color; /* X8R8G8B8 */
static bool frame_open;

static rsxVertexProgram* vertex_program;
static void* vertex_ucode;
static rsxFragmentProgram* fragment_program;
static uint32_t fragment_offset;

static Vertex* vertices;
static uint32_t vertices_offset;
static uint32_t vertex_count;
static GpuTexture* white;

/* Textures the engine dropped while the RSX may still draw with them: freed after the flip. */
static GpuTexture** dead_textures;
static int dead_count, dead_capacity;

void rfvp_ps3_gpu_screen_size(uint32_t* width, uint32_t* height) {
    *width = SCREEN_WIDTH;
    *height = SCREEN_HEIGHT;
}

/* Write RGBA8 rows into the texture at (x, y). The texture is A8R8G8B8: big-endian words. */
static void copy_pixels(GpuTexture* t, uint32_t x, uint32_t y, uint32_t w, uint32_t h,
                        const uint8_t* rgba) {
    for (uint32_t row = 0; row < h; row++) {
        uint32_t* dst = (uint32_t*) ((uint8_t*) t->pixels + (size_t) (y + row) * t->pitch) + x;
        const uint8_t* src = rgba + (size_t) row * w * 4;
        for (uint32_t col = 0; col < w; col++, src += 4) {
            dst[col] = ((uint32_t) src[3] << 24) | ((uint32_t) src[0] << 16) |
                       ((uint32_t) src[1] << 8) | src[2];
        }
    }
}

void* rfvp_ps3_gpu_create_texture(uint32_t width, uint32_t height, const uint8_t* rgba) {
    GpuTexture* t = calloc(1, sizeof(GpuTexture));
    if (t == NULL) return NULL;
    t->width = width;
    t->height = height;
    t->pitch = (width * 4 + 63) & ~63u;
    t->pixels = rsxMemalign(128, (size_t) t->pitch * height);
    if (t->pixels == NULL) {
        free(t);
        return NULL;
    }
    rsxAddressToOffset(t->pixels, &t->offset);
    copy_pixels(t, 0, 0, width, height, rgba);
    return t;
}

void rfvp_ps3_gpu_update_texture(void* texture, uint32_t x, uint32_t y, uint32_t w, uint32_t h,
                                 const uint8_t* rgba) {
    copy_pixels((GpuTexture*) texture, x, y, w, h, rgba);
    if (frame_open) rsxInvalidateTextureCache(context, GCM_INVALIDATE_TEXTURE);
}

static void free_texture(GpuTexture* t) {
    rsxFree(t->pixels);
    free(t);
}

void rfvp_ps3_gpu_destroy_texture(void* texture) {
    if (dead_count == dead_capacity) {
        int capacity = dead_capacity ? dead_capacity * 2 : 256;
        GpuTexture** grown = realloc(dead_textures, capacity * sizeof(GpuTexture*));
        if (grown == NULL) return; /* leaked rather than freed while in use */
        dead_textures = grown;
        dead_capacity = capacity;
    }
    dead_textures[dead_count++] = texture;
}

static void set_render_target(int index) {
    gcmSurface sf;
    memset(&sf, 0, sizeof(sf));
    sf.colorFormat = GCM_SURFACE_X8R8G8B8;
    sf.colorTarget = GCM_SURFACE_TARGET_0;
    sf.colorLocation[0] = GCM_LOCATION_RSX;
    sf.colorOffset[0] = color_offsets[index];
    sf.colorPitch[0] = SCREEN_WIDTH * 4;
    for (int i = 1; i < 4; i++) {
        sf.colorLocation[i] = GCM_LOCATION_RSX;
        sf.colorPitch[i] = 64;
    }
    sf.depthFormat = GCM_SURFACE_ZETA_Z24S8;
    sf.depthLocation = GCM_LOCATION_RSX;
    sf.depthOffset = depth_offset;
    sf.depthPitch = SCREEN_WIDTH * 4;
    sf.type = GCM_SURFACE_TYPE_LINEAR;
    sf.antiAlias = GCM_SURFACE_CENTER_1;
    sf.width = SCREEN_WIDTH;
    sf.height = SCREEN_HEIGHT;
    rsxSetSurface(context, &sf);
}

static void set_blend(int additive) {
    if (additive) {
        rsxSetBlendFunc(context, GCM_SRC_ALPHA, GCM_ONE, GCM_ZERO, GCM_ONE);
    } else {
        rsxSetBlendFunc(context, GCM_SRC_ALPHA, GCM_ONE_MINUS_SRC_ALPHA, GCM_ONE,
                        GCM_ONE_MINUS_SRC_ALPHA);
    }
}

static void bind_texture(const GpuTexture* t) {
    gcmTexture tex;
    memset(&tex, 0, sizeof(tex));
    /* The hardware's format codes have bit 7 set (A8R8G8B8 is 0x85); PSL1GHT's constants
     * leave it out and rsxLoadTexture does not add it. */
    tex.format = 0x80 | GCM_TEXTURE_FORMAT_A8R8G8B8 | GCM_TEXTURE_FORMAT_LIN;
    tex.mipmap = 1;
    tex.dimension = GCM_TEXTURE_DIMS_2D;
    tex.cubemap = GCM_FALSE;
    tex.remap = GCM_TEXTURE_REMAP_TYPE_REMAP << GCM_TEXTURE_REMAP_TYPE_B_SHIFT |
                GCM_TEXTURE_REMAP_TYPE_REMAP << GCM_TEXTURE_REMAP_TYPE_G_SHIFT |
                GCM_TEXTURE_REMAP_TYPE_REMAP << GCM_TEXTURE_REMAP_TYPE_R_SHIFT |
                GCM_TEXTURE_REMAP_TYPE_REMAP << GCM_TEXTURE_REMAP_TYPE_A_SHIFT |
                GCM_TEXTURE_REMAP_COLOR_B << GCM_TEXTURE_REMAP_COLOR_B_SHIFT |
                GCM_TEXTURE_REMAP_COLOR_G << GCM_TEXTURE_REMAP_COLOR_G_SHIFT |
                GCM_TEXTURE_REMAP_COLOR_R << GCM_TEXTURE_REMAP_COLOR_R_SHIFT |
                GCM_TEXTURE_REMAP_COLOR_A << GCM_TEXTURE_REMAP_COLOR_A_SHIFT;
    tex.width = t->width;
    tex.height = t->height;
    tex.depth = 1;
    tex.pitch = t->pitch;
    tex.location = GCM_LOCATION_RSX;
    tex.offset = t->offset;
    rsxLoadTexture(context, 0, &tex);
    rsxTextureControl(context, 0, GCM_TRUE, 0 << 8, 12 << 8, GCM_TEXTURE_MAX_ANISO_1);
    rsxTextureFilter(context, 0, 0, GCM_TEXTURE_LINEAR, GCM_TEXTURE_LINEAR,
                     GCM_TEXTURE_CONVOLUTION_QUINCUNX);
    rsxTextureWrapMode(context, 0, GCM_TEXTURE_CLAMP_TO_EDGE, GCM_TEXTURE_CLAMP_TO_EDGE,
                       GCM_TEXTURE_CLAMP_TO_EDGE, GCM_TEXTURE_UNSIGNED_REMAP_NORMAL,
                       GCM_TEXTURE_ZFUNC_LESS, 0);
}

void rfvp_ps3_gpu_begin_frame(void) {
    if (frame_open) return;
    set_render_target(current);
    rsxSetColorMask(context, GCM_COLOR_MASK_R | GCM_COLOR_MASK_G | GCM_COLOR_MASK_B |
                                 GCM_COLOR_MASK_A);
    rsxSetColorMaskMrt(context, 0);
    rsxSetClearColor(context, clear_color);
    rsxSetClearDepthStencil(context, 0xffffff00);
    rsxClearSurface(context, GCM_CLEAR_R | GCM_CLEAR_G | GCM_CLEAR_B | GCM_CLEAR_A |
                                 GCM_CLEAR_Z | GCM_CLEAR_S);
    float scale[4] = { SCREEN_WIDTH * 0.5f, SCREEN_HEIGHT * -0.5f, 0.5f, 0.0f };
    float offset[4] = { SCREEN_WIDTH * 0.5f, SCREEN_HEIGHT * 0.5f, 0.5f, 0.0f };
    rsxSetViewport(context, 0, 0, SCREEN_WIDTH, SCREEN_HEIGHT, 0.0f, 1.0f, scale, offset);
    rsxSetScissor(context, 0, 0, SCREEN_WIDTH, SCREEN_HEIGHT);
    rsxSetDepthTestEnable(context, GCM_FALSE);
    rsxSetDepthWriteEnable(context, GCM_FALSE);
    rsxSetCullFaceEnable(context, GCM_FALSE);
    rsxSetShadeModel(context, GCM_SHADE_MODEL_SMOOTH);
    rsxSetBlendEquation(context, GCM_FUNC_ADD, GCM_FUNC_ADD);
    rsxSetBlendEnable(context, GCM_TRUE);
    rsxLoadVertexProgram(context, vertex_program, vertex_ucode);
    rsxLoadFragmentProgramLocation(context, fragment_program, fragment_offset, GCM_LOCATION_RSX);
    rsxInvalidateTextureCache(context, GCM_INVALIDATE_TEXTURE);
    /* The vertex buffer is bound once; each draw starts at its own vertices. */
    rsxBindVertexArrayAttrib(context, GCM_VERTEX_ATTRIB_POS, 0, vertices_offset, sizeof(Vertex), 2,
                             GCM_VERTEX_DATA_TYPE_F32, GCM_LOCATION_RSX);
    rsxBindVertexArrayAttrib(context, GCM_VERTEX_ATTRIB_TEX0, 0, vertices_offset + 8,
                             sizeof(Vertex), 2, GCM_VERTEX_DATA_TYPE_F32, GCM_LOCATION_RSX);
    rsxBindVertexArrayAttrib(context, GCM_VERTEX_ATTRIB_COLOR0, 0, vertices_offset + 16,
                             sizeof(Vertex), 4, GCM_VERTEX_DATA_TYPE_F32, GCM_LOCATION_RSX);
    vertex_count = 0;
    frame_open = true;
}

void rfvp_ps3_gpu_set_clip(int32_t x0, int32_t y0, int32_t x1, int32_t y1) {
    if (x1 <= x0 || y1 <= y0) {
        x0 = y0 = 0;
        x1 = y1 = 1;
    }
    rsxSetScissor(context, (u16) x0, (u16) y0, (u16) (x1 - x0), (u16) (y1 - y0));
}

static void put_vertex(Vertex* v, float x, float y, float s, float t, uint32_t rgba) {
    v->x = x * (2.0f / SCREEN_WIDTH) - 1.0f;
    v->y = 1.0f - y * (2.0f / SCREEN_HEIGHT);
    v->s = s;
    v->t = t;
    v->r = (rgba & 0xff) * (1.0f / 255.0f);
    v->g = ((rgba >> 8) & 0xff) * (1.0f / 255.0f);
    v->b = ((rgba >> 16) & 0xff) * (1.0f / 255.0f);
    v->a = (rgba >> 24) * (1.0f / 255.0f);
}

/* Draw `count` vertices (screen x, y, texel u, v each) as `type` with texture `t`. */
static void draw(GpuTexture* t, uint32_t type, const float* c, int count, uint32_t rgba,
                 int additive) {
    if (!frame_open) return;
    if ((vertex_count + count) * sizeof(Vertex) > VERTEX_BUFFER_SIZE) return; /* frame is full */
    float sw = 1.0f / t->width, sh = 1.0f / t->height;
    for (int i = 0; i < count; i++) {
        put_vertex(&vertices[vertex_count + i], c[i * 4], c[i * 4 + 1], c[i * 4 + 2] * sw,
                   c[i * 4 + 3] * sh, rgba);
    }
    bind_texture(t);
    set_blend(additive);
    rsxDrawVertexArray(context, type, vertex_count, count);
    vertex_count += count;
}

/* `c`: screen x, y and texel u, v for each corner, in triangle-strip order. */
void rfvp_ps3_gpu_draw_sprite(void* texture, const float* c, uint32_t rgba, int additive) {
    draw(texture, GCM_TYPE_TRIANGLE_STRIP, c, 4, rgba, additive);
}

void rfvp_ps3_gpu_draw_rect(int32_t x0, int32_t y0, int32_t x1, int32_t y1, uint32_t rgba,
                            int additive) {
    float c[16] = { x0, y0, 0, 0, x0, y1, 0, 8, x1, y0, 8, 0, x1, y1, 8, 8 };
    draw(white, GCM_TYPE_TRIANGLE_STRIP, c, 4, rgba, additive);
}

void rfvp_ps3_gpu_end_frame(void) {
}

/* The pointer, when the stick has shown it: a small white arrow with a dark outline. */
static void draw_cursor(Session* session) {
    int32_t x, y;
    if (!frame_open || !rfvp_ps3_cursor(session, &x, &y)) return;
    rfvp_ps3_gpu_set_clip(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT);
    float fx = (float) x, fy = (float) y;
    float outline[12] = { fx - 2, fy - 4, 4, 4, fx - 2, fy + 24, 4, 4, fx + 20, fy + 18, 4, 4 };
    float arrow[12] = { fx, fy, 4, 4, fx, fy + 18, 4, 4, fx + 14, fy + 14, 4, 4 };
    draw(white, GCM_TYPE_TRIANGLES, outline, 3, 0xff000000u, 0);
    draw(white, GCM_TYPE_TRIANGLES, arrow, 3, 0xffffffffu, 0);
}

/* Send the RSX back to the start of the command buffer. Done once a frame while it is idle, so
 * the buffer never fills: wrapping around when it does (the system's callback) stalls. */
static void rewind_commands(void) {
    gcmControlRegister* ctrl = gcmGetControlRegister();
    rsxFlushBuffer(context);
    u32 begin, current;
    rsxAddressToOffset(context->begin, &begin);
    rsxAddressToOffset(context->current, &current);
    while (ctrl->get != current) usleep(30); /* everything sent so far is done */
    *context->current = 0x20000000 | begin;  /* JUMP begin */
    __asm__ volatile("sync" ::: "memory");
    ctrl->put = begin;
    while (ctrl->get != begin) usleep(30);
    context->current = context->begin;
}

/* Show the drawn buffer at the next vertical blank, and wait for it: the RSX is then done with
 * the frame, so its vertex buffer and the textures dropped during it are free again. */
static void video_flip(void) {
    gcmResetFlipStatus();
    gcmSetFlip(context, current);
    rsxFlushBuffer(context);
    gcmSetWaitFlip(context);
    unsigned waited = 0;
    while (gcmGetFlipStatus() != 0) {
        usleep(200);
        if (++waited == 10000) { /* 2 s: report where the RSX stopped */
            gcmControlRegister* ctrl = gcmGetControlRegister();
            printf("rfvp: the RSX is stuck: put %x get %x ref %x, %u vertices\n",
                   (unsigned) ctrl->put, (unsigned) ctrl->get, (unsigned) ctrl->ref, vertex_count);
            fflush(stdout);
        }
    }
    rewind_commands();
    for (int i = 0; i < dead_count; i++) free_texture(dead_textures[i]);
    dead_count = 0;
    current ^= 1;
    frame_open = false;
}

static bool video_init(void) {
    void* host = memalign(1024 * 1024, HOST_BUFFER_SIZE);
    if (host == NULL || rsxInit(&context, COMMAND_BUFFER_SIZE, HOST_BUFFER_SIZE, host) != 0) {
        printf("rfvp: rsxInit failed\n");
        return false;
    }
    videoState state;
    if (videoGetState(0, 0, &state) != 0 || state.state != 0) {
        printf("rfvp: no video output\n");
        return false;
    }
    videoConfiguration config;
    memset(&config, 0, sizeof(config));
    config.resolution = VIDEO_RESOLUTION_720;
    config.format = VIDEO_BUFFER_FORMAT_XRGB;
    config.pitch = SCREEN_WIDTH * 4;
    config.aspect = VIDEO_ASPECT_AUTO;
    if (videoConfigure(0, &config, NULL, 0) != 0) {
        printf("rfvp: the display does not take 720p\n");
        return false;
    }
    gcmSetFlipMode(GCM_FLIP_VSYNC);
    for (int i = 0; i < 2; i++) {
        color_buffers[i] = rsxMemalign(64, SCREEN_WIDTH * SCREEN_HEIGHT * 4);
        if (color_buffers[i] == NULL) return false;
        rsxAddressToOffset(color_buffers[i], &color_offsets[i]);
        gcmSetDisplayBuffer(i, color_offsets[i], SCREEN_WIDTH * 4, SCREEN_WIDTH, SCREEN_HEIGHT);
    }
    depth_buffer = rsxMemalign(64, SCREEN_WIDTH * SCREEN_HEIGHT * 4);
    vertices = rsxMemalign(128, VERTEX_BUFFER_SIZE);
    if (depth_buffer == NULL || vertices == NULL) return false;
    rsxAddressToOffset(depth_buffer, &depth_offset);
    rsxAddressToOffset(vertices, &vertices_offset);

    /* The shaders (sprite.vp and sprite.fp, assembled by cgcomp). */
    vertex_program = (rsxVertexProgram*) sprite_vpo;
    u32 size;
    rsxVertexProgramGetUCode(vertex_program, &vertex_ucode, &size);
    /* cgcomp marks the 2D texture coordinates only from a Cg program's declarations; for
     * assembly the mask stays empty and the RSX would not take texcoord 0 as 2D. */
    fragment_program = memalign(64, sprite_fpo_size);
    if (fragment_program == NULL) return false;
    memcpy(fragment_program, sprite_fpo, sprite_fpo_size);
    fragment_program->texcoord2D = fragment_program->texcoords;
    void* ucode;
    rsxFragmentProgramGetUCode(fragment_program, &ucode, &size);
    void* fragment_ucode = rsxMemalign(64, size);
    if (fragment_ucode == NULL) return false;
    memcpy(fragment_ucode, ucode, size);
    rsxAddressToOffset(fragment_ucode, &fragment_offset);

    static uint8_t white_pixels[8 * 8 * 4];
    memset(white_pixels, 0xff, sizeof(white_pixels));
    white = rfvp_ps3_gpu_create_texture(8, 8, white_pixels);
    return white != NULL;
}

/* ---- Sound ---------------------------------------------------------------------------------- */

#define AUDIO_BLOCKS 16
static u32 audio_port;
static audioPortConfig audio_config;
static uint64_t audio_written; /* blocks filled so far */
static bool audio_ready;

static void audio_init(void) {
    if (audioInit() != 0) {
        printf("rfvp: audioInit failed: no sound\n");
        return;
    }
    audioPortParam params;
    params.numChannels = AUDIO_PORT_2CH;
    params.numBlocks = AUDIO_BLOCK_16;
    params.attrib = 0;
    params.level = 1.0f;
    if (audioPortOpen(&params, &audio_port) != 0 ||
        audioGetPortConfig(audio_port, &audio_config) != 0) {
        printf("rfvp: no audio port: no sound\n");
        return;
    }
    memset((void*) (uintptr_t) audio_config.audioDataStart, 0, audio_config.portSize);
    audioPortStart(audio_port);
    audio_ready = true;
}

/* Fill the blocks the port has played since the last call (it plays 48 kHz stereo floats, 256
 * frames a block, around a ring of AUDIO_BLOCKS). */
static void audio_pump(Session* session) {
    if (!audio_ready) return;
    static int16_t pcm[AUDIO_BLOCK_SAMPLES * 2];
    uint64_t playing = *(volatile uint64_t*) (uintptr_t) audio_config.readIndex;
    float* ring = (float*) (uintptr_t) audio_config.audioDataStart;
    /* Keep filling until the block before the one playing. */
    uint64_t target = playing + AUDIO_BLOCKS - 1;
    while ((audio_written % AUDIO_BLOCKS) != (target % AUDIO_BLOCKS)) {
        rfvp_ps3_audio_render(session, pcm, AUDIO_BLOCK_SAMPLES);
        float* block = ring + (audio_written % AUDIO_BLOCKS) * AUDIO_BLOCK_SAMPLES * 2;
        for (int i = 0; i < AUDIO_BLOCK_SAMPLES * 2; i++) block[i] = pcm[i] * (1.0f / 32768.0f);
        audio_written++;
    }
}

static void audio_fini(void) {
    if (!audio_ready) return;
    audioPortStop(audio_port);
    audioPortClose(audio_port);
    audioQuit();
}

/* ---- Input ---------------------------------------------------------------------------------- */

static RfvpPs3Input pad_state = { 0, 128, 128 };

/* The first connected pad. ioPadGetData reports nothing (len 0) when nothing changed, so the
 * last reading stays. */
static void read_input(RfvpPs3Input* input) {
    padInfo info;
    if (ioPadGetInfo(&info) == 0) {
        for (int i = 0; i < MAX_PADS; i++) {
            if (!info.status[i]) continue;
            padData data;
            if (ioPadGetData(i, &data) == 0 && data.len > 0) {
                uint32_t b = 0;
                if (data.BTN_CROSS) b |= BUTTON_A;
                if (data.BTN_CIRCLE) b |= BUTTON_B;
                if (data.BTN_TRIANGLE) b |= BUTTON_X;
                if (data.BTN_SQUARE) b |= BUTTON_Y;
                if (data.BTN_SELECT) b |= BUTTON_SELECT;
                if (data.BTN_START) b |= BUTTON_START;
                if (data.BTN_UP) b |= BUTTON_UP;
                if (data.BTN_DOWN) b |= BUTTON_DOWN;
                if (data.BTN_LEFT) b |= BUTTON_LEFT;
                if (data.BTN_RIGHT) b |= BUTTON_RIGHT;
                if (data.BTN_L1) b |= BUTTON_L;
                if (data.BTN_R1) b |= BUTTON_R;
                if (data.BTN_L2) b |= BUTTON_ZL;
                if (data.BTN_R2) b |= BUTTON_ZR;
                pad_state.buttons = b;
                pad_state.stick_x = (uint8_t) data.ANA_L_H;
                pad_state.stick_y = (uint8_t) data.ANA_L_V;
            }
            break;
        }
    }
    *input = pad_state;
}

/* Debug: button presses scripted in debug.txt (`press=cross@600`), for testing without hands. */
#define MAX_PRESSES 32
static struct { uint32_t button; unsigned frame; } presses[MAX_PRESSES];
static int press_count;
static unsigned shot_frame = 0xffffffff;

static void read_debug_file(void) {
    static const struct { const char* name; uint32_t bit; } names[] = {
        { "cross", BUTTON_A }, { "circle", BUTTON_B }, { "triangle", BUTTON_X },
        { "square", BUTTON_Y }, { "up", BUTTON_UP }, { "down", BUTTON_DOWN },
        { "left", BUTTON_LEFT }, { "right", BUTTON_RIGHT }, { "start", BUTTON_START },
        { "select", BUTTON_SELECT }, { "l1", BUTTON_L }, { "r1", BUTTON_R },
    };
    char path[300];
    snprintf(path, sizeof(path), "%s/debug.txt", game_root);
    FILE* f = fopen(path, "r");
    if (f == NULL) return;
    char line[64], name[16];
    unsigned frame;
    while (fgets(line, sizeof(line), f) != NULL && press_count < MAX_PRESSES) {
        if (sscanf(line, "shot=%u", &frame) == 1) {
            shot_frame = frame;
            clear_color = 0x00400000; /* dark red: tells drawing apart from reading back */
        }
        if (sscanf(line, "press=%15[a-z0-9]@%u", name, &frame) != 2) continue;
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

static void apply_presses(RfvpPs3Input* input, unsigned frame) {
    for (int i = 0; i < press_count; i++) {
        if (frame >= presses[i].frame && frame < presses[i].frame + 4) input->buttons |= presses[i].button;
    }
}

/* Debug: the frame shown last, to debug_screen.bmp in the save folder. */
static void save_screenshot(void) {
    char path[300];
    snprintf(path, sizeof(path), "%s/debug_screen.bmp", save_root);
    FILE* f = fopen(path, "wb");
    if (f == NULL) return;
    const uint32_t w = SCREEN_WIDTH, h = SCREEN_HEIGHT, size = 54 + w * h * 3;
    /* BMP's header fields are little-endian. */
    uint8_t header[54] = { 'B', 'M' };
    for (int i = 0; i < 4; i++) {
        header[2 + i] = (uint8_t) (size >> (i * 8));
        header[18 + i] = (uint8_t) (w >> (i * 8));
        header[22 + i] = (uint8_t) (h >> (i * 8));
    }
    header[10] = 54;
    header[14] = 40;
    header[26] = 1;
    header[28] = 24;
    fwrite(header, 1, sizeof(header), f);
    static uint8_t line[SCREEN_WIDTH * 3];
    const uint32_t* shown = color_buffers[current ^ 1];
    for (int y = (int) h - 1; y >= 0; y--) {
        for (uint32_t x = 0; x < w; x++) {
            uint32_t p = shown[y * SCREEN_WIDTH + x]; /* X8R8G8B8 */
            line[x * 3] = (uint8_t) p;
            line[x * 3 + 1] = (uint8_t) (p >> 8);
            line[x * 3 + 2] = (uint8_t) (p >> 16);
        }
        fwrite(line, 1, sizeof(line), f);
    }
    fclose(f);
}

/* ---- Main ----------------------------------------------------------------------------------- */

static volatile bool exit_requested;

static void sysutil_callback(u64 status, u64 param, void* user) {
    (void) param;
    (void) user;
    if (status == SYSUTIL_EXIT_GAME) exit_requested = true;
}

static bool find_game(void) {
    for (unsigned i = 0; i < sizeof(GameRoots) / sizeof(GameRoots[0]); i++) {
        if (rfvp_ps3_path_kind(GameRoots[i], NULL) != 2) continue;
        snprintf(game_root, sizeof(game_root), "%s", GameRoots[i]);
        /* Saves go in the folder holding the game. */
        snprintf(save_root, sizeof(save_root), "%s", GameRoots[i]);
        char* slash = strrchr(save_root, '/');
        if (slash != NULL) *slash = '\0';
        return true;
    }
    return false;
}

int main(void) {
    printf("rfvp: starting\n");
    sysUtilRegisterCallback(SYSUTIL_EVENT_SLOT0, sysutil_callback, NULL);
    ioPadInit(7);
    if (!video_init()) return 1;
    audio_init();

    if (!find_game()) {
        printf("rfvp: no game: put it in one of\n");
        for (unsigned i = 0; i < sizeof(GameRoots) / sizeof(GameRoots[0]); i++) printf("  %s\n", GameRoots[i]);
        return 1;
    }
    printf("rfvp: game in %s\n", game_root);
    read_debug_file();

    Session* session = rfvp_ps3_start(game_root, save_root);
    if (session == NULL) {
        printf("rfvp: the game could not be started; see %s/last_error.txt\n", save_root);
    } else {
        unsigned frames = 0;
        while (!exit_requested) {
            sysUtilCheckCallback();
            RfvpPs3Input input;
            read_input(&input);
            apply_presses(&input, frames);
            int status = rfvp_ps3_frame(session, &input);
            draw_cursor(session);
            if (frame_open) {
                video_flip();
            } else {
                usleep(16000); /* nothing new to show: about a frame */
            }
            audio_pump(session);
            if (frames == shot_frame) save_screenshot();
            frames++;
            if (status != 0) break;
        }
        rfvp_ps3_stop(session);
    }

    audio_fini();
    ioPadEnd();
    return 0;
}
