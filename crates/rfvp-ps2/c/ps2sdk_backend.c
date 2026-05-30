#include "ps2sdk_backend.h"

#include <debug.h>
#include <fileio.h>
#include <graph.h>
#include <kernel.h>
#include <libpad.h>
#include <loadfile.h>
#include <malloc.h>
#include <sifrpc.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <tamtypes.h>
#include <timer.h>

#define RFVP_PS2_WIDTH 640
#define RFVP_PS2_HEIGHT 448
#define RFVP_PS2_MAX_TEXTURES 256
#define RFVP_PS2_MAX_PATH 512

typedef struct Ps2Texture {
    uint8_t used;
    uint32_t width;
    uint32_t height;
    RawPixelFormat format;
    uint32_t *pixels;
} Ps2Texture;

typedef struct Ps2Backend {
    char root[RFVP_PS2_MAX_PATH];
    int should_exit;
    uint64_t tick_resolution;
    uint64_t start_tick;
    uint32_t *framebuffer;
    Ps2Texture textures[RFVP_PS2_MAX_TEXTURES];
    uint8_t pad_buffer[256] __attribute__((aligned(64)));
    uint32_t prev_buttons;
} Ps2Backend;

static Ps2Backend g_backend;

static int starts_with(const char *path, const char *prefix) {
    return strncmp(path, prefix, strlen(prefix)) == 0;
}

static int path_from_bytes(const uint8_t *path, size_t path_len, char *out, size_t out_len) {
    if (!path || !out || out_len == 0 || path_len >= out_len) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    memcpy(out, path, path_len);
    out[path_len] = '\0';
    return RFVP_PS2_OK;
}

static int resolve_path(Ps2Backend *backend, const uint8_t *path, size_t path_len, char *out, size_t out_len) {
    char local[RFVP_PS2_MAX_PATH];
    int status = path_from_bytes(path, path_len, local, sizeof(local));
    if (status != RFVP_PS2_OK) {
        return status;
    }

    if (starts_with(local, "host:/") || starts_with(local, "mass:/") || starts_with(local, "cdfs:/")) {
        if (strlen(local) >= out_len) {
            return RFVP_PS2_INVALID_ARGUMENT;
        }
        strcpy(out, local);
        return RFVP_PS2_OK;
    }

    size_t root_len = strlen(backend->root);
    size_t local_len = strlen(local);
    int needs_slash = root_len > 0 && backend->root[root_len - 1] != '/';
    if (root_len + (size_t)needs_slash + local_len + 1 > out_len) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }

    strcpy(out, backend->root);
    if (needs_slash) {
        strcat(out, "/");
    }
    strcat(out, local);
    return RFVP_PS2_OK;
}

static uint8_t clamp_u8(float value) {
    if (value <= 0.0f) {
        return 0;
    }
    if (value >= 1.0f) {
        return 255;
    }
    return (uint8_t)(value * 255.0f + 0.5f);
}

static uint32_t pack_color(RawColorRgba color) {
    uint32_t r = clamp_u8(color.r);
    uint32_t g = clamp_u8(color.g);
    uint32_t b = clamp_u8(color.b);
    uint32_t a = clamp_u8(color.a);
    return r | (g << 8) | (b << 16) | (a << 24);
}

static uint32_t blend_pixel(uint32_t dst, uint32_t src, RawBlendMode blend) {
    uint32_t sr = src & 0xff;
    uint32_t sg = (src >> 8) & 0xff;
    uint32_t sb = (src >> 16) & 0xff;
    uint32_t sa = (src >> 24) & 0xff;
    uint32_t dr = dst & 0xff;
    uint32_t dg = (dst >> 8) & 0xff;
    uint32_t db = (dst >> 16) & 0xff;

    if (blend == RawBlendMode_Opaque || sa == 255) {
        return src;
    }

    if (blend == RawBlendMode_Add) {
        uint32_t r = dr + ((sr * sa) / 255);
        uint32_t g = dg + ((sg * sa) / 255);
        uint32_t b = db + ((sb * sa) / 255);
        if (r > 255) r = 255;
        if (g > 255) g = 255;
        if (b > 255) b = 255;
        return r | (g << 8) | (b << 16) | 0xff000000u;
    }

    uint32_t inv = 255 - sa;
    uint32_t r = (sr * sa + dr * inv) / 255;
    uint32_t g = (sg * sa + dg * inv) / 255;
    uint32_t b = (sb * sa + db * inv) / 255;
    return r | (g << 8) | (b << 16) | 0xff000000u;
}

