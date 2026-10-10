#include "wut_backend.h"

#include <coreinit/cache.h>
#include <coreinit/screen.h>
#include <coreinit/thread.h>
#include <coreinit/time.h>
#include <dirent.h>
#include <limits.h>
#include <malloc.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>
#include <vpad/input.h>
#include <whb/log.h>
#include <whb/log_cafe.h>
#include <whb/log_console.h>
#include <whb/proc.h>
#include <whb/sdcard.h>

#define RFVP_WIIU_MAX_PATH 768
#define RFVP_WIIU_BUTTON_A      (1u << 0)
#define RFVP_WIIU_BUTTON_B      (1u << 1)
#define RFVP_WIIU_BUTTON_X      (1u << 2)
#define RFVP_WIIU_BUTTON_Y      (1u << 3)
#define RFVP_WIIU_BUTTON_LEFT   (1u << 4)
#define RFVP_WIIU_BUTTON_RIGHT  (1u << 5)
#define RFVP_WIIU_BUTTON_UP     (1u << 6)
#define RFVP_WIIU_BUTTON_DOWN   (1u << 7)
#define RFVP_WIIU_BUTTON_PLUS   (1u << 8)
#define RFVP_WIIU_BUTTON_MINUS  (1u << 9)

typedef struct WiiUBackend {
    int should_exit;
    int sd_mounted;
    char sd_root[RFVP_WIIU_MAX_PATH];
    OSTime start_time;
    void *tv_buffer;
    void *drc_buffer;
    size_t tv_buffer_size;
    size_t drc_buffer_size;
} WiiUBackend;

static WiiUBackend g_backend;

static int path_from_bytes(const uint8_t *path, size_t path_len, char *out, size_t out_len) {
    if (!path || !out || out_len == 0 || path_len >= out_len) {
        return RFVP_WIIU_INVALID_ARGUMENT;
    }
    memcpy(out, path, path_len);
    out[path_len] = '\0';
    return RFVP_WIIU_OK;
}

static int starts_with(const char *path, const char *prefix) {
    return strncmp(path, prefix, strlen(prefix)) == 0;
}

static int resolve_path(const uint8_t *path, size_t path_len, char *out, size_t out_len) {
    char local[RFVP_WIIU_MAX_PATH];
    int status = path_from_bytes(path, path_len, local, sizeof(local));
    if (status != RFVP_WIIU_OK) {
        return status;
    }

    if (starts_with(local, "sd:/")) {
        const char *rest = local + 4;
        int written = snprintf(out, out_len, "%s/%s", g_backend.sd_root, rest);
        return written >= 0 && (size_t)written < out_len ? RFVP_WIIU_OK : RFVP_WIIU_INVALID_ARGUMENT;
    }

    if (starts_with(local, "/")) {
        if (strlen(local) >= out_len) {
            return RFVP_WIIU_INVALID_ARGUMENT;
        }
        strcpy(out, local);
        return RFVP_WIIU_OK;
    }

    int written = snprintf(out, out_len, "%s/%s", g_backend.sd_root, local);
    return written >= 0 && (size_t)written < out_len ? RFVP_WIIU_OK : RFVP_WIIU_INVALID_ARGUMENT;
}

static uint32_t pack_rgba(const RawRgba8 *pixel) {
    return ((uint32_t)pixel->r << 24) | ((uint32_t)pixel->g << 16) | ((uint32_t)pixel->b << 8) | pixel->a;
}

