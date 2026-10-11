/*
 * The PS2 runtime: owns the IOP modules, the GS (through gsKit), the pad (libpad), sound
 * (audsrv) and the main loop, and runs the engine (platform/ps2/host, a Rust static library)
 * one frame at a time.
 *
 * The game is read from the disc (`cdrom0:`, saving to the memory card, `mc0:/RFVP`) when the
 * ELF was started from one, from `host:game` when it was started from `host:` (an emulator's
 * host file system), otherwise from `mass:/RFVP` (a USB drive).
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <malloc.h>
#include <dirent.h>
#include <sys/stat.h>

#include <kernel.h>
#include <sifrpc.h>
#include <loadfile.h>
#include <sbv_patches.h>
#include <timer.h>
#include <libpad.h>
#include <audsrv.h>
#include <gsKit.h>
#include <dmaKit.h>
#include <screenshot.h>
#include <sio.h>

/* audsrv.irx, embedded at build time (see Makefile). */
extern unsigned char audsrv_irx[];
extern unsigned int size_audsrv_irx;

/* The engine (platform/ps2/host/src/ffi.rs). */
typedef struct Session Session;

/* `struct Ps2Input` in event.rs. */
typedef struct RfvpPs2Input {
    uint32_t buttons;
    uint8_t stick_x;
    uint8_t stick_y;
} RfvpPs2Input;

/* `struct CWallTime` in ffi.rs. */
typedef struct RfvpPs2WallTime {
    uint16_t year;
    uint8_t month;
    uint8_t day;
    uint8_t day_of_week;
    uint8_t hour;
    uint8_t minute;
} RfvpPs2WallTime;

Session *rfvp_ps2_start(const char *game_root, const char *save_root);
int rfvp_ps2_frame(Session *session, const RfvpPs2Input *input);
int rfvp_ps2_cursor(Session *session, int32_t *x, int32_t *y);
void rfvp_ps2_audio_render(Session *session, int16_t *dst, size_t frames);
void rfvp_ps2_stop(Session *session);
uint32_t rfvp_ps2_last_mark(void);

static GSGLOBAL *gs;
/* Debug: print the sprites of one frame (set by main). */
static int log_sprites;

/* ---- services for the engine ---- */

void rfvp_ps2_log(const uint8_t *message, size_t len) {
    fwrite(message, 1, len, stdout);
    fflush(stdout);
}

/* The CPU cycle counter (COP0 Count), for timing short stretches of code. */
uint32_t rfvp_ps2_cycles(void) {
    uint32_t count;
    __asm__ volatile("mfc0 %0, $9" : "=r"(count));
    return count;
}

uint64_t rfvp_ps2_ticks_us(void) {
    /* The EE timer counts the 147.456 MHz bus clock. */
    return GetTimerSystemTime() * 1000000ull / kBUSCLK;
}

int rfvp_ps2_wall_time(RfvpPs2WallTime *out) {
    (void)out;
    return 0;
}

/* 0: missing, 1: a file (its size in `size`), 2: a directory. */
int rfvp_ps2_path_kind(const char *path, uint64_t *size) {
    struct stat st;
    if (stat(path, &st) != 0) {
        return 0;
    }
    if (S_ISDIR(st.st_mode)) {
        return 2;
    }
    *size = (uint64_t)st.st_size;
    return 1;
}

typedef void (*DirVisitor)(void *ctx, const char *name, int is_dir, uint64_t size);

/* Call `visit` for each entry of `path`; -1 if it cannot be listed. Sizes may be 0 (unknown). */
int rfvp_ps2_list_dir(const char *path, DirVisitor visit, void *ctx) {
    DIR *dir = opendir(path);
    if (dir == NULL) {
        return -1;
    }
    struct dirent *entry;
    char full[512];
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
            continue;
        }
        uint64_t size = 0;
        int is_dir;
        if (entry->d_type == DT_DIR || entry->d_type == DT_REG) {
            /* Known from the listing: no stat per file (an asset folder holds thousands, and
             * each stat is a round trip to the IOP). The size is then left at 0. */
            is_dir = entry->d_type == DT_DIR;
        } else {
            snprintf(full, sizeof(full), "%s/%s", path, entry->d_name);
            is_dir = rfvp_ps2_path_kind(full, &size) == 2;
        }
        visit(ctx, entry->d_name, is_dir, size);
    }
    closedir(dir);
    return 0;
}

