#include "psl1ght_backend.h"

#include <dirent.h>
#include <errno.h>
#include <limits.h>
#include <malloc.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <unistd.h>

#include <io/pad.h>
#include <rsx/rsx.h>
#include <sysutil/sysutil.h>
#include <sysutil/video.h>

#define RFVP_PS3_MAX_PATH 768
#define RFVP_PS3_COMMAND_BUFFER_SIZE 0x80000u
#define RFVP_PS3_HOST_BUFFER_SIZE (128u * 1024u * 1024u)
#define RFVP_PS3_FRAME_BUFFER_COUNT 2u
#define RFVP_PS3_BUTTON_A      (1u << 0)
#define RFVP_PS3_BUTTON_B      (1u << 1)
#define RFVP_PS3_BUTTON_X      (1u << 2)
#define RFVP_PS3_BUTTON_Y      (1u << 3)
#define RFVP_PS3_BUTTON_LEFT   (1u << 4)
#define RFVP_PS3_BUTTON_RIGHT  (1u << 5)
#define RFVP_PS3_BUTTON_UP     (1u << 6)
#define RFVP_PS3_BUTTON_DOWN   (1u << 7)
#define RFVP_PS3_BUTTON_PLUS   (1u << 8)
#define RFVP_PS3_BUTTON_MINUS  (1u << 9)

typedef struct PS3Backend {
    int should_exit;
    int video_ready;
    uint64_t start_us;
    char app_root[RFVP_PS3_MAX_PATH];
    gcmContextData *rsx_context;
    void *rsx_host;
    uint32_t display_width;
    uint32_t display_height;
    uint32_t color_pitch;
    uint32_t color_offset[RFVP_PS3_FRAME_BUFFER_COUNT];
    uint32_t *color_buffer[RFVP_PS3_FRAME_BUFFER_COUNT];
    uint32_t current_buffer;
} PS3Backend;

static PS3Backend g_backend;

static uint64_t now_us(void) {
    struct timeval tv;
    if (gettimeofday(&tv, NULL) != 0) {
        return 0;
    }
    return ((uint64_t)tv.tv_sec * 1000000u) + (uint64_t)tv.tv_usec;
}

static void ps3_sysutil_callback(uint64_t status, uint64_t param, void *userdata) {
    (void)param;
    (void)userdata;
    if (status == SYSUTIL_EXIT_GAME) {
        g_backend.should_exit = 1;
    }
}

static int path_from_bytes(const uint8_t *path, size_t path_len, char *out, size_t out_len) {
    if (!path || !out || out_len == 0 || path_len >= out_len) {
        return RFVP_PS3_INVALID_ARGUMENT;
    }
    memcpy(out, path, path_len);
    out[path_len] = '\0';
    return RFVP_PS3_OK;
}

static int starts_with(const char *path, const char *prefix) {
    return strncmp(path, prefix, strlen(prefix)) == 0;
}

static uint32_t pack_x8r8g8b8(const RawRgba8 *pixel) {
    return ((uint32_t)pixel->r << 16) | ((uint32_t)pixel->g << 8) | (uint32_t)pixel->b;
}

static void set_app_root_from_argv(const char *argv0) {
    const char *env_root = getenv("RFVP_PS3_ROOT");
    if (env_root && env_root[0] != '\0' && strlen(env_root) < sizeof(g_backend.app_root)) {
        strcpy(g_backend.app_root, env_root);
        return;
    }

    if (argv0 && argv0[0] == '/') {
        size_t len = strlen(argv0);
        if (len < sizeof(g_backend.app_root)) {
            strcpy(g_backend.app_root, argv0);
            char *slash = strrchr(g_backend.app_root, '/');
            if (slash && slash != g_backend.app_root) {
                *slash = '\0';
                return;
            }
        }
    }

    strcpy(g_backend.app_root, ".");
}