int rfvp_wiiu_platform_init(int argc, char **argv) {
    (void)argc;
    (void)argv;
    memset(&g_backend, 0, sizeof(g_backend));

    WHBProcInit();
    WHBLogCafeInit();
    WHBLogConsoleInit();
    VPADInit();

    if (!WHBMountSdCard()) {
        return RFVP_WIIU_IO;
    }
    g_backend.sd_mounted = 1;
    const char *sd_path = WHBGetSdCardMountPath();
    if (!sd_path || strlen(sd_path) >= sizeof(g_backend.sd_root)) {
        return RFVP_WIIU_BACKEND;
    }
    strcpy(g_backend.sd_root, sd_path);

    OSScreenInit();
    g_backend.tv_buffer_size = OSScreenGetBufferSizeEx(SCREEN_TV);
    g_backend.drc_buffer_size = OSScreenGetBufferSizeEx(SCREEN_DRC);
    g_backend.tv_buffer = memalign(0x100, g_backend.tv_buffer_size);
    g_backend.drc_buffer = memalign(0x100, g_backend.drc_buffer_size);
    if (!g_backend.tv_buffer || !g_backend.drc_buffer) {
        return RFVP_WIIU_OUT_OF_MEMORY;
    }
    OSScreenSetBufferEx(SCREEN_TV, g_backend.tv_buffer);
    OSScreenSetBufferEx(SCREEN_DRC, g_backend.drc_buffer);
    OSScreenEnableEx(SCREEN_TV, TRUE);
    OSScreenEnableEx(SCREEN_DRC, TRUE);
    OSScreenClearBufferEx(SCREEN_TV, 0);
    OSScreenClearBufferEx(SCREEN_DRC, 0);
    OSScreenFlipBuffersEx(SCREEN_TV);
    OSScreenFlipBuffersEx(SCREEN_DRC);

    g_backend.start_time = OSGetTime();
    return RFVP_WIIU_OK;
}

void rfvp_wiiu_platform_fini(void) {
    if (g_backend.tv_buffer) {
        free(g_backend.tv_buffer);
    }
    if (g_backend.drc_buffer) {
        free(g_backend.drc_buffer);
    }
    OSScreenShutdown();
    if (g_backend.sd_mounted) {
        WHBUnmountSdCard();
    }
    WHBLogConsoleFree();
    WHBLogCafeDeinit();
    WHBProcShutdown();
}

int rfvp_wiiu_platform_should_exit(void) {
    return g_backend.should_exit || !WHBProcIsRunning();
}

void rfvp_wiiu_platform_sleep_frame(void) {
    OSSleepTicks(OSMillisecondsToTicks(16));
}

void rfvp_wiiu_platform_log(uint32_t level, const uint8_t *message, size_t message_len) {
    WHBLogPrintf("[rfvp:%u] %.*s", level, (int)message_len, message ? (const char *)message : "");
}

void rfvp_wiiu_platform_fatal(uint32_t code, const uint8_t *message, size_t message_len) {
    WHBLogPrintf("rfvp fatal %u: %.*s", code, (int)message_len, message ? (const char *)message : "");
    WHBProcStopRunning();
    while (1) {
        OSSleepTicks(OSMillisecondsToTicks(100));
    }
}

uint64_t rfvp_wiiu_platform_ticks_us(void) {
    return OSTicksToMicroseconds(OSGetTime() - g_backend.start_time);
}

int rfvp_wiiu_platform_poll_input(RawWiiUInputState *out_state) {
    if (!out_state) {
        return RFVP_WIIU_INVALID_ARGUMENT;
    }
    memset(out_state, 0, sizeof(*out_state));

    VPADStatus status;
    memset(&status, 0, sizeof(status));
    VPADReadError error = VPAD_READ_SUCCESS;
    int read = VPADRead(VPAD_CHAN_0, &status, 1, &error);
    if (read <= 0) {
        return error == VPAD_READ_NO_SAMPLES ? RFVP_WIIU_OK : RFVP_WIIU_BACKEND;
    }

    uint32_t buttons = 0;
    if (status.hold & VPAD_BUTTON_A) buttons |= RFVP_WIIU_BUTTON_A;
    if (status.hold & VPAD_BUTTON_B) buttons |= RFVP_WIIU_BUTTON_B;
    if (status.hold & VPAD_BUTTON_X) buttons |= RFVP_WIIU_BUTTON_X;
    if (status.hold & VPAD_BUTTON_Y) buttons |= RFVP_WIIU_BUTTON_Y;
    if (status.hold & VPAD_BUTTON_LEFT) buttons |= RFVP_WIIU_BUTTON_LEFT;
    if (status.hold & VPAD_BUTTON_RIGHT) buttons |= RFVP_WIIU_BUTTON_RIGHT;
    if (status.hold & VPAD_BUTTON_UP) buttons |= RFVP_WIIU_BUTTON_UP;
    if (status.hold & VPAD_BUTTON_DOWN) buttons |= RFVP_WIIU_BUTTON_DOWN;
    if (status.hold & VPAD_BUTTON_PLUS) buttons |= RFVP_WIIU_BUTTON_PLUS;
    if (status.hold & VPAD_BUTTON_MINUS) buttons |= RFVP_WIIU_BUTTON_MINUS;
    if (status.hold & VPAD_BUTTON_HOME) {
        g_backend.should_exit = 1;
    }

    out_state->buttons = buttons;
    out_state->left_stick_x = (int32_t)(status.leftStick.x * 32767.0f);
    out_state->left_stick_y = (int32_t)(status.leftStick.y * 32767.0f);
    return RFVP_WIIU_OK;
}