static int pixel_format_bpp(RawPixelFormat format) {
    switch (format) {
        case RawPixelFormat_Rgba8:
        case RawPixelFormat_Bgra8:
            return 4;
        case RawPixelFormat_Rgb8:
            return 3;
        case RawPixelFormat_Luma8:
        case RawPixelFormat_Alpha8:
            return 1;
        case RawPixelFormat_LumaA8:
            return 2;
        default:
            return 0;
    }
}

static uint32_t convert_pixel(RawPixelFormat format, const uint8_t *src) {
    switch (format) {
        case RawPixelFormat_Rgba8:
            return src[0] | (src[1] << 8) | (src[2] << 16) | (src[3] << 24);
        case RawPixelFormat_Bgra8:
            return src[2] | (src[1] << 8) | (src[0] << 16) | (src[3] << 24);
        case RawPixelFormat_Rgb8:
            return src[0] | (src[1] << 8) | (src[2] << 16) | 0xff000000u;
        case RawPixelFormat_Luma8:
            return src[0] | (src[0] << 8) | (src[0] << 16) | 0xff000000u;
        case RawPixelFormat_Alpha8:
            return 0x00ffffffu | (src[0] << 24);
        case RawPixelFormat_LumaA8:
            return src[0] | (src[0] << 8) | (src[0] << 16) | (src[1] << 24);
        default:
            return 0;
    }
}

static int upload_texture_pixels(
    Ps2Texture *texture,
    RawTextureRect rect,
    const uint8_t *pixels,
    size_t pixels_len,
    size_t stride) {
    if (!texture || !texture->used || !texture->pixels || !pixels) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    if (rect.x + rect.width > texture->width || rect.y + rect.height > texture->height) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    int bpp = pixel_format_bpp(texture->format);
    if (bpp == 0) {
        return RFVP_PS2_UNSUPPORTED;
    }
    if (stride == 0) {
        stride = rect.width * (size_t)bpp;
    }
    if (stride < rect.width * (size_t)bpp || pixels_len < stride * rect.height) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }

    for (uint32_t y = 0; y < rect.height; y++) {
        const uint8_t *src_row = pixels + y * stride;
        uint32_t *dst_row = texture->pixels + (rect.y + y) * texture->width + rect.x;
        for (uint32_t x = 0; x < rect.width; x++) {
            dst_row[x] = convert_pixel(texture->format, src_row + x * (uint32_t)bpp);
        }
    }
    return RFVP_PS2_OK;
}

static int ps2_fs_open(void *ctx, const uint8_t *path, size_t path_len, RawFileHandle *out_handle) {
    if (!ctx || !out_handle) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    char resolved[RFVP_PS2_MAX_PATH];
    int status = resolve_path((Ps2Backend *)ctx, path, path_len, resolved, sizeof(resolved));
    if (status != RFVP_PS2_OK) {
        return status;
    }
    int fd = fioOpen(resolved, O_RDONLY);
    if (fd < 0) {
        return RFVP_PS2_NOT_FOUND;
    }
    out_handle->value = (uint64_t)(uint32_t)fd;
    return RFVP_PS2_OK;
}

static void ps2_fs_close(void *ctx, RawFileHandle handle) {
    (void)ctx;
    if (handle.value != UINT64_MAX) {
        fioClose((int)(uint32_t)handle.value);
    }
}