static int resolve_path(const uint8_t *path, size_t path_len, char *out, size_t out_len) {
    char local[RFVP_PS3_MAX_PATH];
    int status = path_from_bytes(path, path_len, local, sizeof(local));
    if (status != RFVP_PS3_OK) {
        return status;
    }

    if (starts_with(local, "app0:/")) {
        const char *rest = local + 6;
        int written = snprintf(out, out_len, "%s/%s", g_backend.app_root, rest);
        return written >= 0 && (size_t)written < out_len ? RFVP_PS3_OK : RFVP_PS3_INVALID_ARGUMENT;
    }

    if (starts_with(local, "/")) {
        if (strlen(local) >= out_len) {
            return RFVP_PS3_INVALID_ARGUMENT;
        }
        strcpy(out, local);
        return RFVP_PS3_OK;
    }

    return RFVP_PS3_INVALID_ARGUMENT;
}

static void wait_flip(void) {
    while (gcmGetFlipStatus() != 0) {
        sysUtilCheckCallback();
        usleep(200);
    }
    gcmResetFlipStatus();
}

static int init_video(void) {
    g_backend.rsx_host = memalign(1024 * 1024, RFVP_PS3_HOST_BUFFER_SIZE);
    if (!g_backend.rsx_host) {
        return RFVP_PS3_OUT_OF_MEMORY;
    }

    if (rsxInit(
            &g_backend.rsx_context,
            RFVP_PS3_COMMAND_BUFFER_SIZE,
            RFVP_PS3_HOST_BUFFER_SIZE,
            g_backend.rsx_host) != 0) {
        return RFVP_PS3_BACKEND;
    }

    videoState state;
    memset(&state, 0, sizeof(state));
    if (videoGetState(0, 0, &state) != 0 || state.state != 0) {
        return RFVP_PS3_BACKEND;
    }

    videoResolution resolution;
    memset(&resolution, 0, sizeof(resolution));
    if (videoGetResolution(state.displayMode.resolution, &resolution) != 0) {
        return RFVP_PS3_BACKEND;
    }

    videoConfiguration config;
    memset(&config, 0, sizeof(config));
    config.resolution = state.displayMode.resolution;
    config.format = VIDEO_BUFFER_COLOR_FORMAT_X8R8G8B8;
    config.pitch = resolution.width * 4;
    config.aspect = state.displayMode.aspect;

    if (videoConfigure(0, &config, NULL, 0) != 0) {
        return RFVP_PS3_BACKEND;
    }
    if (videoGetState(0, 0, &state) != 0) {
        return RFVP_PS3_BACKEND;
    }

    g_backend.display_width = resolution.width;
    g_backend.display_height = resolution.height;
    g_backend.color_pitch = g_backend.display_width * 4;

    gcmSetFlipMode(GCM_FLIP_VSYNC);

    for (uint32_t i = 0; i < RFVP_PS3_FRAME_BUFFER_COUNT; i++) {
        size_t byte_len = (size_t)g_backend.color_pitch * g_backend.display_height;
        g_backend.color_buffer[i] = (uint32_t *)rsxMemalign(64, byte_len);
        if (!g_backend.color_buffer[i]) {
            return RFVP_PS3_OUT_OF_MEMORY;
        }
        memset(g_backend.color_buffer[i], 0, byte_len);
        if (rsxAddressToOffset(g_backend.color_buffer[i], &g_backend.color_offset[i]) != 0) {
            return RFVP_PS3_BACKEND;
        }
        if (gcmSetDisplayBuffer(
                (uint8_t)i,
                g_backend.color_offset[i],
                g_backend.color_pitch,
                g_backend.display_width,
                g_backend.display_height) != 0) {
            return RFVP_PS3_BACKEND;
        }
    }

    gcmResetFlipStatus();
    g_backend.current_buffer = 0;
    g_backend.video_ready = 1;
    return RFVP_PS3_OK;
}

int rfvp_ps3_platform_init(int argc, char **argv) {
    memset(&g_backend, 0, sizeof(g_backend));
    set_app_root_from_argv((argc > 0 && argv) ? argv[0] : NULL);

    if (sysUtilRegisterCallback(SYSUTIL_EVENT_SLOT0, ps3_sysutil_callback, NULL) != 0) {
        return RFVP_PS3_BACKEND;
    }
    if (ioPadInit(7) != 0) {
        return RFVP_PS3_BACKEND;
    }
    int video_status = init_video();
    if (video_status != RFVP_PS3_OK) {
        return video_status;
    }

    g_backend.start_us = now_us();
    return RFVP_PS3_OK;
}

