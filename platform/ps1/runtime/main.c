/* The PlayStation runtime (PSn00bSDK). The rfvp engine (Rust, linked in as one object, see
 * ../build_ps1_rust.sh) runs a frame per call; this file gives it the console:
 *
 * - files: the disc's files by the game's names (FILES.MAP, written by rfvp-psx-convert), read
 *   through a small sector cache;
 * - drawing: GPU primitives into double-buffered 320x240 frames; textures in VRAM, cut into
 *   tiles that each sit inside one texture page;
 * - sound: SPU voices playing sounds from sound RAM, and XA music streamed by the drive (paused
 *   while files are read, then resumed where it was);
 * - the pad.
 *
 * The converter has already put everything in the console's formats; nothing is decoded here. */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <hwregs_c.h>
#include <psxapi.h>
#include <psxcd.h>
#include <psxetc.h>
#include <psxgpu.h>
#include <psxpad.h>
#include <psxsn.h>
#include <psxspu.h>

#define SCREEN_W 320
#define SCREEN_H 240

/* RAM the port uses: 2 MiB on a console, 8 MiB on a development one (or an emulator set so). */
#ifndef RAM_TOP
#define RAM_TOP 0x80800000u
#endif
#define STACK_SIZE (128 * 1024)

typedef struct Session Session;

/* `struct Ps1Input` in event.rs. */
typedef struct {
    uint32_t buttons;
    uint8_t stick_x, stick_y;
} RfvpPs1Input;