/* ---- drawing ---- */

/* The GS takes 0x80 as full intensity and full opacity. */
static inline uint8_t gs_level(uint8_t v) {
    return (uint8_t)((v * 128 + 127) / 255);
}

/* A texture's tint: the GS modulates with 0x80 as 1.0. */
static u64 gs_tint(uint32_t rgba) {
    return GS_SETREG_RGBAQ(gs_level(rgba & 0xff), gs_level((rgba >> 8) & 0xff),
                           gs_level((rgba >> 16) & 0xff), gs_level(rgba >> 24), 0x00);
}

/* A flat color: full-range RGB, alpha with 0x80 as opaque. */
static u64 gs_flat(uint32_t rgba) {
    return GS_SETREG_RGBAQ(rgba & 0xff, (rgba >> 8) & 0xff, (rgba >> 16) & 0xff,
                           gs_level(rgba >> 24), 0x00);
}

static void gs_blend(int additive) {
    /* Normal: (Cs - Cd) * As + Cd. Additive: (Cs - 0) * As + Cd. */
    gsKit_set_primalpha(gs, additive ? GS_SETREG_ALPHA(0, 2, 0, 1, 0) : GS_SETREG_ALPHA(0, 1, 0, 1, 0), 0);
}

void rfvp_ps2_gs_screen_size(uint32_t *width, uint32_t *height) {
    *width = gs->Width;
    *height = gs->Height;
}

/* Copy RGBA8 pixels into a GS texture's memory, with alpha scaled to the GS's 0..0x80. */
static void copy_pixels(GSTEXTURE *tex, uint32_t x, uint32_t y, uint32_t w, uint32_t h, const uint8_t *rgba) {
    uint8_t *dst = (uint8_t *)tex->Mem;
    for (uint32_t row = 0; row < h; row++) {
        const uint8_t *s = rgba + (size_t)row * w * 4;
        uint8_t *d = dst + ((size_t)(y + row) * tex->Width + x) * 4;
        for (uint32_t col = 0; col < w; col++) {
            d[0] = s[0];
            d[1] = s[1];
            d[2] = s[2];
            d[3] = gs_level(s[3]);
            d += 4;
            s += 4;
        }
    }
}

void *rfvp_ps2_gs_create_texture(uint32_t width, uint32_t height, const uint8_t *rgba) {
    GSTEXTURE *tex = calloc(1, sizeof(GSTEXTURE));
    if (tex == NULL) {
        return NULL;
    }
    tex->Width = width;
    tex->Height = height;
    tex->PSM = GS_PSM_CT32;
    /* The GS reads texture rows TBW * 64 pixels apart. */
    tex->TBW = (width + 63) / 64;
    tex->Filter = GS_FILTER_LINEAR;
    tex->Mem = memalign(128, gsKit_texture_size_ee(width, height, GS_PSM_CT32));
    if (tex->Mem == NULL) {
        free(tex);
        return NULL;
    }
    copy_pixels(tex, 0, 0, width, height, rgba);
    /* The GS reads the pixels by DMA, from memory: write them out of the data cache. */
    FlushCache(0);
    return tex;
}

void rfvp_ps2_gs_update_texture(void *texture, uint32_t x, uint32_t y, uint32_t w, uint32_t h, const uint8_t *rgba) {
    GSTEXTURE *tex = texture;
    copy_pixels(tex, x, y, w, h, rgba);
    FlushCache(0);
    gsKit_TexManager_invalidate(gs, tex);
}

void rfvp_ps2_gs_destroy_texture(void *texture) {
    GSTEXTURE *tex = texture;
    gsKit_TexManager_free(gs, tex);
    free(tex->Mem);
    free(tex);
}