void rfvp_ps3_platform_fini(void) {
    if (g_backend.rsx_context) {
        rsxFinish(g_backend.rsx_context, 1);
    }
    for (uint32_t i = 0; i < RFVP_PS3_FRAME_BUFFER_COUNT; i++) {
        if (g_backend.color_buffer[i]) {
            rsxFree(g_backend.color_buffer[i]);
        }
    }
    if (g_backend.rsx_host) {
        free(g_backend.rsx_host);
    }
    ioPadEnd();
    sysUtilUnregisterCallback(SYSUTIL_EVENT_SLOT0);
}

int rfvp_ps3_platform_should_exit(void) {
    sysUtilCheckCallback();
    return g_backend.should_exit;
}

void rfvp_ps3_platform_sleep_frame(void) {
    sysUtilCheckCallback();
    usleep(16666);
}

void rfvp_ps3_platform_log(uint32_t level, const uint8_t *message, size_t message_len) {
    printf("[rfvp:%u] %.*s\n", level, (int)message_len, message ? (const char *)message : "");
}

void rfvp_ps3_platform_fatal(uint32_t code, const uint8_t *message, size_t message_len) {
    printf("rfvp fatal %u: %.*s\n", code, (int)message_len, message ? (const char *)message : "");
    g_backend.should_exit = 1;
    while (1) {
        sysUtilCheckCallback();
        usleep(100000);
    }
}

uint64_t rfvp_ps3_platform_ticks_us(void) {
    uint64_t current = now_us();
    return current >= g_backend.start_us ? current - g_backend.start_us : 0;
}

int rfvp_ps3_platform_poll_input(RawPS3InputState *out_state) {
    if (!out_state) {
        return RFVP_PS3_INVALID_ARGUMENT;
    }
    memset(out_state, 0, sizeof(*out_state));
    sysUtilCheckCallback();

    padInfo info;
    memset(&info, 0, sizeof(info));
    if (ioPadGetInfo(&info) != 0) {
        return RFVP_PS3_BACKEND;
    }
    if ((info.status[0] & 1u) == 0) {
        return RFVP_PS3_OK;
    }

    padData data;
    memset(&data, 0, sizeof(data));
    if (ioPadGetData(0, &data) != 0 || data.len == 0) {
        return RFVP_PS3_OK;
    }

    uint32_t buttons = 0;
    if (data.BTN_CROSS) buttons |= RFVP_PS3_BUTTON_A;
    if (data.BTN_CIRCLE) buttons |= RFVP_PS3_BUTTON_B;
    if (data.BTN_SQUARE) buttons |= RFVP_PS3_BUTTON_X;
    if (data.BTN_TRIANGLE) buttons |= RFVP_PS3_BUTTON_Y;
    if (data.BTN_LEFT) buttons |= RFVP_PS3_BUTTON_LEFT;
    if (data.BTN_RIGHT) buttons |= RFVP_PS3_BUTTON_RIGHT;
    if (data.BTN_UP) buttons |= RFVP_PS3_BUTTON_UP;
    if (data.BTN_DOWN) buttons |= RFVP_PS3_BUTTON_DOWN;
    if (data.BTN_START) buttons |= RFVP_PS3_BUTTON_PLUS;
    if (data.BTN_SELECT) buttons |= RFVP_PS3_BUTTON_MINUS;
    if (data.BTN_PS) {
        g_backend.should_exit = 1;
    }

    out_state->buttons = buttons;
    out_state->left_stick_x = ((int32_t)data.ANA_L_H - 128) * 256;
    out_state->left_stick_y = (128 - (int32_t)data.ANA_L_V) * 256;
    return RFVP_PS3_OK;
}