static int ps2_fs_read_at(void *ctx, RawFileHandle handle, uint64_t offset, uint8_t *buf, size_t len, size_t *out_read) {
    (void)ctx;
    if (!buf || !out_read || offset > 0x7fffffffu) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    if (fioLseek((int)(uint32_t)handle.value, (int)offset, SEEK_SET) < 0) {
        return RFVP_PS2_IO;
    }
    int n = fioRead((int)(uint32_t)handle.value, buf, (int)len);
    if (n < 0) {
        return RFVP_PS2_IO;
    }
    *out_read = (size_t)n;
    return RFVP_PS2_OK;
}

static int ps2_fs_len(void *ctx, RawFileHandle handle, uint64_t *out_len) {
    (void)ctx;
    if (!out_len) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    int fd = (int)(uint32_t)handle.value;
    int cur = fioLseek(fd, 0, SEEK_CUR);
    int end = fioLseek(fd, 0, SEEK_END);
    if (cur < 0 || end < 0) {
        return RFVP_PS2_IO;
    }
    fioLseek(fd, cur, SEEK_SET);
    *out_len = (uint64_t)(uint32_t)end;
    return RFVP_PS2_OK;
}

static int ps2_fs_metadata(void *ctx, const uint8_t *path, size_t path_len, RawFileInfo *out_info) {
    if (!ctx || !out_info) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    char resolved[RFVP_PS2_MAX_PATH];
    int status = resolve_path((Ps2Backend *)ctx, path, path_len, resolved, sizeof(resolved));
    if (status != RFVP_PS2_OK) {
        return status;
    }
    fio_stat_t stat;
    memset(&stat, 0, sizeof(stat));
    if (fioGetstat(resolved, &stat) < 0) {
        return RFVP_PS2_NOT_FOUND;
    }
    out_info->len = (uint64_t)(uint32_t)stat.size;
    out_info->kind = (stat.mode & FIO_SO_IFDIR) ? RawFileKind_Directory : RawFileKind_File;
    return RFVP_PS2_OK;
}

static int ps2_fs_write_all(void *ctx, const uint8_t *path, size_t path_len, const uint8_t *bytes, size_t byte_len) {
    if (!ctx || (!bytes && byte_len != 0)) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    char resolved[RFVP_PS2_MAX_PATH];
    int status = resolve_path((Ps2Backend *)ctx, path, path_len, resolved, sizeof(resolved));
    if (status != RFVP_PS2_OK) {
        return status;
    }
    int fd = fioOpen(resolved, O_WRONLY | O_CREAT | O_TRUNC);
    if (fd < 0) {
        return RFVP_PS2_IO;
    }
    size_t written = 0;
    while (written < byte_len) {
        int n = fioWrite(fd, (void *)(bytes + written), (int)(byte_len - written));
        if (n <= 0) {
            fioClose(fd);
            return RFVP_PS2_IO;
        }
        written += (size_t)n;
    }
    fioClose(fd);
    return RFVP_PS2_OK;
}

static int extension_matches(const char *name, const char *extension) {
    const char *dot = strrchr(name, '.');
    if (!dot || dot[1] == '\0') {
        return 0;
    }
    return strcmp(dot + 1, extension) == 0;
}

static int ps2_fs_enumerate_by_extension(
    void *ctx,
    const uint8_t *root,
    size_t root_len,
    const uint8_t *extension,
    size_t extension_len,
    void *visitor_ctx,
    RawEnumerateVisitorFn visitor) {
    if (!ctx || !visitor) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    char resolved[RFVP_PS2_MAX_PATH];
    char ext[32];
    int status = resolve_path((Ps2Backend *)ctx, root, root_len, resolved, sizeof(resolved));
    if (status != RFVP_PS2_OK) {
        return status;
    }
    status = path_from_bytes(extension, extension_len, ext, sizeof(ext));
    if (status != RFVP_PS2_OK) {
        return status;
    }
    int dir = fioDopen(resolved);
    if (dir < 0) {
        return RFVP_PS2_NOT_FOUND;
    }
    fio_dirent_t entry;
    while (fioDread(dir, &entry) > 0) {
        if (!extension_matches(entry.name, ext)) {
            continue;
        }
        RawFileInfo info;
        info.len = (uint64_t)(uint32_t)entry.stat.size;
        info.kind = (entry.stat.mode & FIO_SO_IFDIR) ? RawFileKind_Directory : RawFileKind_File;
        status = visitor(visitor_ctx, (const uint8_t *)entry.name, strlen(entry.name), info);
        if (status != RFVP_PS2_OK) {
            fioDclose(dir);
            return status;
        }
    }
    fioDclose(dir);
    return RFVP_PS2_OK;
}