void rfvp_ps2_gs_begin_frame(void) {
    gsKit_clear(gs, GS_SETREG_RGBAQ(0, 0, 0, 0x80, 0));
}

void rfvp_ps2_gs_set_clip(int32_t x0, int32_t y0, int32_t x1, int32_t y1) {
    if (x1 <= x0 || y1 <= y0) {
        /* Nothing may be drawn: a one-pixel box off to the corner. */
        x0 = y0 = 0;
        x1 = y1 = 1;
    }
    gsKit_set_scissor(gs, GS_SETREG_SCISSOR(x0, x1 - 1, y0, y1 - 1));
}

/* `corners`: x, y, u, v for each of the four corners, in triangle-strip order. (Floats only
 * cross from Rust through memory: Rust's code is soft-float, which passes them in integer
 * registers.) */
void rfvp_ps2_gs_draw_sprite(void *texture, const float *c, uint32_t rgba, int additive) {
    GSTEXTURE *tex = texture;
    if (log_sprites) {
        printf("rfvp: sprite %ux%u (%d,%d uv %d,%d) (%d,%d uv %d,%d) (%d,%d uv %d,%d) (%d,%d uv %d,%d) %08x\n",
               (unsigned)tex->Width, (unsigned)tex->Height,
               (int)c[0], (int)c[1], (int)c[2], (int)c[3], (int)c[4], (int)c[5], (int)c[6], (int)c[7],
               (int)c[8], (int)c[9], (int)c[10], (int)c[11], (int)c[12], (int)c[13], (int)c[14], (int)c[15],
               (unsigned)rgba);
    }
    /* The textures on screen are more than the video memory left by the frame buffers, so the
     * texture manager reuses it within a frame. It uploads at once while drawing is queued:
     * run what is queued first, so earlier sprites are drawn before their texture is replaced. */
    gsKit_queue_exec(gs);
    gsKit_TexManager_bind(gs, tex);
    gs_blend(additive);
    gsKit_prim_quad_texture_3d(gs, tex,
        c[0], c[1], 0, c[2], c[3],
        c[4], c[5], 0, c[6], c[7],
        c[8], c[9], 0, c[10], c[11],
        c[12], c[13], 0, c[14], c[15],
        gs_tint(rgba));
}

void rfvp_ps2_gs_draw_rect(int32_t x0, int32_t y0, int32_t x1, int32_t y1, uint32_t rgba, int additive) {
    gs_blend(additive);
    gsKit_prim_quad_3d(gs, x0, y0, 0, x1, y0, 0, x0, y1, 0, x1, y1, 0, gs_flat(rgba));
}

void rfvp_ps2_gs_end_frame(void) {
}

/* The pointer, when the stick has shown it: a small white arrow with a dark outline. */
static void draw_cursor(Session *session) {
    int32_t x, y;
    if (!rfvp_ps2_cursor(session, &x, &y)) {
        return;
    }
    /* The cursor moves in a 640x448 space (event.rs); PAL screens are taller. */
    float fx = (float)x;
    float fy = (float)y * (float)gs->Height / 448.0f;
    gsKit_set_scissor(gs, GS_SETREG_SCISSOR(0, gs->Width - 1, 0, gs->Height - 1));
    gs_blend(0);
    gsKit_prim_triangle_3d(gs, fx - 1, fy - 2, 0, fx - 1, fy + 15, 0, fx + 12, fy + 11, 0,
                           GS_SETREG_RGBAQ(0, 0, 0, 0x80, 0));
    gsKit_prim_triangle_3d(gs, fx, fy, 0, fx, fy + 12, 0, fx + 9, fy + 9, 0,
                           GS_SETREG_RGBAQ(0x80, 0x80, 0x80, 0x80, 0));
}