int rfvp_ps3_platform_present_rgba8(const RawRgba8 *pixels, uint32_t width, uint32_t height) {
    if (!pixels || width == 0 || height == 0) {
        return RFVP_PS3_INVALID_ARGUMENT;
    }
    if (!g_backend.video_ready || !g_backend.rsx_context) {
        return RFVP_PS3_BACKEND;
    }

    wait_flip();

    uint32_t buffer_index = g_backend.current_buffer;
    uint32_t *dst = g_backend.color_buffer[buffer_index];
    if (!dst) {
        return RFVP_PS3_BACKEND;
    }

    for (uint32_t y = 0; y < g_backend.display_height; y++) {
        uint32_t src_y = (y * height) / g_backend.display_height;
        uint32_t *dst_row = dst + ((size_t)y * g_backend.display_width);
        for (uint32_t x = 0; x < g_backend.display_width; x++) {
            uint32_t src_x = (x * width) / g_backend.display_width;
            dst_row[x] = pack_x8r8g8b8(&pixels[(size_t)src_y * width + src_x]);
        }
    }

    if (gcmSetFlip(g_backend.rsx_context, (uint8_t)buffer_index) != 0) {
        return RFVP_PS3_BACKEND;
    }
    rsxFlushBuffer(g_backend.rsx_context);
    gcmSetWaitFlip(g_backend.rsx_context);

    g_backend.current_buffer = (buffer_index + 1) % RFVP_PS3_FRAME_BUFFER_COUNT;
    return RFVP_PS3_OK;
}

int rfvp_ps3_platform_fs_open(const uint8_t *path, size_t path_len, RawPS3FileHandle *out_handle) {
    if (!out_handle) {
        return RFVP_PS3_INVALID_ARGUMENT;
    }
    char resolved[RFVP_PS3_MAX_PATH];
    int status = resolve_path(path, path_len, resolved, sizeof(resolved));
    if (status != RFVP_PS3_OK) {
        return status;
    }
    FILE *file = fopen(resolved, "rb");
    if (!file) {
        return errno == ENOENT ? RFVP_PS3_NOT_FOUND : RFVP_PS3_IO;
    }
    out_handle->value = (uint64_t)(uintptr_t)file;
    return RFVP_PS3_OK;
}

void rfvp_ps3_platform_fs_close(RawPS3FileHandle handle) {
    if (handle.value != UINT64_MAX) {
        fclose((FILE *)(uintptr_t)handle.value);
    }
}

int rfvp_ps3_platform_fs_read_at(RawPS3FileHandle handle, uint64_t offset, uint8_t *buf, size_t len, size_t *out_read) {
    if (!buf || !out_read || offset > LONG_MAX) {
        return RFVP_PS3_INVALID_ARGUMENT;
    }
    FILE *file = (FILE *)(uintptr_t)handle.value;
    if (!file || fseek(file, (long)offset, SEEK_SET) != 0) {
        return RFVP_PS3_IO;
    }
    size_t n = fread(buf, 1, len, file);
    if (n < len && ferror(file)) {
        return RFVP_PS3_IO;
    }
    *out_read = n;
    return RFVP_PS3_OK;
}

int rfvp_ps3_platform_fs_len(RawPS3FileHandle handle, uint64_t *out_len) {
    if (!out_len) {
        return RFVP_PS3_INVALID_ARGUMENT;
    }
    FILE *file = (FILE *)(uintptr_t)handle.value;
    if (!file) {
        return RFVP_PS3_INVALID_ARGUMENT;
    }
    long cur = ftell(file);
    if (cur < 0 || fseek(file, 0, SEEK_END) != 0) {
        return RFVP_PS3_IO;
    }
    long end = ftell(file);
    if (end < 0 || fseek(file, cur, SEEK_SET) != 0) {
        return RFVP_PS3_IO;
    }
    *out_len = (uint64_t)end;
    return RFVP_PS3_OK;
}