static int ps2_renderer_init(void *ctx, uint32_t width, uint32_t height) {
    Ps2Backend *backend = (Ps2Backend *)ctx;
    if (!backend || width != RFVP_PS2_WIDTH || height != RFVP_PS2_HEIGHT) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    if (!backend->framebuffer) {
        backend->framebuffer = (uint32_t *)memalign(64, RFVP_PS2_WIDTH * RFVP_PS2_HEIGHT * sizeof(uint32_t));
        if (!backend->framebuffer) {
            return RFVP_PS2_OUT_OF_MEMORY;
        }
    }
    memset(backend->framebuffer, 0, RFVP_PS2_WIDTH * RFVP_PS2_HEIGHT * sizeof(uint32_t));
    return RFVP_PS2_OK;
}

static void ps2_renderer_shutdown(void *ctx) {
    Ps2Backend *backend = (Ps2Backend *)ctx;
    if (!backend) {
        return;
    }
    for (uint32_t i = 0; i < RFVP_PS2_MAX_TEXTURES; i++) {
        if (backend->textures[i].pixels) {
            free(backend->textures[i].pixels);
        }
        memset(&backend->textures[i], 0, sizeof(backend->textures[i]));
    }
    if (backend->framebuffer) {
        free(backend->framebuffer);
        backend->framebuffer = NULL;
    }
}

static int ps2_renderer_create_texture(
    void *ctx,
    uint32_t texture_id,
    RawTextureDesc desc,
    const uint8_t *pixels,
    size_t pixels_len,
    size_t stride) {
    Ps2Backend *backend = (Ps2Backend *)ctx;
    if (!backend || texture_id >= RFVP_PS2_MAX_TEXTURES || desc.width == 0 || desc.height == 0) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    if (desc.mip_count > 1) {
        return RFVP_PS2_UNSUPPORTED;
    }
    int bpp = pixel_format_bpp(desc.format);
    if (bpp == 0) {
        return RFVP_PS2_UNSUPPORTED;
    }
    Ps2Texture *texture = &backend->textures[texture_id];
    if (texture->pixels) {
        free(texture->pixels);
    }
    memset(texture, 0, sizeof(*texture));
    texture->pixels = (uint32_t *)memalign(64, desc.width * desc.height * sizeof(uint32_t));
    if (!texture->pixels) {
        return RFVP_PS2_OUT_OF_MEMORY;
    }
    texture->used = 1;
    texture->width = desc.width;
    texture->height = desc.height;
    texture->format = desc.format;
    memset(texture->pixels, 0, desc.width * desc.height * sizeof(uint32_t));
    if (pixels) {
        RawTextureRect rect = {0, 0, desc.width, desc.height};
        return upload_texture_pixels(texture, rect, pixels, pixels_len, stride);
    }
    return RFVP_PS2_OK;
}

static int ps2_renderer_update_texture(
    void *ctx,
    uint32_t texture_id,
    RawTextureRect rect,
    const uint8_t *pixels,
    size_t pixels_len,
    size_t stride) {
    Ps2Backend *backend = (Ps2Backend *)ctx;
    if (!backend || texture_id >= RFVP_PS2_MAX_TEXTURES) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    return upload_texture_pixels(&backend->textures[texture_id], rect, pixels, pixels_len, stride);
}

