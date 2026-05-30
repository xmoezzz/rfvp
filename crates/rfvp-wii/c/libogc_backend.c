#include "libogc_backend.h"

#include <dirent.h>
#include <fat.h>
#include <gccore.h>
#include <limits.h>
#include <malloc.h>
#include <ogc/lwp_watchdog.h>
#include <ogc/machine/processor.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include <wiiuse/wpad.h>

#define RFVP_WII_WIDTH 640
#define RFVP_WII_HEIGHT 480
#define RFVP_WII_MAX_TEXTURES 256
#define RFVP_WII_MAX_PATH 512

typedef struct WiiTexture {
    uint8_t used;
    uint32_t width;
    uint32_t height;
    RawPixelFormat format;
    uint32_t *pixels;
} WiiTexture;

typedef struct WiiBackend {
    char root[RFVP_WII_MAX_PATH];
    int should_exit;
    uint64_t start_tick;
    uint32_t *framebuffer;
    GXRModeObj *rmode;
    void *xfb;
    uint32_t xfb_width;
    uint32_t xfb_height;
    WiiTexture textures[RFVP_WII_MAX_TEXTURES];
    uint32_t prev_buttons;
} WiiBackend;

static WiiBackend g_backend;

static int starts_with(const char *path, const char *prefix) {
    return strncmp(path, prefix, strlen(prefix)) == 0;
}

static int path_from_bytes(const uint8_t *path, size_t path_len, char *out, size_t out_len) {
    if (!path || !out || out_len == 0 || path_len >= out_len) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    memcpy(out, path, path_len);
    out[path_len] = '\0';
    return RFVP_WII_OK;
}

static int resolve_path(WiiBackend *backend, const uint8_t *path, size_t path_len, char *out, size_t out_len) {
    char local[RFVP_WII_MAX_PATH];
    int status = path_from_bytes(path, path_len, local, sizeof(local));
    if (status != RFVP_WII_OK) {
        return status;
    }

    if (starts_with(local, "sd:/") || starts_with(local, "usb:/") || starts_with(local, "dvd:/")) {
        if (strlen(local) >= out_len) {
            return RFVP_WII_INVALID_ARGUMENT;
        }
        strcpy(out, local);
        return RFVP_WII_OK;
    }

    size_t root_len = strlen(backend->root);
    size_t local_len = strlen(local);
    int needs_slash = root_len > 0 && backend->root[root_len - 1] != '/';
    if (root_len + (size_t)needs_slash + local_len + 1 > out_len) {
        return RFVP_WII_INVALID_ARGUMENT;
    }

    strcpy(out, backend->root);
    if (needs_slash) {
        strcat(out, "/");
    }
    strcat(out, local);
    return RFVP_WII_OK;
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
    WiiTexture *texture,
    RawTextureRect rect,
    const uint8_t *pixels,
    size_t pixels_len,
    size_t stride) {
    if (!texture || !texture->used || !texture->pixels || !pixels) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    if (rect.x + rect.width > texture->width || rect.y + rect.height > texture->height) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    int bpp = pixel_format_bpp(texture->format);
    if (bpp == 0) {
        return RFVP_WII_UNSUPPORTED;
    }
    if (stride == 0) {
        stride = rect.width * (size_t)bpp;
    }
    if (stride < rect.width * (size_t)bpp || pixels_len < stride * rect.height) {
        return RFVP_WII_INVALID_ARGUMENT;
    }

    for (uint32_t y = 0; y < rect.height; y++) {
        const uint8_t *src_row = pixels + y * stride;
        uint32_t *dst_row = texture->pixels + (rect.y + y) * texture->width + rect.x;
        for (uint32_t x = 0; x < rect.width; x++) {
            dst_row[x] = convert_pixel(texture->format, src_row + x * (uint32_t)bpp);
        }
    }
    return RFVP_WII_OK;
}