static void gs_init(void) {
    gs = gsKit_init_global();
    gs->PSM = GS_PSM_CT32;
    gs->PSMZ = GS_PSMZ_16S;
    gs->ZBuffering = GS_SETTING_OFF;
    gs->DoubleBuffering = GS_SETTING_ON;
    gs->PrimAlphaEnable = GS_SETTING_ON;
    dmaKit_init(D_CTRL_RELE_OFF, D_CTRL_MFD_OFF, D_CTRL_STS_UNSPEC, D_CTRL_STD_OFF, D_CTRL_RCYC_8, 1 << DMA_CHANNEL_GIF);
    dmaKit_chan_init(DMA_CHANNEL_GIF);
    gsKit_init_screen(gs);
    gsKit_TexManager_init(gs);
    gsKit_set_test(gs, GS_ZTEST_OFF);
    gsKit_set_clamp(gs, GS_CMODE_CLAMP);
    gsKit_mode_switch(gs, GS_ONESHOT);
}

/* Debug: the picture on screen as a 32-bit BMP. */
static void save_screen(const char *path) {
    uint32_t w = gs->Width, h = gs->Height;
    uint8_t *pixels = memalign(64, w * h * 4);
    if (pixels == NULL) {
        return;
    }
    /* The buffer shown is the one not being drawn into. */
    unsigned shown = gs->ScreenBuffer[gs->ActiveBuffer ^ 1];
    /* In strips: one transfer reads back at most 256 KiB correctly. */
    for (uint32_t y = 0; y < h; y += 64) {
        uint32_t rows = h - y < 64 ? h - y : 64;
        ps2_screenshot(pixels + (size_t)y * w * 4, shown / 256, 0, y, w, rows, GS_PSM_CT32);
    }
    FILE *f = fopen(path, "wb");
    if (f != NULL) {
        uint32_t image = w * h * 4;
        uint8_t header[54] = { 'B', 'M' };
        uint32_t file_size = 54 + image;
        int32_t neg_h = -(int32_t)h;
        memcpy(header + 2, &file_size, 4);
        header[10] = 54;
        header[14] = 40;
        memcpy(header + 18, &w, 4);
        memcpy(header + 22, &neg_h, 4);
        header[26] = 1;
        header[28] = 32;
        fwrite(header, 1, 54, f);
        /* RGBA -> BGRA */
        for (uint32_t i = 0; i < w * h; i++) {
            uint8_t r = pixels[i * 4];
            pixels[i * 4] = pixels[i * 4 + 2];
            pixels[i * 4 + 2] = r;
            pixels[i * 4 + 3] = 255;
        }
        fwrite(pixels, 1, image, f);
        fclose(f);
    }
    free(pixels);
}

/* ---- pad ---- */

static char pad_buf[256] __attribute__((aligned(64)));
static int pad_ready;

static void pad_init(void) {
    padInit(0);
    pad_ready = padPortOpen(0, 0, pad_buf) != 0;
}

static void pad_read(RfvpPs2Input *input) {
    input->buttons = 0;
    input->stick_x = 128;
    input->stick_y = 128;
    if (!pad_ready) {
        return;
    }
    int state = padGetState(0, 0);
    if (state != PAD_STATE_STABLE && state != PAD_STATE_FINDCTP1) {
        return;
    }
    static int analog_requested;
    if (!analog_requested) {
        /* A DualShock starts digital; turn on (and lock) the sticks. */
        padSetMainMode(0, 0, PAD_MMODE_DUALSHOCK, PAD_MMODE_LOCK);
        analog_requested = 1;
    }
    struct padButtonStatus status;
    if (padRead(0, 0, &status) == 0) {
        return;
    }
    /* libpad reports buttons held as 0 bits. */
    input->buttons = 0xffffu ^ status.btns;
    if ((status.mode >> 4) == 0x7) {
        input->stick_x = status.ljoy_h;
        input->stick_y = status.ljoy_v;
    }
}

/* Debug: button presses scripted in debug.txt (`press=cross@600`), for testing without hands. */
#define MAX_PRESSES 32
static struct { uint32_t button; unsigned frame; } presses[MAX_PRESSES];
static int press_count;