static void ps2_renderer_destroy_texture(void *ctx, uint32_t texture_id) {
    Ps2Backend *backend = (Ps2Backend *)ctx;
    if (!backend || texture_id >= RFVP_PS2_MAX_TEXTURES) {
        return;
    }
    if (backend->textures[texture_id].pixels) {
        free(backend->textures[texture_id].pixels);
    }
    memset(&backend->textures[texture_id], 0, sizeof(backend->textures[texture_id]));
}

static int ps2_renderer_begin_frame(void *ctx, uint32_t width, uint32_t height, const RawColorRgba *clear) {
    Ps2Backend *backend = (Ps2Backend *)ctx;
    if (!backend || !backend->framebuffer || width != RFVP_PS2_WIDTH || height != RFVP_PS2_HEIGHT) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    uint32_t color = clear ? pack_color(*clear) : 0xff000000u;
    for (size_t i = 0; i < RFVP_PS2_WIDTH * RFVP_PS2_HEIGHT; i++) {
        backend->framebuffer[i] = color;
    }
    return RFVP_PS2_OK;
}

static int scissor_contains(const RawDrawSpriteCommand *cmd, int x, int y) {
    if (!cmd->has_scissor) {
        return 1;
    }
    return x >= cmd->scissor.x && y >= cmd->scissor.y &&
           x < cmd->scissor.x + cmd->scissor.width && y < cmd->scissor.y + cmd->scissor.height;
}

static float edge(float ax, float ay, float bx, float by, float cx, float cy) {
    return (cx - ax) * (by - ay) - (cy - ay) * (bx - ax);
}

static uint32_t sample_texture(const Ps2Texture *texture, float u, float v, RawTextureFilter filter) {
    if (!texture || !texture->used || !texture->pixels) {
        return 0;
    }
    if (filter == RawTextureFilter_Linear) {
        return RFVP_PS2_UNSUPPORTED;
    }
    int x = (int)(u * (float)(texture->width - 1) + 0.5f);
    int y = (int)(v * (float)(texture->height - 1) + 0.5f);
    if (x < 0) x = 0;
    if (y < 0) y = 0;
    if (x >= (int)texture->width) x = (int)texture->width - 1;
    if (y >= (int)texture->height) y = (int)texture->height - 1;
    return texture->pixels[y * texture->width + x];
}