static int wii_fs_open(void *ctx, const uint8_t *path, size_t path_len, RawFileHandle *out_handle) {
    if (!ctx || !out_handle) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    char resolved[RFVP_WII_MAX_PATH];
    int status = resolve_path((WiiBackend *)ctx, path, path_len, resolved, sizeof(resolved));
    if (status != RFVP_WII_OK) {
        return status;
    }
    FILE *file = fopen(resolved, "rb");
    if (!file) {
        return RFVP_WII_NOT_FOUND;
    }
    out_handle->value = (uint64_t)(uintptr_t)file;
    return RFVP_WII_OK;
}

static void wii_fs_close(void *ctx, RawFileHandle handle) {
    (void)ctx;
    if (handle.value != UINT64_MAX) {
        fclose((FILE *)(uintptr_t)handle.value);
    }
}

static int wii_fs_read_at(void *ctx, RawFileHandle handle, uint64_t offset, uint8_t *buf, size_t len, size_t *out_read) {
    (void)ctx;
    if (!buf || !out_read || offset > LONG_MAX) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    FILE *file = (FILE *)(uintptr_t)handle.value;
    if (!file || fseek(file, (long)offset, SEEK_SET) != 0) {
        return RFVP_WII_IO;
    }
    size_t n = fread(buf, 1, len, file);
    if (n < len && ferror(file)) {
        return RFVP_WII_IO;
    }
    *out_read = n;
    return RFVP_WII_OK;
}

static int wii_fs_len(void *ctx, RawFileHandle handle, uint64_t *out_len) {
    (void)ctx;
    if (!out_len) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    FILE *file = (FILE *)(uintptr_t)handle.value;
    if (!file) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    long cur = ftell(file);
    if (cur < 0 || fseek(file, 0, SEEK_END) != 0) {
        return RFVP_WII_IO;
    }
    long end = ftell(file);
    if (end < 0 || fseek(file, cur, SEEK_SET) != 0) {
        return RFVP_WII_IO;
    }
    *out_len = (uint64_t)end;
    return RFVP_WII_OK;
}

static int wii_fs_metadata(void *ctx, const uint8_t *path, size_t path_len, RawFileInfo *out_info) {
    if (!ctx || !out_info) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    char resolved[RFVP_WII_MAX_PATH];
    int status = resolve_path((WiiBackend *)ctx, path, path_len, resolved, sizeof(resolved));
    if (status != RFVP_WII_OK) {
        return status;
    }
    struct stat stat_buf;
    memset(&stat_buf, 0, sizeof(stat_buf));
    if (stat(resolved, &stat_buf) != 0) {
        return RFVP_WII_NOT_FOUND;
    }
    out_info->len = (uint64_t)stat_buf.st_size;
    out_info->kind = S_ISDIR(stat_buf.st_mode) ? RawFileKind_Directory : RawFileKind_File;
    return RFVP_WII_OK;
}

static int wii_fs_write_all(void *ctx, const uint8_t *path, size_t path_len, const uint8_t *bytes, size_t byte_len) {
    if (!ctx || (!bytes && byte_len != 0)) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    char resolved[RFVP_WII_MAX_PATH];
    int status = resolve_path((WiiBackend *)ctx, path, path_len, resolved, sizeof(resolved));
    if (status != RFVP_WII_OK) {
        return status;
    }
    FILE *file = fopen(resolved, "wb");
    if (!file) {
        return RFVP_WII_IO;
    }
    if (byte_len != 0 && fwrite(bytes, 1, byte_len, file) != byte_len) {
        fclose(file);
        return RFVP_WII_IO;
    }
    fclose(file);
    return RFVP_WII_OK;
}

static int extension_matches(const char *name, const char *extension) {
    const char *dot = strrchr(name, '.');
    if (!dot || dot[1] == '\0') {
        return 0;
    }
    return strcmp(dot + 1, extension) == 0;
}