static void add_press(const char *line) {
    static const struct { const char *name; uint32_t bit; } names[] = {
        { "cross", PAD_CROSS }, { "circle", PAD_CIRCLE }, { "triangle", PAD_TRIANGLE },
        { "square", PAD_SQUARE }, { "up", PAD_UP }, { "down", PAD_DOWN }, { "left", PAD_LEFT },
        { "right", PAD_RIGHT }, { "start", PAD_START }, { "select", PAD_SELECT },
        { "l1", PAD_L1 }, { "r1", PAD_R1 },
    };
    char name[16];
    unsigned frame;
    if (press_count >= MAX_PRESSES || sscanf(line, "press=%15[a-z0-9]@%u", name, &frame) != 2) {
        return;
    }
    for (unsigned i = 0; i < sizeof(names) / sizeof(names[0]); i++) {
        if (strcmp(name, names[i].name) == 0) {
            presses[press_count].button = names[i].bit;
            presses[press_count].frame = frame;
            press_count++;
        }
    }
}

/* Held for four frames from the frame given. */
static uint32_t scripted_buttons(unsigned frame) {
    uint32_t held = 0;
    for (int i = 0; i < press_count; i++) {
        if (frame >= presses[i].frame && frame < presses[i].frame + 4) {
            held |= presses[i].button;
        }
    }
    return held;
}

/* ---- sound ---- */

static int audio_ready;

static void audio_init(void) {
    int ret = SifLoadModule("rom0:LIBSD", 0, NULL);
    if (ret < 0) {
        printf("rfvp: LIBSD did not load (%d): no sound\n", ret);
        return;
    }
    int irx_ret = 0;
    ret = SifExecModuleBuffer(audsrv_irx, size_audsrv_irx, 0, NULL, &irx_ret);
    if (ret < 0 || audsrv_init() != 0) {
        printf("rfvp: audsrv did not start (%d): no sound\n", ret);
        return;
    }
    struct audsrv_fmt_t format = { .freq = 48000, .bits = 16, .channels = 2 };
    if (audsrv_set_format(&format) != 0) {
        printf("rfvp: audsrv refused 48 kHz stereo: no sound\n");
        return;
    }
    audsrv_set_volume(MAX_VOLUME);
    audio_ready = 1;
}

/* Fill what audsrv has room for (it plays from its own buffer on the IOP). */
static void audio_pump(Session *session) {
    static int16_t chunk[1024 * 2];
    if (!audio_ready) {
        return;
    }
    int room = audsrv_available();
    while (room >= (int)sizeof(chunk)) {
        rfvp_ps2_audio_render(session, chunk, 1024);
        audsrv_play_audio((const char *)chunk, sizeof(chunk));
        room -= sizeof(chunk);
    }
}

/* Debug: a vertical-blank handler that reports where the engine is when no frame has finished
 * for two seconds (through the SIO, which works from an interrupt). */
static volatile unsigned watchdog_frames;
static unsigned watchdog_seen, watchdog_still;

static void sio_hex(uint32_t v) {
    char text[12] = "0x";
    for (int i = 0; i < 8; i++) {
        unsigned d = (v >> (28 - i * 4)) & 0xf;
        text[2 + i] = (char)(d < 10 ? '0' + d : 'a' + d - 10);
    }
    text[10] = '\n';
    text[11] = 0;
    sio_puts(text);
}

static int watchdog_vblank(int cause) {
    (void)cause;
    if (watchdog_frames != watchdog_seen) {
        watchdog_seen = watchdog_frames;
        watchdog_still = 0;
    } else if (++watchdog_still == 100) {
        sio_puts("rfvp watchdog: stalled at frame ");
        sio_hex(watchdog_seen);
        sio_puts("rfvp watchdog: core mark ");
        sio_hex(rfvp_ps2_last_mark());
        watchdog_still = 0;
    }
    ExitHandler();
    return 0;
}

/* ---- main ---- */