static int draw_triangle(Ps2Backend *backend, const Ps2Texture *texture, const RawDrawSpriteCommand *cmd, int i0, int i1, int i2) {
    const RawVertex2D *a = &cmd->vertices[i0];
    const RawVertex2D *b = &cmd->vertices[i1];
    const RawVertex2D *c = &cmd->vertices[i2];
    float area = edge(a->position[0], a->position[1], b->position[0], b->position[1], c->position[0], c->position[1]);
    if (area == 0.0f) {
        return RFVP_PS2_OK;
    }
    int min_x = (int)a->position[0];
    int max_x = min_x;
    int min_y = (int)a->position[1];
    int max_y = min_y;
    const RawVertex2D *verts[3] = {a, b, c};
    for (int i = 1; i < 3; i++) {
        int x = (int)verts[i]->position[0];
        int y = (int)verts[i]->position[1];
        if (x < min_x) min_x = x;
        if (x > max_x) max_x = x;
        if (y < min_y) min_y = y;
        if (y > max_y) max_y = y;
    }
    if (min_x < 0) min_x = 0;
    if (min_y < 0) min_y = 0;
    if (max_x >= RFVP_PS2_WIDTH) max_x = RFVP_PS2_WIDTH - 1;
    if (max_y >= RFVP_PS2_HEIGHT) max_y = RFVP_PS2_HEIGHT - 1;

    for (int y = min_y; y <= max_y; y++) {
        for (int x = min_x; x <= max_x; x++) {
            if (!scissor_contains(cmd, x, y)) {
                continue;
            }
            float px = (float)x + 0.5f;
            float py = (float)y + 0.5f;
            float w0 = edge(b->position[0], b->position[1], c->position[0], c->position[1], px, py) / area;
            float w1 = edge(c->position[0], c->position[1], a->position[0], a->position[1], px, py) / area;
            float w2 = edge(a->position[0], a->position[1], b->position[0], b->position[1], px, py) / area;
            if (w0 < 0.0f || w1 < 0.0f || w2 < 0.0f) {
                continue;
            }
            float u = a->tex_coord[0] * w0 + b->tex_coord[0] * w1 + c->tex_coord[0] * w2;
            float v = a->tex_coord[1] * w0 + b->tex_coord[1] * w1 + c->tex_coord[1] * w2;
            uint32_t src = sample_texture(texture, u, v, cmd->filter);
            if (src == (uint32_t)RFVP_PS2_UNSUPPORTED) {
                return RFVP_PS2_UNSUPPORTED;
            }
            uint32_t tint = pack_color((RawColorRgba){
                a->color.r * w0 + b->color.r * w1 + c->color.r * w2,
                a->color.g * w0 + b->color.g * w1 + c->color.g * w2,
                a->color.b * w0 + b->color.b * w1 + c->color.b * w2,
                a->color.a * w0 + b->color.a * w1 + c->color.a * w2,
            });
            uint32_t tr = tint & 0xff;
            uint32_t tg = (tint >> 8) & 0xff;
            uint32_t tb = (tint >> 16) & 0xff;
            uint32_t ta = (tint >> 24) & 0xff;
            uint32_t sr = ((src & 0xff) * tr) / 255;
            uint32_t sg = (((src >> 8) & 0xff) * tg) / 255;
            uint32_t sb = (((src >> 16) & 0xff) * tb) / 255;
            uint32_t sa = (((src >> 24) & 0xff) * ta) / 255;
            uint32_t out = sr | (sg << 8) | (sb << 16) | (sa << 24);
            uint32_t *dst = &backend->framebuffer[y * RFVP_PS2_WIDTH + x];
            *dst = blend_pixel(*dst, out, cmd->blend);
        }
    }
    return RFVP_PS2_OK;
}

static int ps2_renderer_draw_sprite(void *ctx, const RawDrawSpriteCommand *command) {
    Ps2Backend *backend = (Ps2Backend *)ctx;
    if (!backend || !backend->framebuffer || !command || command->texture_id >= RFVP_PS2_MAX_TEXTURES) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    Ps2Texture *texture = &backend->textures[command->texture_id];
    if (!texture->used) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    int status = draw_triangle(backend, texture, command, 0, 1, 2);
    if (status != RFVP_PS2_OK) {
        return status;
    }
    return draw_triangle(backend, texture, command, 0, 2, 3);
}

static int ps2_renderer_draw_solid(void *ctx, const RawDrawSolidCommand *command) {
    Ps2Backend *backend = (Ps2Backend *)ctx;
    if (!backend || !backend->framebuffer || !command) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    int x0 = command->rect.x;
    int y0 = command->rect.y;
    int x1 = command->rect.x + command->rect.width;
    int y1 = command->rect.y + command->rect.height;
    if (x0 < 0) x0 = 0;
    if (y0 < 0) y0 = 0;
    if (x1 > RFVP_PS2_WIDTH) x1 = RFVP_PS2_WIDTH;
    if (y1 > RFVP_PS2_HEIGHT) y1 = RFVP_PS2_HEIGHT;
    uint32_t src = pack_color(command->color);
    for (int y = y0; y < y1; y++) {
        for (int x = x0; x < x1; x++) {
            if (command->has_scissor &&
                (x < command->scissor.x || y < command->scissor.y ||
                 x >= command->scissor.x + command->scissor.width ||
                 y >= command->scissor.y + command->scissor.height)) {
                continue;
            }
            uint32_t *dst = &backend->framebuffer[y * RFVP_PS2_WIDTH + x];
            *dst = blend_pixel(*dst, src, command->blend);
        }
    }
    return RFVP_PS2_OK;
}

static int ps2_renderer_end_frame(void *ctx) {
    (void)ctx;
    return RFVP_PS2_OK;
}