int rfvp_wiiu_platform_present_rgba8(const RawRgba8 *pixels, uint32_t width, uint32_t height) {
    if (!pixels || width == 0 || height == 0) {
        return RFVP_WIIU_INVALID_ARGUMENT;
    }
    const uint32_t tv_w = 1280;
    const uint32_t tv_h = 720;
    const uint32_t drc_w = 854;
    const uint32_t drc_h = 480;

    for (uint32_t y = 0; y < tv_h; y++) {
        uint32_t src_y = (y * height) / tv_h;
        for (uint32_t x = 0; x < tv_w; x++) {
            uint32_t src_x = (x * width) / tv_w;
            OSScreenPutPixelEx(SCREEN_TV, x, y, pack_rgba(&pixels[src_y * width + src_x]));
        }
    }

    for (uint32_t y = 0; y < drc_h; y++) {
        uint32_t src_y = (y * height) / drc_h;
        for (uint32_t x = 0; x < drc_w; x++) {
            uint32_t src_x = (x * width) / drc_w;
            OSScreenPutPixelEx(SCREEN_DRC, x, y, pack_rgba(&pixels[src_y * width + src_x]));
        }
    }

    DCFlushRange(g_backend.tv_buffer, g_backend.tv_buffer_size);
    DCFlushRange(g_backend.drc_buffer, g_backend.drc_buffer_size);
    OSScreenFlipBuffersEx(SCREEN_TV);
    OSScreenFlipBuffersEx(SCREEN_DRC);
    return RFVP_WIIU_OK;
}

int rfvp_wiiu_platform_fs_open(const uint8_t *path, size_t path_len, RawWiiUFileHandle *out_handle) {
    if (!out_handle) {
        return RFVP_WIIU_INVALID_ARGUMENT;
    }
    char resolved[RFVP_WIIU_MAX_PATH];
    int status = resolve_path(path, path_len, resolved, sizeof(resolved));
    if (status != RFVP_WIIU_OK) {
        return status;
    }
    FILE *file = fopen(resolved, "rb");
    if (!file) {
        return RFVP_WIIU_NOT_FOUND;
    }
    out_handle->value = (uint64_t)(uintptr_t)file;
    return RFVP_WIIU_OK;
}

void rfvp_wiiu_platform_fs_close(RawWiiUFileHandle handle) {
    if (handle.value != UINT64_MAX) {
        fclose((FILE *)(uintptr_t)handle.value);
    }
}

int rfvp_wiiu_platform_fs_read_at(RawWiiUFileHandle handle, uint64_t offset, uint8_t *buf, size_t len, size_t *out_read) {
    if (!buf || !out_read || offset > LONG_MAX) {
        return RFVP_WIIU_INVALID_ARGUMENT;
    }
    FILE *file = (FILE *)(uintptr_t)handle.value;
    if (!file || fseek(file, (long)offset, SEEK_SET) != 0) {
        return RFVP_WIIU_IO;
    }
    size_t n = fread(buf, 1, len, file);
    if (n < len && ferror(file)) {
        return RFVP_WIIU_IO;
    }
    *out_read = n;
    return RFVP_WIIU_OK;
}

int rfvp_wiiu_platform_fs_len(RawWiiUFileHandle handle, uint64_t *out_len) {
    if (!out_len) {
        return RFVP_WIIU_INVALID_ARGUMENT;
    }
    FILE *file = (FILE *)(uintptr_t)handle.value;
    if (!file) {
        return RFVP_WIIU_INVALID_ARGUMENT;
    }
    long cur = ftell(file);
    if (cur < 0 || fseek(file, 0, SEEK_END) != 0) {
        return RFVP_WIIU_IO;
    }
    long end = ftell(file);
    if (end < 0 || fseek(file, cur, SEEK_SET) != 0) {
        return RFVP_WIIU_IO;
    }
    *out_len = (uint64_t)end;
    return RFVP_WIIU_OK;
}