int main(int argc, char *argv[]) {
    SifInitRpc(0);
    /* Lets modules be loaded from EE memory (audsrv.irx) on retail BIOSes. */
    sbv_patch_enable_lmb();
    SifLoadModule("rom0:SIO2MAN", 0, NULL);
    SifLoadModule("rom0:PADMAN", 0, NULL);
    printf("rfvp: starting\n");

    gs_init();
    pad_init();
    audio_init();

    const char *root = "mass:/RFVP";
    const char *save_root = root;
    if (argc > 0 && argv[0] != NULL && strncmp(argv[0], "host:", 5) == 0) {
        root = save_root = "host:game";
    } else if (argc > 0 && argv[0] != NULL && strncmp(argv[0], "cdrom0:", 7) == 0) {
        /* A disc is read-only: saves go to the memory card in slot 1. */
        root = "cdrom0:";
        save_root = "mc0:/RFVP";
        SifLoadModule("rom0:MCMAN", 0, NULL);
        SifLoadModule("rom0:MCSERV", 0, NULL);
    }
    printf("rfvp: game directory %s, saves in %s\n", root, save_root);
    Session *session = rfvp_ps2_start(root, save_root);
    if (session == NULL) {
        printf("rfvp: the game could not be started; see last_error.txt in %s\n", save_root);
        SleepThread();
        return 1;
    }

    /* Debug: with debug.txt in the game directory, save the screen at frame `shot=N` (300 if
     * not given) to debug_screen.bmp and stop: reading the GS back leaves the GIF unusable.
     * `press=BUTTON@N` lines press a button at frame N. */
    char flag_path[128];
    snprintf(flag_path, sizeof(flag_path), "%s/debug.txt", root);
    unsigned shot_frame = 0;
    FILE *flag = fopen(flag_path, "r");
    if (flag != NULL) {
        char line[64];
        shot_frame = 300;
        while (fgets(line, sizeof(line), flag) != NULL) {
            sscanf(line, "shot=%u", &shot_frame);
            add_press(line);
        }
        fclose(flag);
    }
    int debug_screens = shot_frame != 0;
    if (debug_screens) {
        AddIntcHandler(INTC_VBLANK_S, watchdog_vblank, 0);
        EnableIntc(INTC_VBLANK_S);
    }
    unsigned frames = 0;

    for (;;) {
        RfvpPs2Input input;
        pad_read(&input);
        input.buttons |= scripted_buttons(frames);
        log_sprites = debug_screens && frames + 1 == shot_frame;
        if (log_sprites) {
            printf("rfvp: screen %dx%d mode %d interlace %d field %d magh %d magv %d dw %d dh %d\n",
                   gs->Width, gs->Height, gs->Mode, gs->Interlace, gs->Field, gs->MagH, gs->MagV,
                   gs->DW, gs->DH);
        }
        uint64_t t0 = rfvp_ps2_ticks_us();
        int status = rfvp_ps2_frame(session, &input);
        uint64_t t1 = rfvp_ps2_ticks_us();
        draw_cursor(session);
        audio_pump(session);
        uint64_t t2 = rfvp_ps2_ticks_us();
        gsKit_queue_exec(gs);
        uint64_t t3 = rfvp_ps2_ticks_us();
        gsKit_sync_flip(gs);
        uint64_t t4 = rfvp_ps2_ticks_us();
        if (debug_screens) {
            static uint64_t engine_us, audio_us, gs_us, flip_us;
            engine_us += t1 - t0;
            audio_us += t2 - t1;
            gs_us += t3 - t2;
            flip_us += t4 - t3;
            if (frames % 60 == 59) {
                printf("rfvp: per frame: engine %u us, audio %u us, gs %u us, vsync %u us\n",
                       (unsigned)(engine_us / 60), (unsigned)(audio_us / 60), (unsigned)(gs_us / 60),
                       (unsigned)(flip_us / 60));
                engine_us = audio_us = gs_us = flip_us = 0;
            }
        }
        gsKit_TexManager_nextFrame(gs);
        frames++;
        watchdog_frames = frames;
        if (debug_screens && frames == shot_frame) {
            char path[160];
            snprintf(path, sizeof(path), "%s/debug_screen.bmp", save_root);
            save_screen(path);
            printf("rfvp: saved %s at frame %u; stopping\n", path, frames);
            SleepThread();
        }
        if (status != 0) {
            break;
        }
    }
    rfvp_ps2_stop(session);
    if (audio_ready) {
        audsrv_quit();
    }
    printf("rfvp: stopped\n");
    SleepThread();
    return 0;
}