static int ps2_renderer_present(void *ctx) {
    (void)ctx;
    graph_wait_vsync();
    return RFVP_PS2_OK;
}

static int ps2_audio_unsupported(void) {
    return RFVP_PS2_UNSUPPORTED;
}

static int ps2_audio_load(void *ctx, uint32_t stream_id, const uint8_t *bytes, size_t byte_len) {
    (void)ctx; (void)stream_id; (void)bytes; (void)byte_len;
    return ps2_audio_unsupported();
}

static int ps2_audio_play(void *ctx, uint32_t stream_id, RawAudioParams params, uint32_t fade_in_ms) {
    (void)ctx; (void)stream_id; (void)params; (void)fade_in_ms;
    return ps2_audio_unsupported();
}

static int ps2_audio_stop(void *ctx, uint32_t stream_id, uint32_t fade_ms) {
    (void)ctx; (void)stream_id; (void)fade_ms;
    return ps2_audio_unsupported();
}

static int ps2_audio_pause(void *ctx, uint32_t stream_id) {
    (void)ctx; (void)stream_id;
    return ps2_audio_unsupported();
}

static int ps2_audio_resume(void *ctx, uint32_t stream_id) {
    (void)ctx; (void)stream_id;
    return ps2_audio_unsupported();
}

static int ps2_audio_set_params(void *ctx, uint32_t stream_id, RawAudioParams params) {
    (void)ctx; (void)stream_id; (void)params;
    return ps2_audio_unsupported();
}

static void ps2_audio_destroy(void *ctx, uint32_t stream_id) {
    (void)ctx; (void)stream_id;
}

static int ps2_audio_tick(void *ctx, uint64_t delta_us) {
    (void)ctx; (void)delta_us;
    return RFVP_PS2_OK;
}

static void push_button_edge(void *app, uint32_t current, uint32_t previous, uint32_t mask, uint32_t key_id) {
    int now = (current & mask) != 0;
    int before = (previous & mask) != 0;
    if (now != before) {
        rfvp_ps2_app_push_key(app, key_id, now ? 1 : 0);
    }
}

static uint64_t ps2_clock_ticks_us(void *ctx) {
    Ps2Backend *backend = (Ps2Backend *)ctx;
    uint64_t ticks = GetTimerSystemTime();
    if (backend && backend->tick_resolution != 0) {
        return ((ticks - backend->start_tick) * 1000000ULL) / backend->tick_resolution;
    }
    return 0;
}

static void ps2_log(void *ctx, uint32_t level, const uint8_t *message, size_t message_len) {
    (void)ctx;
    scr_printf("[rfvp:%u] ", level);
    for (size_t i = 0; i < message_len; i++) {
        scr_printf("%c", message[i]);
    }
    scr_printf("\n");
}

static void ps2_fatal(void *ctx, uint32_t code, const uint8_t *message, size_t message_len) {
    (void)ctx;
    scr_printf("rfvp fatal %u: ", code);
    for (size_t i = 0; i < message_len; i++) {
        scr_printf("%c", message[i]);
    }
    scr_printf("\n");
}

int rfvp_ps2_platform_init(int argc, char **argv) {
    memset(&g_backend, 0, sizeof(g_backend));
    SifInitRpc(0);
    init_scr();
    fioInit();
    padInit(0);
    if (padPortOpen(0, 0, g_backend.pad_buffer) == 0) {
        scr_printf("rfvp: padPortOpen failed\n");
    }
    TimerInit();

    const char *root = "mass:/";
    if (argc > 1 && argv && argv[1] && argv[1][0] != '\0') {
        root = argv[1];
    }
    if (strlen(root) >= sizeof(g_backend.root)) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    strcpy(g_backend.root, root);

    g_backend.tick_resolution = kBUSCLK;
    g_backend.start_tick = GetTimerSystemTime();
    return RFVP_PS2_OK;
}

void rfvp_ps2_platform_fini(void) {
    ps2_renderer_shutdown(&g_backend);
}