static int wii_fs_enumerate_by_extension(
    void *ctx,
    const uint8_t *root,
    size_t root_len,
    const uint8_t *extension,
    size_t extension_len,
    void *visitor_ctx,
    RawEnumerateVisitorFn visitor) {
    if (!ctx || !visitor) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    char resolved[RFVP_WII_MAX_PATH];
    char ext[32];
    int status = resolve_path((WiiBackend *)ctx, root, root_len, resolved, sizeof(resolved));
    if (status != RFVP_WII_OK) {
        return status;
    }
    status = path_from_bytes(extension, extension_len, ext, sizeof(ext));
    if (status != RFVP_WII_OK) {
        return status;
    }
    DIR *dir = opendir(resolved);
    if (!dir) {
        return RFVP_WII_NOT_FOUND;
    }
    struct dirent *entry = NULL;
    while ((entry = readdir(dir)) != NULL) {
        if (!extension_matches(entry->d_name, ext)) {
            continue;
        }
        char child[RFVP_WII_MAX_PATH];
        if (snprintf(child, sizeof(child), "%s/%s", resolved, entry->d_name) >= (int)sizeof(child)) {
            closedir(dir);
            return RFVP_WII_INVALID_ARGUMENT;
        }
        struct stat stat_buf;
        memset(&stat_buf, 0, sizeof(stat_buf));
        if (stat(child, &stat_buf) != 0) {
            closedir(dir);
            return RFVP_WII_IO;
        }
        RawFileInfo info;
        info.len = (uint64_t)stat_buf.st_size;
        info.kind = S_ISDIR(stat_buf.st_mode) ? RawFileKind_Directory : RawFileKind_File;
        status = visitor(visitor_ctx, (const uint8_t *)entry->d_name, strlen(entry->d_name), info);
        if (status != RFVP_WII_OK) {
            closedir(dir);
            return status;
        }
    }
    closedir(dir);
    return RFVP_WII_OK;
}

static int wii_renderer_init(void *ctx, uint32_t width, uint32_t height) {
    WiiBackend *backend = (WiiBackend *)ctx;
    if (!backend || width != RFVP_WII_WIDTH || height != RFVP_WII_HEIGHT) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    if (!backend->framebuffer) {
        backend->framebuffer = (uint32_t *)memalign(64, RFVP_WII_WIDTH * RFVP_WII_HEIGHT * sizeof(uint32_t));
        if (!backend->framebuffer) {
            return RFVP_WII_OUT_OF_MEMORY;
        }
    }
    memset(backend->framebuffer, 0, RFVP_WII_WIDTH * RFVP_WII_HEIGHT * sizeof(uint32_t));
    return RFVP_WII_OK;
}