int rfvp_ps3_platform_fs_metadata(const uint8_t *path, size_t path_len, RawPS3FileInfo *out_info) {
    if (!out_info) {
        return RFVP_PS3_INVALID_ARGUMENT;
    }
    char resolved[RFVP_PS3_MAX_PATH];
    int status = resolve_path(path, path_len, resolved, sizeof(resolved));
    if (status != RFVP_PS3_OK) {
        return status;
    }
    struct stat stat_buf;
    memset(&stat_buf, 0, sizeof(stat_buf));
    if (stat(resolved, &stat_buf) != 0) {
        return errno == ENOENT ? RFVP_PS3_NOT_FOUND : RFVP_PS3_IO;
    }
    out_info->len = (uint64_t)stat_buf.st_size;
    out_info->kind = S_ISDIR(stat_buf.st_mode) ? RawPS3FileKind_Directory : RawPS3FileKind_File;
    return RFVP_PS3_OK;
}

int rfvp_ps3_platform_fs_write_all(const uint8_t *path, size_t path_len, const uint8_t *bytes, size_t byte_len) {
    if (!bytes && byte_len != 0) {
        return RFVP_PS3_INVALID_ARGUMENT;
    }
    char resolved[RFVP_PS3_MAX_PATH];
    int status = resolve_path(path, path_len, resolved, sizeof(resolved));
    if (status != RFVP_PS3_OK) {
        return status;
    }
    FILE *file = fopen(resolved, "wb");
    if (!file) {
        return RFVP_PS3_IO;
    }
    if (byte_len != 0 && fwrite(bytes, 1, byte_len, file) != byte_len) {
        fclose(file);
        return RFVP_PS3_IO;
    }
    fclose(file);
    return RFVP_PS3_OK;
}

static int extension_matches(const char *name, const char *extension) {
    const char *dot = strrchr(name, '.');
    return dot && dot[1] != '\0' && strcmp(dot + 1, extension) == 0;
}

static int enumerate_dir_recursive(const char *root, const char *extension, void *visitor_ctx, RawEnumerateVisitorFn visitor) {
    DIR *dir = opendir(root);
    if (!dir) {
        return errno == ENOENT ? RFVP_PS3_NOT_FOUND : RFVP_PS3_IO;
    }
    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
            continue;
        }
        char child[RFVP_PS3_MAX_PATH];
        if (snprintf(child, sizeof(child), "%s/%s", root, entry->d_name) >= (int)sizeof(child)) {
            closedir(dir);
            return RFVP_PS3_INVALID_ARGUMENT;
        }
        struct stat stat_buf;
        memset(&stat_buf, 0, sizeof(stat_buf));
        if (stat(child, &stat_buf) != 0) {
            closedir(dir);
            return RFVP_PS3_IO;
        }
        if (S_ISDIR(stat_buf.st_mode)) {
            int status = enumerate_dir_recursive(child, extension, visitor_ctx, visitor);
            if (status != RFVP_PS3_OK) {
                closedir(dir);
                return status;
            }
        } else if (extension_matches(child, extension)) {
            RawPS3FileInfo info;
            info.len = (uint64_t)stat_buf.st_size;
            info.kind = RawPS3FileKind_File;
            int status = visitor(visitor_ctx, (const uint8_t *)child, strlen(child), info);
            if (status != RFVP_PS3_OK) {
                closedir(dir);
                return status;
            }
        }
    }
    closedir(dir);
    return RFVP_PS3_OK;
}

int rfvp_ps3_platform_fs_enumerate_by_extension(
    const uint8_t *root,
    size_t root_len,
    const uint8_t *extension,
    size_t extension_len,
    void *visitor_ctx,
    RawEnumerateVisitorFn visitor) {
    if (!visitor) {
        return RFVP_PS3_INVALID_ARGUMENT;
    }
    char resolved[RFVP_PS3_MAX_PATH];
    char ext[32];
    int status = resolve_path(root, root_len, resolved, sizeof(resolved));
    if (status != RFVP_PS3_OK) {
        return status;
    }
    status = path_from_bytes(extension, extension_len, ext, sizeof(ext));
    if (status != RFVP_PS3_OK) {
        return status;
    }
    return enumerate_dir_recursive(resolved, ext, visitor_ctx, visitor);
}