int rfvp_wiiu_platform_fs_metadata(const uint8_t *path, size_t path_len, RawWiiUFileInfo *out_info) {
    if (!out_info) {
        return RFVP_WIIU_INVALID_ARGUMENT;
    }
    char resolved[RFVP_WIIU_MAX_PATH];
    int status = resolve_path(path, path_len, resolved, sizeof(resolved));
    if (status != RFVP_WIIU_OK) {
        return status;
    }
    struct stat stat_buf;
    memset(&stat_buf, 0, sizeof(stat_buf));
    if (stat(resolved, &stat_buf) != 0) {
        return RFVP_WIIU_NOT_FOUND;
    }
    out_info->len = (uint64_t)stat_buf.st_size;
    out_info->kind = S_ISDIR(stat_buf.st_mode) ? RawWiiUFileKind_Directory : RawWiiUFileKind_File;
    return RFVP_WIIU_OK;
}

int rfvp_wiiu_platform_fs_write_all(const uint8_t *path, size_t path_len, const uint8_t *bytes, size_t byte_len) {
    if (!bytes && byte_len != 0) {
        return RFVP_WIIU_INVALID_ARGUMENT;
    }
    char resolved[RFVP_WIIU_MAX_PATH];
    int status = resolve_path(path, path_len, resolved, sizeof(resolved));
    if (status != RFVP_WIIU_OK) {
        return status;
    }
    FILE *file = fopen(resolved, "wb");
    if (!file) {
        return RFVP_WIIU_IO;
    }
    if (byte_len != 0 && fwrite(bytes, 1, byte_len, file) != byte_len) {
        fclose(file);
        return RFVP_WIIU_IO;
    }
    fclose(file);
    return RFVP_WIIU_OK;
}

static int extension_matches(const char *name, const char *extension) {
    const char *dot = strrchr(name, '.');
    return dot && dot[1] != '\0' && strcmp(dot + 1, extension) == 0;
}

static int enumerate_dir_recursive(const char *root, const char *extension, void *visitor_ctx, RawEnumerateVisitorFn visitor) {
    DIR *dir = opendir(root);
    if (!dir) {
        return RFVP_WIIU_NOT_FOUND;
    }
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
            continue;
        }
        char child[RFVP_WIIU_MAX_PATH];
        if (snprintf(child, sizeof(child), "%s/%s", root, entry->d_name) >= (int)sizeof(child)) {
            closedir(dir);
            return RFVP_WIIU_INVALID_ARGUMENT;
        }
        struct stat stat_buf;
        memset(&stat_buf, 0, sizeof(stat_buf));
        if (stat(child, &stat_buf) != 0) {
            closedir(dir);
            return RFVP_WIIU_IO;
        }
        if (S_ISDIR(stat_buf.st_mode)) {
            int status = enumerate_dir_recursive(child, extension, visitor_ctx, visitor);
            if (status != RFVP_WIIU_OK) {
                closedir(dir);
                return status;
            }
        } else if (extension_matches(child, extension)) {
            RawWiiUFileInfo info;
            info.len = (uint64_t)stat_buf.st_size;
            info.kind = RawWiiUFileKind_File;
            int status = visitor(visitor_ctx, (const uint8_t *)child, strlen(child), info);
            if (status != RFVP_WIIU_OK) {
                closedir(dir);
                return status;
            }
        }
    }
    closedir(dir);
    return RFVP_WIIU_OK;
}

int rfvp_wiiu_platform_fs_enumerate_by_extension(
    const uint8_t *root,
    size_t root_len,
    const uint8_t *extension,
    size_t extension_len,
    void *visitor_ctx,
    RawEnumerateVisitorFn visitor) {
    if (!visitor) {
        return RFVP_WIIU_INVALID_ARGUMENT;
    }
    char resolved[RFVP_WIIU_MAX_PATH];
    char ext[32];
    int status = resolve_path(root, root_len, resolved, sizeof(resolved));
    if (status != RFVP_WIIU_OK) {
        return status;
    }
    status = path_from_bytes(extension, extension_len, ext, sizeof(ext));
    if (status != RFVP_WIIU_OK) {
        return status;
    }
    return enumerate_dir_recursive(resolved, ext, visitor_ctx, visitor);
}