static void wii_renderer_shutdown(void *ctx) {
    WiiBackend *backend = (WiiBackend *)ctx;
    if (!backend) {
        return;
    }
    for (uint32_t i = 0; i < RFVP_WII_MAX_TEXTURES; i++) {
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

static int wii_renderer_create_texture(
    void *ctx,
    uint32_t texture_id,
    RawTextureDesc desc,
    const uint8_t *pixels,
    size_t pixels_len,
    size_t stride) {
    WiiBackend *backend = (WiiBackend *)ctx;
    if (!backend || texture_id >= RFVP_WII_MAX_TEXTURES || desc.width == 0 || desc.height == 0) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    if (desc.mip_count > 1) {
        return RFVP_WII_UNSUPPORTED;
    }
    int bpp = pixel_format_bpp(desc.format);
    if (bpp == 0) {
        return RFVP_WII_UNSUPPORTED;
    }
    WiiTexture *texture = &backend->textures[texture_id];
    if (texture->pixels) {
        free(texture->pixels);
    }
    memset(texture, 0, sizeof(*texture));
    texture->pixels = (uint32_t *)memalign(64, desc.width * desc.height * sizeof(uint32_t));
    if (!texture->pixels) {
        return RFVP_WII_OUT_OF_MEMORY;
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
    return RFVP_WII_OK;
}

static int wii_renderer_update_texture(
    void *ctx,
    uint32_t texture_id,
    RawTextureRect rect,
    const uint8_t *pixels,
    size_t pixels_len,
    size_t stride) {
    WiiBackend *backend = (WiiBackend *)ctx;
    if (!backend || texture_id >= RFVP_WII_MAX_TEXTURES) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    return upload_texture_pixels(&backend->textures[texture_id], rect, pixels, pixels_len, stride);
}

static void wii_renderer_destroy_texture(void *ctx, uint32_t texture_id) {
    WiiBackend *backend = (WiiBackend *)ctx;
    if (!backend || texture_id >= RFVP_WII_MAX_TEXTURES) {
        return;
    }
    if (backend->textures[texture_id].pixels) {
        free(backend->textures[texture_id].pixels);
    }
    memset(&backend->textures[texture_id], 0, sizeof(backend->textures[texture_id]));
}

static int wii_renderer_begin_frame(void *ctx, uint32_t width, uint32_t height, const RawColorRgba *clear) {
    WiiBackend *backend = (WiiBackend *)ctx;
    if (!backend || !backend->framebuffer || width != RFVP_WII_WIDTH || height != RFVP_WII_HEIGHT) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    uint32_t color = clear ? pack_color(*clear) : 0xff000000u;
    for (size_t i = 0; i < RFVP_WII_WIDTH * RFVP_WII_HEIGHT; i++) {
        backend->framebuffer[i] = color;
    }
    return RFVP_WII_OK;
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

static uint32_t sample_texture(const WiiTexture *texture, float u, float v, RawTextureFilter filter) {
    if (!texture || !texture->used || !texture->pixels) {
        return 0;
    }
    if (filter == RawTextureFilter_Linear) {
        return RFVP_WII_UNSUPPORTED;
    }
    int x = (int)(u * (float)(texture->width - 1) + 0.5f);
    int y = (int)(v * (float)(texture->height - 1) + 0.5f);
    if (x < 0) x = 0;
    if (y < 0) y = 0;
    if (x >= (int)texture->width) x = (int)texture->width - 1;
    if (y >= (int)texture->height) y = (int)texture->height - 1;
    return texture->pixels[y * texture->width + x];
}

static int draw_triangle(WiiBackend *backend, const WiiTexture *texture, const RawDrawSpriteCommand *cmd, int i0, int i1, int i2) {
    const RawVertex2D *a = &cmd->vertices[i0];
    const RawVertex2D *b = &cmd->vertices[i1];
    const RawVertex2D *c = &cmd->vertices[i2];
    float area = edge(a->position[0], a->position[1], b->position[0], b->position[1], c->position[0], c->position[1]);
    if (area == 0.0f) {
        return RFVP_WII_OK;
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
    if (max_x >= RFVP_WII_WIDTH) max_x = RFVP_WII_WIDTH - 1;
    if (max_y >= RFVP_WII_HEIGHT) max_y = RFVP_WII_HEIGHT - 1;

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
            if (src == (uint32_t)RFVP_WII_UNSUPPORTED) {
                return RFVP_WII_UNSUPPORTED;
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
            uint32_t *dst = &backend->framebuffer[y * RFVP_WII_WIDTH + x];
            *dst = blend_pixel(*dst, out, cmd->blend);
        }
    }
    return RFVP_WII_OK;
}

static int wii_renderer_draw_sprite(void *ctx, const RawDrawSpriteCommand *command) {
    WiiBackend *backend = (WiiBackend *)ctx;
    if (!backend || !backend->framebuffer || !command || command->texture_id >= RFVP_WII_MAX_TEXTURES) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    WiiTexture *texture = &backend->textures[command->texture_id];
    if (!texture->used) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    int status = draw_triangle(backend, texture, command, 0, 1, 2);
    if (status != RFVP_WII_OK) {
        return status;
    }
    return draw_triangle(backend, texture, command, 0, 2, 3);
}

static int wii_renderer_draw_solid(void *ctx, const RawDrawSolidCommand *command) {
    WiiBackend *backend = (WiiBackend *)ctx;
    if (!backend || !backend->framebuffer || !command) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    int x0 = command->rect.x;
    int y0 = command->rect.y;
    int x1 = command->rect.x + command->rect.width;
    int y1 = command->rect.y + command->rect.height;
    if (x0 < 0) x0 = 0;
    if (y0 < 0) y0 = 0;
    if (x1 > RFVP_WII_WIDTH) x1 = RFVP_WII_WIDTH;
    if (y1 > RFVP_WII_HEIGHT) y1 = RFVP_WII_HEIGHT;
    uint32_t src = pack_color(command->color);
    for (int y = y0; y < y1; y++) {
        for (int x = x0; x < x1; x++) {
            if (command->has_scissor &&
                (x < command->scissor.x || y < command->scissor.y ||
                 x >= command->scissor.x + command->scissor.width ||
                 y >= command->scissor.y + command->scissor.height)) {
                continue;
            }
            uint32_t *dst = &backend->framebuffer[y * RFVP_WII_WIDTH + x];
            *dst = blend_pixel(*dst, src, command->blend);
        }
    }
    return RFVP_WII_OK;
}

static int wii_renderer_end_frame(void *ctx) {
    (void)ctx;
    return RFVP_WII_OK;
}

static uint8_t clamp_video_i32(int value) {
    if (value < 0) {
        return 0;
    }
    if (value > 255) {
        return 255;
    }
    return (uint8_t)value;
}

static void rgba_to_ycbcr(uint32_t rgba, uint8_t *y, uint8_t *cb, uint8_t *cr) {
    int r = (int)(rgba & 0xff);
    int g = (int)((rgba >> 8) & 0xff);
    int b = (int)((rgba >> 16) & 0xff);
    *y = clamp_video_i32(((66 * r + 129 * g + 25 * b + 128) >> 8) + 16);
    *cb = clamp_video_i32(((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128);
    *cr = clamp_video_i32(((112 * r - 94 * g - 18 * b + 128) >> 8) + 128);
}

static int wii_renderer_present(void *ctx) {
    WiiBackend *backend = (WiiBackend *)ctx;
    if (!backend || !backend->framebuffer || !backend->xfb || !backend->rmode) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    uint32_t *xfb = (uint32_t *)backend->xfb;
    uint32_t out_w = backend->xfb_width;
    uint32_t out_h = backend->xfb_height;
    for (uint32_t y = 0; y < out_h; y++) {
        uint32_t src_y = (y * RFVP_WII_HEIGHT) / out_h;
        for (uint32_t x = 0; x < out_w; x += 2) {
            uint32_t src_x0 = (x * RFVP_WII_WIDTH) / out_w;
            uint32_t src_x1 = (((x + 1) < out_w ? (x + 1) : x) * RFVP_WII_WIDTH) / out_w;
            uint32_t p0 = backend->framebuffer[src_y * RFVP_WII_WIDTH + src_x0];
            uint32_t p1 = backend->framebuffer[src_y * RFVP_WII_WIDTH + src_x1];
            uint8_t y0, cb0, cr0, y1, cb1, cr1;
            rgba_to_ycbcr(p0, &y0, &cb0, &cr0);
            rgba_to_ycbcr(p1, &y1, &cb1, &cr1);
            uint8_t cb = (uint8_t)(((uint32_t)cb0 + (uint32_t)cb1) / 2);
            uint8_t cr = (uint8_t)(((uint32_t)cr0 + (uint32_t)cr1) / 2);
            xfb[y * (out_w / 2) + (x / 2)] = ((uint32_t)y0 << 24) | ((uint32_t)cb << 16) | ((uint32_t)y1 << 8) | cr;
        }
    }
    DCFlushRange(backend->xfb, VIDEO_GetFrameBufferSize(backend->rmode));
    VIDEO_SetNextFramebuffer(backend->xfb);
    VIDEO_Flush();
    VIDEO_WaitVSync();
    return RFVP_WII_OK;
}

static int wii_audio_unsupported(void) {
    return RFVP_WII_UNSUPPORTED;
}

static int wii_audio_load(void *ctx, uint32_t stream_id, const uint8_t *bytes, size_t byte_len) {
    (void)ctx; (void)stream_id; (void)bytes; (void)byte_len;
    return wii_audio_unsupported();
}

static int wii_audio_play(void *ctx, uint32_t stream_id, RawAudioParams params, uint32_t fade_in_ms) {
    (void)ctx; (void)stream_id; (void)params; (void)fade_in_ms;
    return wii_audio_unsupported();
}

static int wii_audio_stop(void *ctx, uint32_t stream_id, uint32_t fade_ms) {
    (void)ctx; (void)stream_id; (void)fade_ms;
    return wii_audio_unsupported();
}

static int wii_audio_pause(void *ctx, uint32_t stream_id) {
    (void)ctx; (void)stream_id;
    return wii_audio_unsupported();
}

static int wii_audio_resume(void *ctx, uint32_t stream_id) {
    (void)ctx; (void)stream_id;
    return wii_audio_unsupported();
}

static int wii_audio_set_params(void *ctx, uint32_t stream_id, RawAudioParams params) {
    (void)ctx; (void)stream_id; (void)params;
    return wii_audio_unsupported();
}

static void wii_audio_destroy(void *ctx, uint32_t stream_id) {
    (void)ctx; (void)stream_id;
}

static int wii_audio_tick(void *ctx, uint64_t delta_us) {
    (void)ctx; (void)delta_us;
    return RFVP_WII_OK;
}

static void push_button_edge(void *app, uint32_t current, uint32_t previous, uint32_t mask, uint32_t key_id) {
    int now = (current & mask) != 0;
    int before = (previous & mask) != 0;
    if (now != before) {
        rfvp_wii_app_push_key(app, key_id, now ? 1 : 0);
    }
}

static uint64_t wii_clock_ticks_us(void *ctx) {
    WiiBackend *backend = (WiiBackend *)ctx;
    uint64_t ticks = gettime();
    if (backend) {
        return ticks_to_microsecs(ticks - backend->start_tick);
    }
    return 0;
}

static void wii_log(void *ctx, uint32_t level, const uint8_t *message, size_t message_len) {
    (void)ctx;
    printf("[rfvp:%u] ", level);
    for (size_t i = 0; i < message_len; i++) {
        putchar(message[i]);
    }
    putchar('\n');
}

static void wii_fatal(void *ctx, uint32_t code, const uint8_t *message, size_t message_len) {
    (void)ctx;
    printf("rfvp fatal %u: ", code);
    for (size_t i = 0; i < message_len; i++) {
        putchar(message[i]);
    }
    putchar('\n');
}

int rfvp_wii_platform_init(int argc, char **argv) {
    memset(&g_backend, 0, sizeof(g_backend));

    VIDEO_Init();
    WPAD_Init();
    fatInitDefault();

    g_backend.rmode = VIDEO_GetPreferredMode(NULL);
    if (!g_backend.rmode) {
        return RFVP_WII_BACKEND;
    }
    g_backend.xfb = MEM_K0_TO_K1(SYS_AllocateFramebuffer(g_backend.rmode));
    if (!g_backend.xfb) {
        return RFVP_WII_OUT_OF_MEMORY;
    }
    g_backend.xfb_width = g_backend.rmode->fbWidth;
    g_backend.xfb_height = g_backend.rmode->xfbHeight;
    VIDEO_Configure(g_backend.rmode);
    VIDEO_ClearFrameBuffer(g_backend.rmode, g_backend.xfb, COLOR_BLACK);
    VIDEO_SetNextFramebuffer(g_backend.xfb);
    VIDEO_SetBlack(FALSE);
    VIDEO_Flush();
    VIDEO_WaitVSync();
    if (g_backend.rmode->viTVMode & VI_NON_INTERLACE) {
        VIDEO_WaitVSync();
    }
    console_init(g_backend.xfb, 20, 20, g_backend.rmode->fbWidth, g_backend.rmode->xfbHeight, g_backend.rmode->fbWidth * VI_DISPLAY_PIX_SZ);

    const char *root = "sd:/";
    if (argc > 1 && argv && argv[1] && argv[1][0] != '\0') {
        root = argv[1];
    }
    if (strlen(root) >= sizeof(g_backend.root)) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    strcpy(g_backend.root, root);

    g_backend.start_tick = gettime();
    return RFVP_WII_OK;
}

void rfvp_wii_platform_fini(void) {
    wii_renderer_shutdown(&g_backend);
}

int rfvp_wii_platform_poll(void *app) {
    WPAD_ScanPads();
    uint32_t pressed = WPAD_ButtonsHeld(0);
    push_button_edge(app, pressed, g_backend.prev_buttons, WPAD_BUTTON_A, 1);
    push_button_edge(app, pressed, g_backend.prev_buttons, WPAD_BUTTON_B, 2);
    push_button_edge(app, pressed, g_backend.prev_buttons, WPAD_BUTTON_1, 3);
    push_button_edge(app, pressed, g_backend.prev_buttons, WPAD_BUTTON_2, 4);
    push_button_edge(app, pressed, g_backend.prev_buttons, WPAD_BUTTON_LEFT, 5);
    push_button_edge(app, pressed, g_backend.prev_buttons, WPAD_BUTTON_RIGHT, 6);
    push_button_edge(app, pressed, g_backend.prev_buttons, WPAD_BUTTON_UP, 7);
    push_button_edge(app, pressed, g_backend.prev_buttons, WPAD_BUTTON_DOWN, 8);
    push_button_edge(app, pressed, g_backend.prev_buttons, WPAD_BUTTON_MINUS, 9);
    push_button_edge(app, pressed, g_backend.prev_buttons, WPAD_BUTTON_PLUS, 10);
    if ((pressed & WPAD_BUTTON_HOME) != 0 && (g_backend.prev_buttons & WPAD_BUTTON_HOME) == 0) {
        g_backend.should_exit = 1;
        rfvp_wii_app_push_quit(app);
    }
    g_backend.prev_buttons = pressed;
    return RFVP_WII_OK;
}

int rfvp_wii_platform_should_exit(void) {
    return g_backend.should_exit;
}

int rfvp_wii_make_raw_host(RawWiiHost *out_host) {
    if (!out_host) {
        return RFVP_WII_INVALID_ARGUMENT;
    }
    out_host->fs_ctx = &g_backend;
    out_host->fs.open = wii_fs_open;
    out_host->fs.close = wii_fs_close;
    out_host->fs.read_at = wii_fs_read_at;
    out_host->fs.len = wii_fs_len;
    out_host->fs.metadata = wii_fs_metadata;
    out_host->fs.write_all = wii_fs_write_all;
    out_host->fs.enumerate_by_extension = wii_fs_enumerate_by_extension;

    out_host->renderer_ctx = &g_backend;
    out_host->renderer.init = wii_renderer_init;
    out_host->renderer.shutdown = wii_renderer_shutdown;
    out_host->renderer.create_texture = wii_renderer_create_texture;
    out_host->renderer.update_texture = wii_renderer_update_texture;
    out_host->renderer.destroy_texture = wii_renderer_destroy_texture;
    out_host->renderer.begin_frame = wii_renderer_begin_frame;
    out_host->renderer.draw_sprite = wii_renderer_draw_sprite;
    out_host->renderer.draw_solid = wii_renderer_draw_solid;
    out_host->renderer.end_frame = wii_renderer_end_frame;
    out_host->renderer.present = wii_renderer_present;

    out_host->audio_ctx = &g_backend;
    out_host->audio.load_native = wii_audio_load;
    out_host->audio.play = wii_audio_play;
    out_host->audio.stop = wii_audio_stop;
    out_host->audio.pause = wii_audio_pause;
    out_host->audio.resume = wii_audio_resume;
    out_host->audio.set_params = wii_audio_set_params;
    out_host->audio.destroy = wii_audio_destroy;
    out_host->audio.tick = wii_audio_tick;

    out_host->clock_ctx = &g_backend;
    out_host->clock.ticks_us = wii_clock_ticks_us;

    out_host->log_ctx = &g_backend;
    out_host->log = wii_log;
    out_host->fatal_ctx = &g_backend;
    out_host->fatal = wii_fatal;
    return RFVP_WII_OK;
}