int rfvp_ps2_platform_poll(void *app) {
    struct padButtonStatus buttons;
    int state = padGetState(0, 0);
    if (state == PAD_STATE_STABLE || state == PAD_STATE_FINDCTP1) {
        if (padRead(0, 0, &buttons) != 0) {
            uint32_t pressed = (~buttons.btns) & 0xffffu;
            push_button_edge(app, pressed, g_backend.prev_buttons, PAD_CROSS, 1);
            push_button_edge(app, pressed, g_backend.prev_buttons, PAD_CIRCLE, 2);
            push_button_edge(app, pressed, g_backend.prev_buttons, PAD_TRIANGLE, 3);
            push_button_edge(app, pressed, g_backend.prev_buttons, PAD_SQUARE, 4);
            push_button_edge(app, pressed, g_backend.prev_buttons, PAD_LEFT, 5);
            push_button_edge(app, pressed, g_backend.prev_buttons, PAD_RIGHT, 6);
            push_button_edge(app, pressed, g_backend.prev_buttons, PAD_UP, 7);
            push_button_edge(app, pressed, g_backend.prev_buttons, PAD_DOWN, 8);
            push_button_edge(app, pressed, g_backend.prev_buttons, PAD_L1, 9);
            push_button_edge(app, pressed, g_backend.prev_buttons, PAD_R1, 10);
            if ((pressed & PAD_START) != 0 && (g_backend.prev_buttons & PAD_START) == 0) {
                g_backend.should_exit = 1;
                rfvp_ps2_app_push_quit(app);
            }
            g_backend.prev_buttons = pressed;
        }
    }
    return RFVP_PS2_OK;
}

int rfvp_ps2_platform_should_exit(void) {
    return g_backend.should_exit;
}

int rfvp_ps2_make_raw_host(RawPs2Host *out_host) {
    if (!out_host) {
        return RFVP_PS2_INVALID_ARGUMENT;
    }
    out_host->fs_ctx = &g_backend;
    out_host->fs.open = ps2_fs_open;
    out_host->fs.close = ps2_fs_close;
    out_host->fs.read_at = ps2_fs_read_at;
    out_host->fs.len = ps2_fs_len;
    out_host->fs.metadata = ps2_fs_metadata;
    out_host->fs.write_all = ps2_fs_write_all;
    out_host->fs.enumerate_by_extension = ps2_fs_enumerate_by_extension;

    out_host->renderer_ctx = &g_backend;
    out_host->renderer.init = ps2_renderer_init;
    out_host->renderer.shutdown = ps2_renderer_shutdown;
    out_host->renderer.create_texture = ps2_renderer_create_texture;
    out_host->renderer.update_texture = ps2_renderer_update_texture;
    out_host->renderer.destroy_texture = ps2_renderer_destroy_texture;
    out_host->renderer.begin_frame = ps2_renderer_begin_frame;
    out_host->renderer.draw_sprite = ps2_renderer_draw_sprite;
    out_host->renderer.draw_solid = ps2_renderer_draw_solid;
    out_host->renderer.end_frame = ps2_renderer_end_frame;
    out_host->renderer.present = ps2_renderer_present;

    out_host->audio_ctx = &g_backend;
    out_host->audio.load_native = ps2_audio_load;
    out_host->audio.play = ps2_audio_play;
    out_host->audio.stop = ps2_audio_stop;
    out_host->audio.pause = ps2_audio_pause;
    out_host->audio.resume = ps2_audio_resume;
    out_host->audio.set_params = ps2_audio_set_params;
    out_host->audio.destroy = ps2_audio_destroy;
    out_host->audio.tick = ps2_audio_tick;

    out_host->clock_ctx = &g_backend;
    out_host->clock.ticks_us = ps2_clock_ticks_us;

    out_host->log_ctx = &g_backend;
    out_host->log = ps2_log;
    out_host->fatal_ctx = &g_backend;
    out_host->fatal = ps2_fatal;
    return RFVP_PS2_OK;
}