enum {
    BUTTON_A = 1 << 0,  /* Cross */
    BUTTON_B = 1 << 1,  /* Circle */
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

/* The engine (platform/ps1/host/src/ffi.rs). */
Session* rfvp_ps1_start(void);
int rfvp_ps1_frame(Session* session, const RfvpPs1Input* input);
void rfvp_ps1_stop(Session* session);
void rfvp_ps1_heap_init(void* base, size_t size);

/* ---- Services ------------------------------------------------------------------------------- */

void rfvp_ps1_log(const char* message, size_t len) {
    printf("%.*s", (int) len, message);
}

/* Microseconds: vertical blanks (NTSC, 59.94 a second). */
uint64_t rfvp_ps1_ticks_us(void) {
    return (uint64_t) VSync(-1) * 16683u;
}

uint32_t rfvp_ps1_cycles(void) {
    return (uint32_t) VSync(-1);
}

/* ---- Files ---------------------------------------------------------------------------------- */

typedef struct {
    char game[48];  /* the game's name, lower case */
    char disc[16];  /* the disc's ISO 9660 name */
    int searched, found;
    uint32_t lba, size;
} DiscFile;

static DiscFile* files;
static int file_count;

/* Sectors read ahead, for the many small reads the engine makes. */
#define CACHE_SECTORS 16
static uint8_t cache[CACHE_SECTORS * 2048] __attribute__((aligned(4)));
static uint32_t cache_lba = 0xffffffffu;
static int cache_len;

static int xa_active;
static void xa_suspend(void);
static void xa_resume(void);

static void read_sectors(uint32_t lba, int count, void* buf) {
    CdlLOC loc;
    uint8_t mode = CdlModeSpeed;
    xa_suspend();
    CdControl(CdlSetmode, &mode, 0);
    CdIntToPos(lba, &loc);
    CdControl(CdlSetloc, &loc, 0);
    CdRead(count, (uint32_t*) buf, CdlModeSpeed);
    CdReadSync(0, 0);
}

static void load_file_map(void) {
    CdlFILE f;
    if (!CdSearchFile(&f, "\\FILES.MAP;1")) {
        printf("rfvp: FILES.MAP is not on the disc\n");
        return;
    }
    int sectors = (f.size + 2047) / 2048;
    char* text = malloc(sectors * 2048 + 1);
    read_sectors(CdPosToInt(&f.pos), sectors, text);
    text[f.size] = 0;
    int lines = 0;
    for (char* p = text; *p; p++) lines += *p == '\n';
    files = calloc(lines + 1, sizeof(DiscFile));
    for (char* line = strtok(text, "\n"); line; line = strtok(0, "\n")) {
        DiscFile* e = &files[file_count];
        if (sscanf(line, "%47s %15s", e->game, e->disc) == 2) file_count++;
    }
    free(text);
    printf("rfvp: %d files on the disc\n", file_count);
}

static int name_eq(const char* a, const char* b) {
    for (; *a && *b; a++, b++) {
        char ca = (*a >= 'A' && *a <= 'Z') ? *a + 32 : *a;
        char cb = (*b >= 'A' && *b <= 'Z') ? *b + 32 : *b;
        if (ca == '\\') ca = '/';
        if (cb == '\\') cb = '/';
        if (ca != cb) return 0;
    }
    return *a == *b;
}

/* The handle of a file by the game's name, or -1; `size` gets its length. */
int rfvp_ps1_file_find(const char* name, uint32_t* size) {
    while (name[0] == '.' && name[1] == '/') name += 2;
    while (name[0] == '/') name++;
    for (int i = 0; i < file_count; i++) {
        DiscFile* e = &files[i];
        if (!name_eq(e->game, name)) continue;
        if (!e->searched) {
            char path[32];
            CdlFILE f;
            snprintf(path, sizeof(path), "\\%s;1", e->disc);
            e->searched = 1;
            if (CdSearchFile(&f, path)) {
                e->found = 1;
                e->lba = CdPosToInt(&f.pos);
                e->size = f.size;
            } else {
                printf("rfvp: %s is not on the disc\n", e->disc);
            }
        }
        if (!e->found) return -1;
        if (size) *size = e->size;
        return i;
    }
    return -1;
}

int rfvp_ps1_file_count(void) {
    return file_count;
}

const char* rfvp_ps1_file_name(int i) {
    return i >= 0 && i < file_count ? files[i].game : "";
}

/* The disc position of a file (for XA streams). */
uint32_t rfvp_ps1_file_lba(int handle) {
    return handle >= 0 && handle < file_count ? files[handle].lba : 0;
}

/* Read `len` bytes at `offset` of a file; returns the bytes read. */
int rfvp_ps1_file_read(int handle, uint32_t offset, uint8_t* buf, uint32_t len) {
    if (handle < 0 || handle >= file_count || !files[handle].found) return -1;
    DiscFile* e = &files[handle];
    if (offset >= e->size) return 0;
    if (len > e->size - offset) len = e->size - offset;
    uint32_t done = 0;
    while (done < len) {
        uint32_t pos = offset + done;
        uint32_t lba = e->lba + pos / 2048;
        uint32_t in_sector = pos % 2048;
        uint32_t left = len - done;
        /* Whole sectors straight into the buffer when it is aligned and they are many. */
        if (in_sector == 0 && left >= 2048 * 4 && ((uintptr_t) (buf + done) & 3) == 0) {
            int n = left / 2048;
            read_sectors(lba, n, buf + done);
            done += n * 2048;
            continue;
        }
        if (lba < cache_lba || lba >= cache_lba + (uint32_t) cache_len) {
            read_sectors(lba, CACHE_SECTORS, cache);
            cache_lba = lba;
            cache_len = CACHE_SECTORS;
        }
        uint32_t at = (lba - cache_lba) * 2048 + in_sector;
        uint32_t n = cache_len * 2048 - at;
        if (n > left) n = left;
        memcpy(buf + done, cache + at, n);
        done += n;
    }
    xa_resume();
    return (int) done;
}

/* ---- Drawing -------------------------------------------------------------------------------- */

/* Primitives of a frame (two: one drawn while the next is built). */
#if RAM_TOP <= 0x80200000u
#define PRIM_BUFFER (48 * 1024)
#else
#define PRIM_BUFFER (192 * 1024)
#endif
static DISPENV disp[2];
static DRAWENV draw[2];
static uint8_t prims[2][PRIM_BUFFER] __attribute__((aligned(4)));
static uint8_t* next_prim;
static uint32_t* first_tag;
static uint32_t* last_tag;
static int db;
static int frame_open;

static void* new_prim(size_t size) {
    if (next_prim + size > prims[db] + PRIM_BUFFER) return 0;
    uint32_t* p = (uint32_t*) next_prim;
    next_prim += size;
    if (last_tag) catPrim(last_tag, p);
    else first_tag = p;
    last_tag = p;
    return p;
}

/* VRAM: the frame buffers take x 0-319, y 0-479. Textures go in the rest, 64 halfwords wide
 * texture-page columns (x 320-1023), cut into tiles that each stay inside one column so their
 * texel coordinates fit in a byte. CLUTs go in the rows below the frame buffers (y 480-511). */
#define COLUMNS 11
#define COLUMN_X0 320
#define ROW1_Y 256
#define ROW1_H 224
#define CLUT_Y 480

typedef struct {
    int16_t shelf_y, shelf_h, shelf_x; /* the open shelf */
    int16_t top;                       /* where the next shelf starts */
    int users;
} Bin;

static Bin bins[COLUMNS * 2];

static int bin_x(int b) { return COLUMN_X0 + (b % COLUMNS) * 64; }
static int bin_y(int b) { return b < COLUMNS ? 0 : ROW1_Y; }
static int bin_h(int b) { return b < COLUMNS ? 255 : ROW1_H; }

/* Place `units` x `h` (halfwords) with texel width `texels` (`per` texels a halfword): the
 * tile's texel coordinates must stay within 0..255. */
static int bin_alloc(int units, int h, int texels, int per, int16_t* ox, int16_t* oy) {
    for (int b = 0; b < COLUMNS * 2; b++) {
        Bin* bin = &bins[b];
        int fits_open = bin->shelf_h >= h && bin->shelf_x + units <= 64 &&
                        bin->shelf_x * per + texels <= 255;
        if (!fits_open) {
            /* A new shelf. */
            if (bin->top + h > bin_h(b) || units > 64 || texels > 255) continue;
            bin->shelf_y = bin->top;
            bin->shelf_h = h;
            bin->shelf_x = 0;
            bin->top += h;
        }
        *ox = bin_x(b) + bin->shelf_x;
        *oy = bin_y(b) + bin->shelf_y;
        bin->shelf_x += units;
        bin->users++;
        return b;
    }
    return -1;
}

static void bin_free(int b) {
    if (b < 0) return;
    if (--bins[b].users == 0) memset(&bins[b], 0, sizeof(Bin));
}

/* CLUT cells: 16 halfwords each, 64 a row, 32 rows. */
static uint8_t clut_used[32][64];

static int clut_alloc(int cells, int16_t* x, int16_t* y) {
    for (int row = 0; row < 32; row++) {
        for (int c = 0; c + cells <= 64; c += cells) {
            int free_run = 1;
            for (int k = 0; k < cells; k++) free_run &= !clut_used[row][c + k];
            if (!free_run) continue;
            memset(&clut_used[row][c], 1, cells);
            *x = c * 16;
            *y = CLUT_Y + row;
            return 1;
        }
    }
    return 0;
}

static void clut_free(int16_t x, int16_t y, int cells) {
    if (y < CLUT_Y) return;
    memset(&clut_used[y - CLUT_Y][x / 16], 0, cells);
}

enum { FORMAT_DIRECT = 0, FORMAT_CLUT4 = 1, FORMAT_CLUT8 = 2 };

typedef struct {
    int16_t vx, vy;     /* VRAM position */
    int16_t tx, ty;     /* texel origin in the picture */
    int16_t w, h;       /* texels */
    uint16_t tpage;     /* without the blend mode */
    uint8_t u, v;
    int8_t bin;
} Tile;

typedef struct {
    int format;
    int width, height;
    int16_t clut_x, clut_y, stp_x, stp_y; /* the palette, and one with STP on every color */
    int clut_cells;
    int count;
    Tile tiles[];
} Texture;

static int texels_per_unit(int format) {
    return format == FORMAT_CLUT4 ? 4 : format == FORMAT_CLUT8 ? 2 : 1;
}

/* Upload `rows` of a picture region to VRAM, packed for its depth. */
static void upload_tile(const Tile* t, int format, const uint8_t* data, int stride_bytes) {
    int per = texels_per_unit(format);
    /* Whole 32-bit words: the GPU takes an even number of halfwords. */
    int units = ((t->w + per - 1) / per + 1) & ~1;
    int row_bytes = units * 2;
    uint8_t* buf = calloc(row_bytes, t->h);
    for (int y = 0; y < t->h; y++) {
        const uint8_t* src = data + (t->ty + y) * stride_bytes;
        uint8_t* dst = buf + y * row_bytes;
        /* Only the tile's own texels: the padding stays transparent. */
        if (format == FORMAT_CLUT4) {
            memcpy(dst, src + t->tx / 2, (t->w + 1) / 2);
        } else if (format == FORMAT_CLUT8) {
            memcpy(dst, src + t->tx, t->w);
        } else {
            memcpy(dst, src + t->tx * 2, t->w * 2);
        }
    }
    RECT r;
    setRECT(&r, t->vx, t->vy, units, t->h);
    LoadImage(&r, (uint32_t*) buf);
    DrawSync(0);
    free(buf);
}

static void upload_clut(int16_t x, int16_t y, const uint16_t* colors, int count, int all_stp) {
    /* DMA needs word alignment. */
    static uint16_t buf[256] __attribute__((aligned(4)));
    for (int i = 0; i < count; i++) {
        uint16_t c = colors[i];
        buf[i] = (all_stp && c) ? (c | 0x8000) : c;
    }
    RECT r;
    setRECT(&r, x, y, count, 1);
    LoadImage(&r, (uint32_t*) buf);
    DrawSync(0);
}

/* A texture. `pixels`: for FORMAT_DIRECT 15-bit colors, `width` a row; for the CLUT formats
 * u16 colors, u16 stride, the palette, then the indices (see PixelFormat::Clut8). */
void* rfvp_ps1_gpu_create_texture(int format, uint32_t width, uint32_t height, const uint8_t* pixels) {
    int per = texels_per_unit(format);
    const uint8_t* indices = pixels;
    int stride_bytes = width * 2;
    const uint16_t* palette = 0;
    int colors = 0;
    if (format != FORMAT_DIRECT) {
        colors = pixels[0] | (pixels[1] << 8);
        int stride = pixels[2] | (pixels[3] << 8);
        palette = (const uint16_t*) (pixels + 4);
        indices = pixels + 4 + colors * 2;
        stride_bytes = format == FORMAT_CLUT4 ? stride / 2 : stride;
    }
    /* Tiles: within a column, texel coordinates below 256. */
    int tile_w = format == FORMAT_CLUT4 ? 236 : format == FORMAT_CLUT8 ? 126 : 64;
    int tile_h = 224;
    int cols = (width + tile_w - 1) / tile_w, rows = (height + tile_h - 1) / tile_h;
    Texture* t = calloc(1, sizeof(Texture) + cols * rows * sizeof(Tile));
    if (!t) return 0;
    t->format = format;
    t->width = width;
    t->height = height;
    t->count = cols * rows;
    for (int r = 0; r < rows; r++) {
        for (int c = 0; c < cols; c++) {
            Tile* tile = &t->tiles[r * cols + c];
            tile->tx = c * tile_w;
            tile->ty = r * tile_h;
            tile->w = (int) width - tile->tx < tile_w ? (int) width - tile->tx : tile_w;
            tile->h = (int) height - tile->ty < tile_h ? (int) height - tile->ty : tile_h;
            /* Whole words (see upload_tile). */
            int units = ((tile->w + per - 1) / per + 1) & ~1;
            tile->bin = bin_alloc(units, tile->h, units * per, per, &tile->vx, &tile->vy);
            if (tile->bin < 0) {
                printf("rfvp: VRAM is full (%ux%u texture)\n", (unsigned) width, (unsigned) height);
                for (int k = 0; k < r * cols + c; k++) bin_free(t->tiles[k].bin);
                free(t);
                return 0;
            }
            int page_x = tile->vx & ~63, page_y = tile->vy & ~255;
            tile->tpage = getTPage(format == FORMAT_DIRECT ? 2 : format == FORMAT_CLUT8 ? 1 : 0, 0, page_x, page_y);
            tile->u = (tile->vx - page_x) * per;
            tile->v = tile->vy - page_y;
            upload_tile(tile, format, indices, stride_bytes);
        }
    }
    if (format != FORMAT_DIRECT) {
        t->clut_cells = format == FORMAT_CLUT8 ? 16 : 1;
        int count = format == FORMAT_CLUT8 ? 256 : 16;
        static uint16_t full[256] __attribute__((aligned(4)));
        memset(full, 0, sizeof(full));
        memcpy(full, palette, (colors < count ? colors : count) * 2);
        if (!clut_alloc(t->clut_cells, &t->clut_x, &t->clut_y) ||
            !clut_alloc(t->clut_cells, &t->stp_x, &t->stp_y)) {
            printf("rfvp: CLUT space is full\n");
        } else {
            upload_clut(t->clut_x, t->clut_y, full, count, 0);
            upload_clut(t->stp_x, t->stp_y, full, count, 1);
        }
    }
    return t;
}

/* Replace a rectangle of a direct-color texture from its whole picture (`pixels`: 15-bit
 * colors, `stride` a row), widened to whole words where it has to be. */
void rfvp_ps1_gpu_update_texture(void* texture, uint32_t x, uint32_t y, uint32_t w, uint32_t h,
                                 const uint16_t* pixels, uint32_t stride) {
    Texture* t = texture;
    if (!t || t->format != FORMAT_DIRECT) return;
    DrawSync(0);
    for (int i = 0; i < t->count; i++) {
        Tile* tile = &t->tiles[i];
        int x0 = (int) x > tile->tx ? (int) x : tile->tx;
        int y0 = (int) y > tile->ty ? (int) y : tile->ty;
        int x1 = (int) (x + w) < tile->tx + tile->w ? (int) (x + w) : tile->tx + tile->w;
        int y1 = (int) (y + h) < tile->ty + tile->h ? (int) (y + h) : tile->ty + tile->h;
        if (x1 <= x0 || y1 <= y0) continue;
        /* Even columns from the tile's start: the tile's VRAM rows are whole words. */
        x0 = tile->tx + ((x0 - tile->tx) & ~1);
        x1 = tile->tx + ((x1 - tile->tx + 1) & ~1);
        int cw = x1 - x0, ch = y1 - y0;
        uint16_t* buf = calloc(cw * ch, 2);
        if (!buf) return;
        for (int row = 0; row < ch; row++) {
            for (int col = 0; col < cw; col++) {
                int sx = x0 + col;
                buf[row * cw + col] = sx < t->width ? pixels[(y0 + row) * stride + sx] : 0;
            }
        }
        RECT r;
        setRECT(&r, tile->vx + (x0 - tile->tx), tile->vy + (y0 - tile->ty), cw, ch);
        LoadImage(&r, (uint32_t*) buf);
        DrawSync(0);
        free(buf);
    }
}

void rfvp_ps1_gpu_destroy_texture(void* texture) {
    Texture* t = texture;
    if (!t) return;
    /* The GPU may still draw the last frame with it. */
    DrawSync(0);
    for (int i = 0; i < t->count; i++) bin_free(t->tiles[i].bin);
    if (t->format != FORMAT_DIRECT) {
        clut_free(t->clut_x, t->clut_y, t->clut_cells);
        clut_free(t->stp_x, t->stp_y, t->clut_cells);
    }
    free(t);
}

void rfvp_ps1_gpu_screen_size(uint32_t* w, uint32_t* h) {
    *w = SCREEN_W;
    *h = SCREEN_H;
}

static int clip_x0, clip_y0, clip_x1, clip_y1;

void rfvp_ps1_gpu_set_clip(int32_t x0, int32_t y0, int32_t x1, int32_t y1) {
    if (x0 < 0) x0 = 0;
    if (y0 < 0) y0 = 0;
    if (x1 > SCREEN_W) x1 = SCREEN_W;
    if (y1 > SCREEN_H) y1 = SCREEN_H;
    if (x1 <= x0 || y1 <= y0) x0 = y0 = x1 = y1 = 0;
    clip_x0 = x0, clip_y0 = y0, clip_x1 = x1, clip_y1 = y1;
    if (!frame_open) return;
    DR_AREA* a = new_prim(sizeof(DR_AREA));
    if (!a) return;
    RECT r;
    setRECT(&r, draw[db].clip.x + x0, draw[db].clip.y + y0, x1 - x0, y1 - y0);
    setDrawArea(a, &r);
}

void rfvp_ps1_gpu_begin_frame(void) {
    next_prim = prims[db];
    first_tag = last_tag = 0;
    frame_open = 1;
    /* Black behind everything (the letterbox). */
    FILL* f = new_prim(sizeof(FILL));
    if (f) {
        setFill(f);
        setRGB0(f, 0, 0, 0);
        f->x0 = draw[db].clip.x;
        f->y0 = draw[db].clip.y;
        f->w = SCREEN_W;
        f->h = SCREEN_H;
    }
    rfvp_ps1_gpu_set_clip(0, 0, SCREEN_W, SCREEN_H);
}

/* Sprite alpha on a GPU without it: opaque, half (the STP palette, blend mode 0) or nothing. */
static int alpha_mode(uint32_t rgba, int additive, int* stp) {
    int a = rgba >> 24;
    *stp = 0;
    if (additive) {
        *stp = 1;
        return a > 8 ? 1 : -1;
    }
    if (a < 32) return -1;
    if (a < 192) {
        *stp = 1;
        return 0;
    }
    return 0;
}

static uint8_t tint(uint32_t c, int shift, int additive, int a) {
    int v = (c >> shift) & 0xff;
    if (additive) v = v * a / 255; /* additive fades by brightness */
    return (uint8_t) (v >> 1);     /* 128 is 1.0 */
}

/* `c`: screen x, y and texel u, v for each of the four corners, in any order (the texel
 * rectangle is axis-aligned; the screen quad may be anything). */
void rfvp_ps1_gpu_draw_sprite(void* texture, const float* c, uint32_t rgba, int additive) {
    Texture* t = texture;
    if (!t || !frame_open) return;
    int stp;
    int mode = alpha_mode(rgba, additive, &stp);
    if (mode < 0) return;
    int a = rgba >> 24;
    /* The corners by texel position: q[bottom][right] = x, y. */
    float umin = c[2], umax = c[2], vmin = c[3], vmax = c[3];
    for (int k = 1; k < 4; k++) {
        float u = c[k * 4 + 2], v = c[k * 4 + 3];
        umin = u < umin ? u : umin, umax = u > umax ? u : umax;
        vmin = v < vmin ? v : vmin, vmax = v > vmax ? v : vmax;
    }
    if (umax - umin < 0.5f || vmax - vmin < 0.5f) return;
    float q[2][2][2];
    float umid = (umin + umax) * 0.5f, vmid = (vmin + vmax) * 0.5f;
    for (int k = 0; k < 4; k++) {
        int right = c[k * 4 + 2] > umid, bottom = c[k * 4 + 3] > vmid;
        q[bottom][right][0] = c[k * 4];
        q[bottom][right][1] = c[k * 4 + 1];
    }
    float du = umax - umin, dv = vmax - vmin;
    for (int i = 0; i < t->count; i++) {
        Tile* tile = &t->tiles[i];
        float a0 = tile->tx > umin ? tile->tx : umin, a1 = tile->tx + tile->w < umax ? tile->tx + tile->w : umax;
        float b0 = tile->ty > vmin ? tile->ty : vmin, b1 = tile->ty + tile->h < vmax ? tile->ty + tile->h : vmax;
        if (a1 <= a0 || b1 <= b0) continue;
        POLY_FT4* p = new_prim(sizeof(POLY_FT4));
        if (!p) return;
        setPolyFT4(p);
        setRGB0(p, tint(rgba, 0, additive, a), tint(rgba, 8, additive, a), tint(rgba, 16, additive, a));
        if (stp) setSemiTrans(p, 1);
        /* The piece's corners in the PlayStation's order (top left, top right, bottom left,
         * bottom right), placed by interpolating the sprite's corners. */
        float us[2] = { a0, a1 }, vs[2] = { b0, b1 };
        int16_t xy[4][2];
        uint8_t uv[4][2];
        for (int k = 0; k < 4; k++) {
            int right = k & 1, bottom = k >> 1;
            float uu = us[right], vv = vs[bottom];
            float s = (uu - umin) / du, r = (vv - vmin) / dv;
            float tx = q[0][0][0] + (q[0][1][0] - q[0][0][0]) * s, ty = q[0][0][1] + (q[0][1][1] - q[0][0][1]) * s;
            float bx = q[1][0][0] + (q[1][1][0] - q[1][0][0]) * s, by = q[1][0][1] + (q[1][1][1] - q[1][0][1]) * s;
            xy[k][0] = (int16_t) (tx + (bx - tx) * r + 0.5f);
            xy[k][1] = (int16_t) (ty + (by - ty) * r + 0.5f);
            /* The GPU samples up to the last texel before an edge's coordinate. */
            int lu = (int) (uu - tile->tx + 0.5f), lv = (int) (vv - tile->ty + 0.5f);
            if (right && lu > 0 && tile->u + lu > 255) lu--;
            if (bottom && lv > 0 && tile->v + lv > 255) lv--;
            uv[k][0] = (uint8_t) (tile->u + lu);
            uv[k][1] = (uint8_t) (tile->v + lv);
        }
        setXY4(p, xy[0][0], xy[0][1], xy[1][0], xy[1][1], xy[2][0], xy[2][1], xy[3][0], xy[3][1]);
        setUV4(p, uv[0][0], uv[0][1], uv[1][0], uv[1][1], uv[2][0], uv[2][1], uv[3][0], uv[3][1]);
        p->tpage = tile->tpage | ((mode & 3) << 5);
        p->clut = t->format == FORMAT_DIRECT ? 0
                  : stp ? getClut(t->stp_x, t->stp_y) : getClut(t->clut_x, t->clut_y);
    }
}

void rfvp_ps1_gpu_draw_rect(int32_t x0, int32_t y0, int32_t x1, int32_t y1, uint32_t rgba, int additive) {
    if (!frame_open) return;
    int stp;
    int mode = alpha_mode(rgba, additive, &stp);
    if (mode < 0) return;
    if (stp) {
        DR_TPAGE* tp = new_prim(sizeof(DR_TPAGE));
        if (!tp) return;
        setDrawTPage(tp, 0, 1, getTPage(0, mode, 0, 0));
    }
    POLY_F4* p = new_prim(sizeof(POLY_F4));
    if (!p) return;
    setPolyF4(p);
    int a = rgba >> 24;
    setRGB0(p, tint(rgba, 0, additive, a) * 2, tint(rgba, 8, additive, a) * 2, tint(rgba, 16, additive, a) * 2);
    if (stp) setSemiTrans(p, 1);
    setXY4(p, x0, y0, x1, y0, x0, y1, x1, y1);
}

void rfvp_ps1_gpu_end_frame(void) {
}

/* Show the frame drawn: wait for the GPU and the vertical blank, then swap. */
static void present(void) {
    if (!frame_open) return;
    frame_open = 0;
    if (!first_tag) return;
    termPrim(last_tag);
    DrawSync(0);
    VSync(0);
    PutDispEnv(&disp[db]);
    PutDrawEnv(&draw[db]);
    DrawOTag(first_tag);
    db ^= 1;
}

static void gpu_init(void) {
    ResetGraph(0);
    /* Draw into one buffer while the other is shown. */
    SetDefDispEnv(&disp[0], 0, 0, SCREEN_W, SCREEN_H);
    SetDefDrawEnv(&draw[0], 0, SCREEN_H, SCREEN_W, SCREEN_H);
    SetDefDispEnv(&disp[1], 0, SCREEN_H, SCREEN_W, SCREEN_H);
    SetDefDrawEnv(&draw[1], 0, 0, SCREEN_W, SCREEN_H);
    for (int i = 0; i < 2; i++) {
        draw[i].isbg = 1;
        setRGB0(&draw[i], 0, 0, 0);
        draw[i].dtd = 1; /* dither: smoother gradients from 15-bit colors */
    }
    PutDispEnv(&disp[1]);
    PutDrawEnv(&draw[0]);
    SetDispMask(1);
}

/* ---- Sound ---------------------------------------------------------------------------------- */

/* Sound RAM for sounds: after the SPU's reserved first 4 KiB, below its top (left for
 * reverb). Filled as a ring: an old sound is overwritten when the ring comes around. */
#define SPU_BASE 0x1010
#define SPU_TOP 0x7e000
/* While recording (see record_frame), what the sound hardware is told goes to audio.bin in the
 * PCdrv directory, so the soundtrack can be rebuilt offline: records of eight words (kind,
 * microseconds, six arguments), a sound RAM write followed by its bytes. */
static int audio_fd = -1; /* the recording file (record_frame's), while recording */
enum { A_WRITE = 1, A_PLAY, A_STOP, A_VOLUME, A_XA_BEGIN, A_XA_PAUSE, A_XA_VOLUME, A_LOOP };

static void alog(uint32_t kind, uint32_t a, uint32_t b, uint32_t c, uint32_t d, uint32_t e,
                 uint32_t f, const void* data, uint32_t len) {
    if (audio_fd < 0) return;
    uint32_t rec[9] = { 0x41564652u /* "RFVA" */, kind, (uint32_t) rfvp_ps1_ticks_us(), a, b, c, d, e, f };
    PCwrite(audio_fd, rec, sizeof(rec));
    if (len) PCwrite(audio_fd, data, len);
}

static uint32_t spu_next = SPU_BASE;

/* Copy SPU-ADPCM data to sound RAM; returns its address (0: too large). */
uint32_t rfvp_ps1_spu_upload(const uint8_t* data, uint32_t len) {
    len = (len + 63) & ~63u;
    if (len > SPU_TOP - SPU_BASE) return 0;
    if (spu_next + len > SPU_TOP) spu_next = SPU_BASE;
    uint32_t addr = spu_next;
    spu_next += len;
    SpuSetTransferMode(SPU_TRANSFER_BY_DMA);
    SpuSetTransferStartAddr(addr);
    SpuWrite((const uint32_t*) data, len);
    SpuIsTransferCompleted(SPU_TRANSFER_WAIT);
    alog(A_WRITE, addr, len, 0, 0, 0, 0, data, len);
    return addr;
}

/* Reserve `len` bytes of sound RAM (filled with rfvp_ps1_spu_write); 0 if too big. */
uint32_t rfvp_ps1_spu_alloc(uint32_t len) {
    len = (len + 63) & ~63u;
    if (len > SPU_TOP - SPU_BASE) return 0;
    if (spu_next + len > SPU_TOP) spu_next = SPU_BASE;
    uint32_t addr = spu_next;
    spu_next += len;
    return addr;
}

/* Copy `len` bytes (a multiple of 64) to sound RAM at `addr`. */
void rfvp_ps1_spu_write(uint32_t addr, const uint8_t* data, uint32_t len) {
    SpuSetTransferMode(SPU_TRANSFER_BY_DMA);
    SpuSetTransferStartAddr(addr);
    SpuWrite((const uint32_t*) data, len);
    SpuIsTransferCompleted(SPU_TRANSFER_WAIT);
    alog(A_WRITE, addr, len, 0, 0, 0, 0, data, len);
}

/* Set the flags of a sound in sound RAM: its first block starts the loop and its last ends
 * it (repeat), or the last ends the voice (once). */
void rfvp_ps1_spu_patch_loop(uint32_t addr, uint32_t len, int looped) {
    uint8_t block[16] __attribute__((aligned(4)));
    uint32_t last = addr + (len / 16 - 1) * 16;
    SpuSetTransferMode(SPU_TRANSFER_BY_IO);
    SpuSetTransferStartAddr(addr);
    SpuRead((uint32_t*) block, 16);
    block[1] = (last == addr) ? (looped ? 0x07 : 0x01) : (looped ? 0x04 : 0x00);
    SpuSetTransferStartAddr(addr);
    SpuWrite((uint32_t*) block, 16);
    if (last != addr) {
        SpuSetTransferStartAddr(last);
        SpuRead((uint32_t*) block, 16);
        block[1] = looped ? 0x03 : 0x01;
        SpuSetTransferStartAddr(last);
        SpuWrite((uint32_t*) block, 16);
    }
    SpuSetTransferMode(SPU_TRANSFER_BY_DMA);
    alog(A_LOOP, addr, len, looped, 0, 0, 0, 0, 0);
}

/* Play a sound on `voice` (0-23): volume 0-0x3fff per side. Looping sounds repeat from the
 * start (their loop flags are set when uploaded). */
void rfvp_ps1_spu_play(int voice, uint32_t addr, uint32_t rate, int vol_l, int vol_r) {
    SpuSetKey(0, 1 << voice);
    SPU_CH_FREQ(voice) = getSPUSampleRate(rate);
    SPU_CH_ADDR(voice) = getSPUAddr(addr);
    SPU_CH_VOL_L(voice) = vol_l;
    SPU_CH_VOL_R(voice) = vol_r;
    SPU_CH_ADSR1(voice) = 0x00ff;
    SPU_CH_ADSR2(voice) = 0x0000;
    SpuSetKey(1, 1 << voice);
    alog(A_PLAY, voice, addr, rate, vol_l, vol_r, 0, 0, 0);
}

void rfvp_ps1_spu_volume(int voice, int vol_l, int vol_r) {
    SPU_CH_VOL_L(voice) = vol_l;
    SPU_CH_VOL_R(voice) = vol_r;
    alog(A_VOLUME, voice, vol_l, vol_r, 0, 0, 0, 0, 0);
}

void rfvp_ps1_spu_stop(int voice) {
    SpuSetKey(0, 1 << voice);
    alog(A_STOP, voice, 0, 0, 0, 0, 0, 0, 0);
}

/* XA music: one channel of an interleaved file, from `start` (an absolute sector) for
 * `sectors` of that channel, repeating from `loop` when `repeat`. */
static uint32_t xa_start, xa_end, xa_loop, xa_resume_at;
static uint8_t xa_file, xa_channel;
static int xa_repeat, xa_channels, xa_suspended;

static void xa_begin(uint32_t lba) {
    uint8_t mode = CdlModeRT | CdlModeSF | CdlModeSpeed;
    CdlFILTER filter = { xa_file, xa_channel, 0 };
    CdlLOC loc;
    CdControl(CdlSetmode, &mode, 0);
    CdControl(CdlSetfilter, &filter, 0);
    CdIntToPos(lba, &loc);
    CdControl(CdlReadS, &loc, 0);
    cache_lba = 0xffffffffu; /* the drive has moved on */
    alog(A_XA_BEGIN, lba, xa_file, xa_channel, xa_channels, 0, 0, 0, 0);
}

void rfvp_ps1_xa_play(uint32_t start, uint32_t sectors, uint32_t loop, int file, int channel,
                      int channels, int repeat) {
    xa_start = start;
    xa_end = start + sectors * channels;
    xa_loop = start + loop * channels;
    xa_file = file;
    xa_channel = channel;
    xa_channels = channels;
    xa_repeat = repeat;
    xa_active = 1;
    xa_suspended = 0;
    xa_begin(start);
}

void rfvp_ps1_xa_stop(void) {
    if (!xa_active) return;
    xa_active = 0;
    CdControl(CdlPause, 0, 0);
    alog(A_XA_PAUSE, 0, 0, 0, 0, 0, 0, 0, 0);
}

void rfvp_ps1_xa_volume(int volume) {
    SPU_CD_VOL_L = volume;
    SPU_CD_VOL_R = volume;
    alog(A_XA_VOLUME, volume, 0, 0, 0, 0, 0, 0, 0);
}

static uint32_t drive_lba(void) {
    CdlLOCINFOP info;
    if (!CdControl(CdlGetlocP, 0, (uint8_t*) &info)) return 0;
    CdlLOC loc = { info.minute, info.second, info.sector, 0 };
    return CdPosToInt(&loc);
}

/* Before a file read: note where the music is and stop the drive playing it. */
static void xa_suspend(void) {
    if (!xa_active || xa_suspended) return;
    uint32_t at = drive_lba();
    xa_resume_at = (at >= xa_start && at < xa_end) ? at : xa_start;
    CdControl(CdlPause, 0, 0);
    xa_suspended = 1;
    alog(A_XA_PAUSE, 1, 0, 0, 0, 0, 0, 0, 0);
}

static void xa_resume(void) {
    if (!xa_active || !xa_suspended) return;
    xa_suspended = 0;
    xa_begin(xa_resume_at);
}

/* Once a frame: past its last sector, the music repeats or stops. */
static void xa_poll(void) {
    if (!xa_active || xa_suspended) return;
    uint32_t at = drive_lba();
    if (at && at >= xa_end) {
        if (xa_repeat) {
            xa_begin(xa_loop);
        } else {
            rfvp_ps1_xa_stop();
        }
    }
}

static void spu_setup(void) {
    SpuInit();
    SPU_MASTER_VOL_L = 0x3fff;
    SPU_MASTER_VOL_R = 0x3fff;
    /* CD audio (XA) into the mix. */
    SPU_CTRL |= 0x0001;
    rfvp_ps1_xa_volume(0x3fff);
}

/* ---- Input ---------------------------------------------------------------------------------- */

static uint8_t pad_buf[2][34];

static void read_input(RfvpPs1Input* in) {
    PADTYPE* pad = (PADTYPE*) pad_buf[0];
    in->buttons = 0;
    in->stick_x = in->stick_y = 128;
    /* Only a pad that answered (the buffer reads as every button held before it does). */
    if (pad->stat != 0 || (pad->type != PAD_ID_DIGITAL && pad->type != PAD_ID_ANALOG &&
                           pad->type != PAD_ID_ANALOG_STICK)) return;
    uint16_t held = ~pad->btn;
    static const struct { uint16_t pad; uint32_t bit; } map[] = {
        { PAD_CROSS, BUTTON_A }, { PAD_CIRCLE, BUTTON_B }, { PAD_TRIANGLE, BUTTON_X },
        { PAD_SQUARE, BUTTON_Y }, { PAD_SELECT, BUTTON_SELECT }, { PAD_START, BUTTON_START },
        { PAD_UP, BUTTON_UP }, { PAD_DOWN, BUTTON_DOWN }, { PAD_LEFT, BUTTON_LEFT },
        { PAD_RIGHT, BUTTON_RIGHT }, { PAD_L1, BUTTON_L }, { PAD_R1, BUTTON_R },
        { PAD_L2, BUTTON_ZL }, { PAD_R2, BUTTON_ZR },
    };
    for (unsigned i = 0; i < sizeof(map) / sizeof(map[0]); i++) {
        if (held & map[i].pad) in->buttons |= map[i].bit;
    }
    if (pad->type == PAD_ID_ANALOG || pad->type == PAD_ID_ANALOG_STICK) {
        in->stick_x = pad->ls_x;
        in->stick_y = pad->ls_y;
    }
}

/* ---- Debugging ----------------------------------------------------------------------------- */

/* DEBUG.TXT on the disc (optional): `shot=N` prints the screen at frame N to the TTY (as
 * hex rows between RFVPSHOT lines); `press=cross@600` presses a button at a frame. */
static unsigned shot_frame = 0xffffffffu;
/* `record=N` and `recordfor=M`: from frame N, M frames go to record.raw in the emulator's PCdrv
 * directory (an emulator only: on a console the break instructions hang). */
static unsigned record_from = 0xffffffffu, record_for = 0;
static int record_fd = -1;
#define MAX_PRESSES 64
static struct { uint32_t bit; unsigned frame; } presses[MAX_PRESSES];
static int press_count;
/* Scripted stick tilts, kept apart from the button bits. */
#define STICK_LEFT (1u << 28)
#define STICK_RIGHT (1u << 29)
#define STICK_UP (1u << 30)
#define STICK_DOWN (1u << 31)

static void read_debug_file(void) {
    static const struct { const char* name; uint32_t bit; } names[] = {
        { "cross", BUTTON_A }, { "circle", BUTTON_B }, { "triangle", BUTTON_X },
        { "square", BUTTON_Y }, { "up", BUTTON_UP }, { "down", BUTTON_DOWN },
        { "left", BUTTON_LEFT }, { "right", BUTTON_RIGHT }, { "start", BUTTON_START },
        { "select", BUTTON_SELECT }, { "l1", BUTTON_L }, { "r1", BUTTON_R },
        /* The left stick at full tilt (to move the cursor). */
        { "sleft", STICK_LEFT }, { "sright", STICK_RIGHT }, { "sup", STICK_UP },
        { "sdown", STICK_DOWN },
    };
    CdlFILE f;
    if (!CdSearchFile(&f, "\\DEBUG.TXT;1")) return;
    char* text = calloc(1, ((f.size + 2047) & ~2047) + 1);
    read_sectors(CdPosToInt(&f.pos), (f.size + 2047) / 2048, text);
    text[f.size] = 0;
    printf("rfvp: debug file: %u bytes: %.20s\n", (unsigned) f.size, text);
    /* Line by line (PSn00bSDK's strtok stops after the first). */
    for (char *line = text, *next; *line; line = next) {
        next = line;
        while (*next && *next != '\n' && *next != '\r') next++;
        if (*next) *next++ = 0;
        while (*next == '\n' || *next == '\r') next++;
        char name[16];
        unsigned frame;
        if (sscanf(line, "shot=%u", &frame) == 1) shot_frame = frame;
        if (sscanf(line, "record=%u", &frame) == 1) record_from = frame;
        if (sscanf(line, "recordfor=%u", &frame) == 1) record_for = frame;
        /* (PSn00bSDK's sscanf has no %[.) */
        char* at = strchr(line, '@');
        if (!strncmp(line, "press=", 6) && at && at - line - 6 < (int) sizeof(name) &&
            press_count < MAX_PRESSES) {
            memcpy(name, line + 6, at - line - 6);
            name[at - line - 6] = 0;
            frame = 0;
            for (const char* d = at + 1; *d >= '0' && *d <= '9'; d++) frame = frame * 10 + (*d - '0');
            for (unsigned i = 0; i < sizeof(names) / sizeof(names[0]); i++) {
                if (!strcmp(name, names[i].name)) {
                    presses[press_count].bit = names[i].bit;
                    presses[press_count].frame = frame;
                    press_count++;
                }
            }
        }
    }
    free(text);
    printf("rfvp: debug: shot at %u, %d presses\n", shot_frame, press_count);
}

static void save_screenshot(void) {
    /* The frame shown now (the last one presented). */
    RECT r;
    setRECT(&r, disp[db ^ 1].disp.x, disp[db ^ 1].disp.y, SCREEN_W, 1);
    static uint16_t row[SCREEN_W] __attribute__((aligned(4)));
    static const char hex[] = "0123456789abcdef";
    char line[64 * 4 + 1];
    DrawSync(0);
    printf("RFVPSHOT %d %d\n", SCREEN_W, SCREEN_H);
    for (int y = 0; y < SCREEN_H; y++) {
        r.y = disp[db ^ 1].disp.y + y;
        StoreImage(&r, (uint32_t*) row);
        DrawSync(0);
        /* Short lines (the TTY log splits long ones and loses characters), each tagged with
         * where it goes. */
        for (int x0 = 0; x0 < SCREEN_W; x0 += 64) {
            for (int x = 0; x < 64; x++) {
                uint16_t c = row[x0 + x];
                line[x * 4] = hex[c >> 12];
                line[x * 4 + 1] = hex[(c >> 8) & 15];
                line[x * 4 + 2] = hex[(c >> 4) & 15];
                line[x * 4 + 3] = hex[c & 15];
            }
            line[64 * 4] = 0;
            printf("S %d %d %s\n", y, x0, line);
        }
    }
    printf("RFVPSHOT end\n");
}

/* Whether frames are being recorded (audio.rs then logs what it plays, for the soundtrack). */
int rfvp_ps1_recording(void) {
    return record_fd >= 0;
}

/* Append the frame shown now to frames.raw: "RFVF", frame, microseconds, then 320x240 15-bit
 * pixels, a strip at a time (there is no room for a whole frame). */
static void record_frame(unsigned frame) {
    if (frame < record_from || frame >= record_from + record_for) {
        if (record_fd >= 0 && frame >= record_from + record_for) {
            PCclose(record_fd);
            record_fd = -1;
            audio_fd = -1;
            printf("rfvp: recording done\n");
        }
        return;
    }
    if (record_fd < 0) return;
    enum { STRIP = 16 };
    static uint16_t* strip;
    if (!strip) strip = malloc(SCREEN_W * STRIP * 2);
    uint32_t head[3] = { 0x46564652u, frame, (uint32_t) rfvp_ps1_ticks_us() };
    PCwrite(record_fd, head, sizeof(head));
    RECT r;
    DrawSync(0);
    for (int y = 0; y < SCREEN_H; y += STRIP) {
        setRECT(&r, disp[db ^ 1].disp.x, disp[db ^ 1].disp.y + y, SCREEN_W, STRIP);
        StoreImage(&r, (uint32_t*) strip);
        DrawSync(0);
        PCwrite(record_fd, strip, SCREEN_W * STRIP * 2);
    }
}

/* ---- Main ----------------------------------------------------------------------------------- */

extern uint8_t _end[];

int main(void) {
    /* The heap: from the end of the program to below the stack. */
    /* One heap for C and Rust: malloc and free are the Rust side's (ffi.rs). */
    rfvp_ps1_heap_init((void*) (((uintptr_t) _end + 15) & ~15u), RAM_TOP - STACK_SIZE - (((uintptr_t) _end + 15) & ~15u));
    gpu_init();
    spu_setup();
    InitPAD(pad_buf[0], 34, pad_buf[1], 34);
    StartPAD();
    ChangeClearPAD(0);
    CdInit();
    printf("rfvp: starting, heap %u KiB\n", (unsigned) ((RAM_TOP - STACK_SIZE - (((uintptr_t) _end + 15) & ~15u)) / 1024));
    load_file_map();
    read_debug_file();
    if (record_for) {
        /* One file for the frames and the sound events: DuckStation's PCdrv does not keep two
         * open. */
        PCinit();
        record_fd = PCcreat("record.raw");
        if (record_fd >= 0) {
            PCclose(record_fd);
            record_fd = PCopen("record.raw", PCDRV_MODE_WRITE);
        }
        audio_fd = record_fd;
        rfvp_ps1_xa_volume(0x3fff);
        printf("rfvp: recording %u frames from %u (audio log %d)\n", record_for, record_from, audio_fd);
    }

    Session* session = rfvp_ps1_start();
    if (!session) {
        printf("rfvp: the game could not be started\n");
        for (;;) VSync(0);
    }
    for (unsigned frame = 0;; frame++) {
        RfvpPs1Input input;
        read_input(&input);
        for (int i = 0; i < press_count; i++) {
            if (frame >= presses[i].frame && frame < presses[i].frame + 4) input.buttons |= presses[i].bit;
        }
        if (input.buttons & (STICK_LEFT | STICK_RIGHT | STICK_UP | STICK_DOWN)) {
            if (input.buttons & STICK_LEFT) input.stick_x = 0;
            if (input.buttons & STICK_RIGHT) input.stick_x = 255;
            if (input.buttons & STICK_UP) input.stick_y = 0;
            if (input.buttons & STICK_DOWN) input.stick_y = 255;
            input.buttons &= ~(STICK_LEFT | STICK_RIGHT | STICK_UP | STICK_DOWN);
        }
        int status = rfvp_ps1_frame(session, &input);
        present();
        if (frame == shot_frame) save_screenshot();
        record_frame(frame);
        xa_poll();
        if (status != 0) break;
    }
    rfvp_ps1_stop(session);
    printf("rfvp: stopped\n");
    for (;;) VSync(0);
}
