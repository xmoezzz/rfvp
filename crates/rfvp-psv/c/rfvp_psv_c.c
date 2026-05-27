#include "rfvp_psv_c.h"

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#if defined(RFVP_PSV_VITASDK_BACKEND)
#  include <psp2/io/fcntl.h>
#  include <psp2/io/dirent.h>
#  include <psp2/io/stat.h>
#  include <psp2/kernel/processmgr.h>
#  include <psp2/audioout.h>
#  include <psp2/message_dialog.h>
#endif

#if defined(__has_include)
#  if __has_include(<dirent.h>) && __has_include(<sys/stat.h>)
#    define RFVP_PSV_HAS_DIRENT 1
#    include <dirent.h>
#    include <sys/stat.h>
#  endif
#endif

#ifndef RFVP_PSV_HAS_DIRENT
#  define RFVP_PSV_HAS_DIRENT 0
#endif

#define RFVP_PSV_OK 0
#define RFVP_PSV_IO (-1)
#define RFVP_PSV_NOT_FOUND (-2)
#define RFVP_PSV_INVALID_DATA (-3)
#define RFVP_PSV_INVALID_ARGUMENT (-4)
#define RFVP_PSV_UNSUPPORTED (-5)
#define RFVP_PSV_OUT_OF_MEMORY (-6)
#define RFVP_PSV_CAPACITY_EXCEEDED (-7)
#define RFVP_PSV_END_OF_FILE (-8)
#define RFVP_PSV_BACKEND (-9)

#define RFVP_PSV_MAX_PATH 1024u
#define RFVP_PSV_MAX_TEXTURES 4096u
#define RFVP_PSV_ALLOC_MAGIC 0x52465650484F5241ull
#define RFVP_PSV_MAX_AUDIO_STREAMS 16u
#define RFVP_PSV_AUDIO_OUTPUT_RATE 48000u
#define RFVP_PSV_AUDIO_OUTPUT_CHANNELS 2u
#define RFVP_PSV_AUDIO_PORT_LEN 1024u

typedef struct RfvpPsvAllocHeader {
    void *base;
    size_t size;
    size_t align;
    uint64_t magic;
} RfvpPsvAllocHeader;

typedef struct RfvpPsvTexture {
    uint8_t used;
    uint32_t width;
    uint32_t height;
    uint8_t *rgba8;
} RfvpPsvTexture;

typedef struct RfvpPsvRendererState {
    uint32_t width;
    uint32_t height;
    uint32_t stride_bytes;
    uint8_t *backbuffer;
    uint8_t *external_framebuffer;
    uint32_t external_width;
    uint32_t external_height;
    uint32_t external_stride_bytes;
    RfvpPsvPresentCallback present_callback;
    void *present_callback_ctx;
    RfvpPsvTexture textures[RFVP_PSV_MAX_TEXTURES];
} RfvpPsvRendererState;

typedef struct RfvpPsvFileSystemState {
    char root[RFVP_PSV_MAX_PATH];
} RfvpPsvFileSystemState;

typedef struct RfvpPsvClockState {
    uint64_t manual_ticks_us;
    uint8_t use_manual_ticks;
} RfvpPsvClockState;

typedef struct RfvpPsvAudioStream {
    uint8_t used;
    uint8_t playing;
    uint8_t stopping;
    uint8_t reserved;
    uint32_t sample_rate;
    uint16_t channels;
    RawAudioSampleFormat format;
    RawAudioParams params;
    int16_t *samples;
    size_t sample_count;
    uint64_t phase;
    uint64_t phase_step;
    uint64_t fade_total_us;
    uint64_t fade_remaining_us;
    float fade_start_volume;
} RfvpPsvAudioStream;

typedef struct RfvpPsvAudioState {
    int port;
    int port_open;
    int16_t mix_buffer[RFVP_PSV_AUDIO_PORT_LEN * RFVP_PSV_AUDIO_OUTPUT_CHANNELS];
    RfvpPsvAudioStream streams[RFVP_PSV_MAX_AUDIO_STREAMS];
} RfvpPsvAudioState;

typedef struct RfvpPsvGlobalState {
    RfvpPsvFileSystemState fs;
    RfvpPsvRendererState renderer;
    RfvpPsvClockState clock;
    RfvpPsvAudioState audio;
    uint8_t exit_requested;
} RfvpPsvGlobalState;

static RfvpPsvGlobalState g_rfvp_psv = {
    .fs = { .root = "." },
    .audio = { .port = -1, .port_open = 0 },
};

static uintptr_t rfvp_align_up_uintptr(uintptr_t value, size_t align) {
    uintptr_t mask = (uintptr_t)align - 1u;
    return (value + mask) & ~mask;
}

void *rfvp_psv_alloc(size_t size, size_t align) {
    if (align < sizeof(void *)) {
        align = sizeof(void *);
    }
    if ((align & (align - 1u)) != 0u) {
        return NULL;
    }

    size_t total = size + align - 1u + sizeof(RfvpPsvAllocHeader);
    void *base = malloc(total);
    if (base == NULL) {
        return NULL;
    }

    uintptr_t start = (uintptr_t)base + sizeof(RfvpPsvAllocHeader);
    uintptr_t aligned = rfvp_align_up_uintptr(start, align);
    RfvpPsvAllocHeader *header = ((RfvpPsvAllocHeader *)aligned) - 1;
    header->base = base;
    header->size = size;
    header->align = align;
    header->magic = RFVP_PSV_ALLOC_MAGIC;
    return (void *)aligned;
}

void rfvp_psv_dealloc(void *ptr, size_t size, size_t align) {
    (void)size;
    (void)align;
    if (ptr == NULL) {
        return;
    }
    RfvpPsvAllocHeader *header = ((RfvpPsvAllocHeader *)ptr) - 1;
    if (header->magic != RFVP_PSV_ALLOC_MAGIC) {
        return;
    }
    void *base = header->base;
    header->magic = 0u;
    free(base);
}

static int32_t rfvp_copy_path(const uint8_t *path, size_t path_len, char *out, size_t out_len) {
    if (path == NULL || out == NULL || out_len == 0u || path_len >= out_len) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    for (size_t i = 0; i < path_len; ++i) {
        if (path[i] == 0u) {
            return RFVP_PSV_INVALID_ARGUMENT;
        }
    }
    memcpy(out, path, path_len);
    out[path_len] = '\0';
    return RFVP_PSV_OK;
}

static int32_t rfvp_build_full_path(const uint8_t *path, size_t path_len, char *out, size_t out_len) {
    char local[RFVP_PSV_MAX_PATH];
    int32_t status = rfvp_copy_path(path, path_len, local, sizeof(local));
    if (status != RFVP_PSV_OK) {
        return status;
    }

    if (local[0] == '/' || strstr(local, ":/") != NULL || strstr(local, ":\\") != NULL) {
        size_t len = strlen(local);
        if (len >= out_len) {
            return RFVP_PSV_CAPACITY_EXCEEDED;
        }
        memcpy(out, local, len + 1u);
        return RFVP_PSV_OK;
    }

    const char *root = g_rfvp_psv.fs.root[0] != '\0' ? g_rfvp_psv.fs.root : ".";
    size_t root_len = strlen(root);
    size_t local_len = strlen(local);
    uint8_t need_sep = (root_len > 0u && root[root_len - 1u] != '/' && root[root_len - 1u] != '\\') ? 1u : 0u;
    if (root_len + (size_t)need_sep + local_len >= out_len) {
        return RFVP_PSV_CAPACITY_EXCEEDED;
    }

    memcpy(out, root, root_len);
    size_t pos = root_len;
    if (need_sep) {
        out[pos++] = '/';
    }
    memcpy(out + pos, local, local_len + 1u);
    return RFVP_PSV_OK;
}

int32_t rfvp_psv_c_set_asset_root(const char *root) {
    if (root == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    size_t len = strlen(root);
    if (len >= sizeof(g_rfvp_psv.fs.root)) {
        return RFVP_PSV_CAPACITY_EXCEEDED;
    }
    memcpy(g_rfvp_psv.fs.root, root, len + 1u);
    return RFVP_PSV_OK;
}

#if defined(RFVP_PSV_VITASDK_BACKEND)
static int32_t rfvp_c_open(void *ctx, const uint8_t *path, size_t path_len, RawFileHandle *out_handle) {
    (void)ctx;
    if (out_handle == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    char full_path[RFVP_PSV_MAX_PATH];
    int32_t status = rfvp_build_full_path(path, path_len, full_path, sizeof(full_path));
    if (status != RFVP_PSV_OK) {
        return status;
    }

    SceUID fd = sceIoOpen(full_path, SCE_O_RDONLY, 0);
    if (fd < 0) {
        return RFVP_PSV_NOT_FOUND;
    }
    out_handle->value = (uint64_t)(uint32_t)fd;
    return RFVP_PSV_OK;
}

static void rfvp_c_close(void *ctx, RawFileHandle handle) {
    (void)ctx;
    if (handle.value != UINT64_MAX) {
        sceIoClose((SceUID)(uint32_t)handle.value);
    }
}

static int32_t rfvp_c_read_at(void *ctx, RawFileHandle handle, uint64_t offset, uint8_t *buf, size_t len, size_t *out_read) {
    (void)ctx;
    if (handle.value == UINT64_MAX || buf == NULL || out_read == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    int read_count = sceIoPread((SceUID)(uint32_t)handle.value, buf, len, (SceOff)offset);
    if (read_count < 0) {
        return RFVP_PSV_IO;
    }
    *out_read = (size_t)read_count;
    return RFVP_PSV_OK;
}

static int32_t rfvp_c_len(void *ctx, RawFileHandle handle, uint64_t *out_len) {
    (void)ctx;
    if (handle.value == UINT64_MAX || out_len == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    SceUID fd = (SceUID)(uint32_t)handle.value;
    SceOff old_pos = sceIoLseek(fd, 0, SCE_SEEK_CUR);
    if (old_pos < 0) {
        return RFVP_PSV_IO;
    }
    SceOff end_pos = sceIoLseek(fd, 0, SCE_SEEK_END);
    if (end_pos < 0) {
        return RFVP_PSV_IO;
    }
    if (sceIoLseek(fd, old_pos, SCE_SEEK_SET) < 0) {
        return RFVP_PSV_IO;
    }
    *out_len = (uint64_t)end_pos;
    return RFVP_PSV_OK;
}

static int32_t rfvp_c_metadata(void *ctx, const uint8_t *path, size_t path_len, RawFileInfo *out_info) {
    (void)ctx;
    if (out_info == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    char full_path[RFVP_PSV_MAX_PATH];
    int32_t status = rfvp_build_full_path(path, path_len, full_path, sizeof(full_path));
    if (status != RFVP_PSV_OK) {
        return status;
    }

    SceIoStat st;
    memset(&st, 0, sizeof(st));
    int rc = sceIoGetstat(full_path, &st);
    if (rc < 0) {
        return RFVP_PSV_NOT_FOUND;
    }
    out_info->len = (uint64_t)st.st_size;
    if (SCE_S_ISDIR(st.st_mode)) {
        out_info->kind = RAW_FILE_KIND_DIRECTORY;
    } else {
        out_info->kind = RAW_FILE_KIND_FILE;
    }
    return RFVP_PSV_OK;
}

static int32_t rfvp_c_write_all(void *ctx, const uint8_t *path, size_t path_len, const uint8_t *bytes, size_t byte_len) {
    (void)ctx;
    if (bytes == NULL && byte_len != 0u) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    char full_path[RFVP_PSV_MAX_PATH];
    int32_t status = rfvp_build_full_path(path, path_len, full_path, sizeof(full_path));
    if (status != RFVP_PSV_OK) {
        return status;
    }

    SceUID fd = sceIoOpen(full_path, SCE_O_WRONLY | SCE_O_CREAT | SCE_O_TRUNC, 0666);
    if (fd < 0) {
        return RFVP_PSV_IO;
    }
    size_t written = 0u;
    while (written < byte_len) {
        int rc = sceIoWrite(fd, bytes + written, byte_len - written);
        if (rc < 0) {
            sceIoClose(fd);
            return RFVP_PSV_IO;
        }
        if (rc == 0) {
            sceIoClose(fd);
            return RFVP_PSV_END_OF_FILE;
        }
        written += (size_t)rc;
    }
    sceIoClose(fd);
    return RFVP_PSV_OK;
}
#else
static int32_t rfvp_c_open(void *ctx, const uint8_t *path, size_t path_len, RawFileHandle *out_handle) {
    (void)ctx;
    if (out_handle == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    char full_path[RFVP_PSV_MAX_PATH];
    int32_t status = rfvp_build_full_path(path, path_len, full_path, sizeof(full_path));
    if (status != RFVP_PSV_OK) {
        return status;
    }

    FILE *file = fopen(full_path, "rb");
    if (file == NULL) {
        return errno == ENOENT ? RFVP_PSV_NOT_FOUND : RFVP_PSV_IO;
    }
    out_handle->value = (uint64_t)(uintptr_t)file;
    return RFVP_PSV_OK;
}

static void rfvp_c_close(void *ctx, RawFileHandle handle) {
    (void)ctx;
    FILE *file = (FILE *)(uintptr_t)handle.value;
    if (file != NULL && handle.value != UINT64_MAX) {
        fclose(file);
    }
}

static int32_t rfvp_c_read_at(void *ctx, RawFileHandle handle, uint64_t offset, uint8_t *buf, size_t len, size_t *out_read) {
    (void)ctx;
    FILE *file = (FILE *)(uintptr_t)handle.value;
    if (file == NULL || handle.value == UINT64_MAX || buf == NULL || out_read == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    if (fseek(file, (long)offset, SEEK_SET) != 0) {
        return RFVP_PSV_IO;
    }
    size_t read_count = fread(buf, 1u, len, file);
    if (read_count < len && ferror(file)) {
        return RFVP_PSV_IO;
    }
    *out_read = read_count;
    return RFVP_PSV_OK;
}

static int32_t rfvp_c_len(void *ctx, RawFileHandle handle, uint64_t *out_len) {
    (void)ctx;
    FILE *file = (FILE *)(uintptr_t)handle.value;
    if (file == NULL || handle.value == UINT64_MAX || out_len == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    long old_pos = ftell(file);
    if (old_pos < 0) {
        return RFVP_PSV_IO;
    }
    if (fseek(file, 0L, SEEK_END) != 0) {
        return RFVP_PSV_IO;
    }
    long end_pos = ftell(file);
    if (end_pos < 0) {
        return RFVP_PSV_IO;
    }
    if (fseek(file, old_pos, SEEK_SET) != 0) {
        return RFVP_PSV_IO;
    }
    *out_len = (uint64_t)end_pos;
    return RFVP_PSV_OK;
}

static int32_t rfvp_c_metadata(void *ctx, const uint8_t *path, size_t path_len, RawFileInfo *out_info) {
    (void)ctx;
    if (out_info == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    char full_path[RFVP_PSV_MAX_PATH];
    int32_t status = rfvp_build_full_path(path, path_len, full_path, sizeof(full_path));
    if (status != RFVP_PSV_OK) {
        return status;
    }

#if RFVP_PSV_HAS_DIRENT
    struct stat st;
    if (stat(full_path, &st) == 0) {
        out_info->len = (uint64_t)st.st_size;
        if (S_ISREG(st.st_mode)) {
            out_info->kind = RAW_FILE_KIND_FILE;
        } else if (S_ISDIR(st.st_mode)) {
            out_info->kind = RAW_FILE_KIND_DIRECTORY;
        } else {
            out_info->kind = RAW_FILE_KIND_OTHER;
        }
        return RFVP_PSV_OK;
    }
#endif

    FILE *file = fopen(full_path, "rb");
    if (file == NULL) {
        return errno == ENOENT ? RFVP_PSV_NOT_FOUND : RFVP_PSV_IO;
    }
    if (fseek(file, 0L, SEEK_END) != 0) {
        fclose(file);
        return RFVP_PSV_IO;
    }
    long end_pos = ftell(file);
    fclose(file);
    if (end_pos < 0) {
        return RFVP_PSV_IO;
    }
    out_info->len = (uint64_t)end_pos;
    out_info->kind = RAW_FILE_KIND_FILE;
    return RFVP_PSV_OK;
}

static int32_t rfvp_c_write_all(void *ctx, const uint8_t *path, size_t path_len, const uint8_t *bytes, size_t byte_len) {
    (void)ctx;
    if (bytes == NULL && byte_len != 0u) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    char full_path[RFVP_PSV_MAX_PATH];
    int32_t status = rfvp_build_full_path(path, path_len, full_path, sizeof(full_path));
    if (status != RFVP_PSV_OK) {
        return status;
    }

    FILE *file = fopen(full_path, "wb");
    if (file == NULL) {
        return RFVP_PSV_IO;
    }
    if (byte_len != 0u && fwrite(bytes, 1u, byte_len, file) != byte_len) {
        fclose(file);
        return RFVP_PSV_IO;
    }
    if (fclose(file) != 0) {
        return RFVP_PSV_IO;
    }
    return RFVP_PSV_OK;
}
#endif

static int rfvp_path_has_extension(const char *name, const char *extension) {
    const char *dot = strrchr(name, '.');
    if (dot == NULL) {
        return 0;
    }
    ++dot;
    while (*dot != '\0' && *extension != '\0') {
        char a = *dot++;
        char b = *extension++;
        if (a >= 'A' && a <= 'Z') {
            a = (char)(a - 'A' + 'a');
        }
        if (b >= 'A' && b <= 'Z') {
            b = (char)(b - 'A' + 'a');
        }
        if (a != b) {
            return 0;
        }
    }
    return *dot == '\0' && *extension == '\0';
}

#if defined(RFVP_PSV_VITASDK_BACKEND)
static int32_t rfvp_enumerate_dir_recursive(const char *full_dir, const char *relative_dir, const char *extension, void *visitor_ctx, RawEnumerateByExtensionVisitorFn visitor) {
    SceUID dir = sceIoDopen(full_dir);
    if (dir < 0) {
        return RFVP_PSV_NOT_FOUND;
    }

    for (;;) {
        SceIoDirent entry;
        memset(&entry, 0, sizeof(entry));
        int rc = sceIoDread(dir, &entry);
        if (rc == 0) {
            break;
        }
        if (rc < 0) {
            sceIoDclose(dir);
            return RFVP_PSV_IO;
        }
        if (strcmp(entry.d_name, ".") == 0 || strcmp(entry.d_name, "..") == 0) {
            continue;
        }

        char child_full[RFVP_PSV_MAX_PATH];
        char child_rel[RFVP_PSV_MAX_PATH];
        int n_full = snprintf(child_full, sizeof(child_full), "%s/%s", full_dir, entry.d_name);
        int n_rel;
        if (relative_dir[0] == '\0') {
            n_rel = snprintf(child_rel, sizeof(child_rel), "%s", entry.d_name);
        } else {
            n_rel = snprintf(child_rel, sizeof(child_rel), "%s/%s", relative_dir, entry.d_name);
        }
        if (n_full < 0 || (size_t)n_full >= sizeof(child_full) || n_rel < 0 || (size_t)n_rel >= sizeof(child_rel)) {
            sceIoDclose(dir);
            return RFVP_PSV_CAPACITY_EXCEEDED;
        }

        if (SCE_S_ISDIR(entry.d_stat.st_mode)) {
            int32_t status = rfvp_enumerate_dir_recursive(child_full, child_rel, extension, visitor_ctx, visitor);
            if (status != RFVP_PSV_OK) {
                sceIoDclose(dir);
                return status;
            }
        } else if (rfvp_path_has_extension(entry.d_name, extension)) {
            RawFileInfo info;
            info.len = (uint64_t)entry.d_stat.st_size;
            info.kind = RAW_FILE_KIND_FILE;
            int32_t status = visitor(visitor_ctx, (const uint8_t *)child_rel, strlen(child_rel), info);
            if (status != RFVP_PSV_OK) {
                sceIoDclose(dir);
                return status;
            }
        }
    }

    sceIoDclose(dir);
    return RFVP_PSV_OK;
}
#else
static int32_t rfvp_enumerate_dir_recursive(const char *full_dir, const char *relative_dir, const char *extension, void *visitor_ctx, RawEnumerateByExtensionVisitorFn visitor) {
#if RFVP_PSV_HAS_DIRENT
    DIR *dir = opendir(full_dir);
    if (dir == NULL) {
        return errno == ENOENT ? RFVP_PSV_NOT_FOUND : RFVP_PSV_IO;
    }

    struct dirent *entry;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
            continue;
        }

        char child_full[RFVP_PSV_MAX_PATH];
        char child_rel[RFVP_PSV_MAX_PATH];
        int n_full = snprintf(child_full, sizeof(child_full), "%s/%s", full_dir, entry->d_name);
        int n_rel;
        if (relative_dir[0] == '\0') {
            n_rel = snprintf(child_rel, sizeof(child_rel), "%s", entry->d_name);
        } else {
            n_rel = snprintf(child_rel, sizeof(child_rel), "%s/%s", relative_dir, entry->d_name);
        }
        if (n_full < 0 || (size_t)n_full >= sizeof(child_full) || n_rel < 0 || (size_t)n_rel >= sizeof(child_rel)) {
            closedir(dir);
            return RFVP_PSV_CAPACITY_EXCEEDED;
        }

        struct stat st;
        if (stat(child_full, &st) != 0) {
            closedir(dir);
            return RFVP_PSV_IO;
        }
        if (S_ISDIR(st.st_mode)) {
            int32_t status = rfvp_enumerate_dir_recursive(child_full, child_rel, extension, visitor_ctx, visitor);
            if (status != RFVP_PSV_OK) {
                closedir(dir);
                return status;
            }
        } else if (S_ISREG(st.st_mode) && rfvp_path_has_extension(entry->d_name, extension)) {
            RawFileInfo info;
            info.len = (uint64_t)st.st_size;
            info.kind = RAW_FILE_KIND_FILE;
            int32_t status = visitor(visitor_ctx, (const uint8_t *)child_rel, strlen(child_rel), info);
            if (status != RFVP_PSV_OK) {
                closedir(dir);
                return status;
            }
        }
    }

    closedir(dir);
    return RFVP_PSV_OK;
#else
    (void)full_dir;
    (void)relative_dir;
    (void)extension;
    (void)visitor_ctx;
    (void)visitor;
    return RFVP_PSV_UNSUPPORTED;
#endif
}
#endif

static int32_t rfvp_c_enumerate_by_extension(void *ctx, const uint8_t *root, size_t root_len, const uint8_t *extension, size_t extension_len, void *visitor_ctx, RawEnumerateByExtensionVisitorFn visitor) {
    (void)ctx;
    if (extension == NULL || visitor == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    char full_root[RFVP_PSV_MAX_PATH];
    int32_t status = rfvp_build_full_path(root, root_len, full_root, sizeof(full_root));
    if (status != RFVP_PSV_OK) {
        return status;
    }
    char ext[64];
    status = rfvp_copy_path(extension, extension_len, ext, sizeof(ext));
    if (status != RFVP_PSV_OK) {
        return status;
    }
    return rfvp_enumerate_dir_recursive(full_root, "", ext, visitor_ctx, visitor);
}

static uint8_t rfvp_clamp_u8_from_float(float value) {
    if (value <= 0.0f) {
        return 0u;
    }
    if (value >= 1.0f) {
        return 255u;
    }
    return (uint8_t)(value * 255.0f + 0.5f);
}

static float rfvp_clamp01(float value) {
    if (value < 0.0f) {
        return 0.0f;
    }
    if (value > 1.0f) {
        return 1.0f;
    }
    return value;
}

static uint8_t rfvp_lerp_u8(uint8_t a, uint8_t b, float t) {
    float out = (float)a + ((float)b - (float)a) * t;
    if (out <= 0.0f) {
        return 0u;
    }
    if (out >= 255.0f) {
        return 255u;
    }
    return (uint8_t)(out + 0.5f);
}

static size_t rfvp_source_pixel_size(RawPixelFormat format) {
    switch (format) {
        case RAW_PIXEL_FORMAT_RGBA8:
        case RAW_PIXEL_FORMAT_BGRA8:
            return 4u;
        case RAW_PIXEL_FORMAT_RGB8:
            return 3u;
        case RAW_PIXEL_FORMAT_LUMA_A8:
            return 2u;
        case RAW_PIXEL_FORMAT_LUMA8:
        case RAW_PIXEL_FORMAT_ALPHA8:
            return 1u;
        default:
            return 0u;
    }
}

static void rfvp_decode_pixel(RawPixelFormat format, const uint8_t *src, uint8_t out_rgba[4]) {
    switch (format) {
        case RAW_PIXEL_FORMAT_RGBA8:
            out_rgba[0] = src[0];
            out_rgba[1] = src[1];
            out_rgba[2] = src[2];
            out_rgba[3] = src[3];
            break;
        case RAW_PIXEL_FORMAT_BGRA8:
            out_rgba[0] = src[2];
            out_rgba[1] = src[1];
            out_rgba[2] = src[0];
            out_rgba[3] = src[3];
            break;
        case RAW_PIXEL_FORMAT_RGB8:
            out_rgba[0] = src[0];
            out_rgba[1] = src[1];
            out_rgba[2] = src[2];
            out_rgba[3] = 255u;
            break;
        case RAW_PIXEL_FORMAT_LUMA8:
            out_rgba[0] = src[0];
            out_rgba[1] = src[0];
            out_rgba[2] = src[0];
            out_rgba[3] = 255u;
            break;
        case RAW_PIXEL_FORMAT_LUMA_A8:
            out_rgba[0] = src[0];
            out_rgba[1] = src[0];
            out_rgba[2] = src[0];
            out_rgba[3] = src[1];
            break;
        case RAW_PIXEL_FORMAT_ALPHA8:
            out_rgba[0] = 255u;
            out_rgba[1] = 255u;
            out_rgba[2] = 255u;
            out_rgba[3] = src[0];
            break;
        default:
            out_rgba[0] = 0u;
            out_rgba[1] = 0u;
            out_rgba[2] = 0u;
            out_rgba[3] = 0u;
            break;
    }
}

static RfvpPsvTexture *rfvp_texture_by_id(uint32_t texture_id) {
    if (texture_id >= RFVP_PSV_MAX_TEXTURES) {
        return NULL;
    }
    return &g_rfvp_psv.renderer.textures[texture_id];
}

static int32_t rfvp_c_create_texture(void *ctx, uint32_t texture_id, RawTextureDesc desc, const uint8_t *pixels, size_t pixels_len) {
    (void)ctx;
    if (desc.width == 0u || desc.height == 0u) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    RfvpPsvTexture *texture = rfvp_texture_by_id(texture_id);
    if (texture == NULL) {
        return RFVP_PSV_CAPACITY_EXCEEDED;
    }

    size_t pixel_size = rfvp_source_pixel_size(desc.format);
    if (pixel_size == 0u) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    size_t count = (size_t)desc.width * (size_t)desc.height;
    if (pixels != NULL && pixels_len < count * pixel_size) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }

    uint8_t *new_pixels = (uint8_t *)malloc(count * 4u);
    if (new_pixels == NULL) {
        return RFVP_PSV_OUT_OF_MEMORY;
    }

    if (pixels == NULL) {
        memset(new_pixels, 0, count * 4u);
    } else {
        for (size_t i = 0; i < count; ++i) {
            rfvp_decode_pixel(desc.format, pixels + i * pixel_size, new_pixels + i * 4u);
        }
    }

    if (texture->rgba8 != NULL) {
        free(texture->rgba8);
    }
    texture->used = 1u;
    texture->width = desc.width;
    texture->height = desc.height;
    texture->rgba8 = new_pixels;
    return RFVP_PSV_OK;
}

static int32_t rfvp_c_update_texture(void *ctx, uint32_t texture_id, RawTextureRect rect, const uint8_t *pixels, size_t pixels_len) {
    (void)ctx;
    RfvpPsvTexture *texture = rfvp_texture_by_id(texture_id);
    if (texture == NULL || texture->used == 0u || texture->rgba8 == NULL || pixels == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    if (rect.x > texture->width || rect.y > texture->height || rect.width > texture->width - rect.x || rect.height > texture->height - rect.y) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }

    size_t needed = (size_t)rect.width * (size_t)rect.height * 4u;
    if (pixels_len < needed) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    for (uint32_t y = 0; y < rect.height; ++y) {
        uint8_t *dst = texture->rgba8 + ((size_t)(rect.y + y) * (size_t)texture->width + (size_t)rect.x) * 4u;
        const uint8_t *src = pixels + (size_t)y * (size_t)rect.width * 4u;
        memcpy(dst, src, (size_t)rect.width * 4u);
    }
    return RFVP_PSV_OK;
}

static void rfvp_c_destroy_texture(void *ctx, uint32_t texture_id) {
    (void)ctx;
    RfvpPsvTexture *texture = rfvp_texture_by_id(texture_id);
    if (texture == NULL) {
        return;
    }
    free(texture->rgba8);
    texture->used = 0u;
    texture->width = 0u;
    texture->height = 0u;
    texture->rgba8 = NULL;
}

static void rfvp_put_pixel_blend(uint8_t *dst, const uint8_t src[4], RawBlendMode blend) {
    float sr = (float)src[0] / 255.0f;
    float sg = (float)src[1] / 255.0f;
    float sb = (float)src[2] / 255.0f;
    float sa = (float)src[3] / 255.0f;
    float dr = (float)dst[0] / 255.0f;
    float dg = (float)dst[1] / 255.0f;
    float db = (float)dst[2] / 255.0f;
    float da = (float)dst[3] / 255.0f;
    float out_r;
    float out_g;
    float out_b;
    float out_a;

    switch (blend) {
        case RAW_BLEND_MODE_OPAQUE:
            dst[0] = src[0];
            dst[1] = src[1];
            dst[2] = src[2];
            dst[3] = src[3];
            return;
        case RAW_BLEND_MODE_ADD:
            out_r = rfvp_clamp01(dr + sr * sa);
            out_g = rfvp_clamp01(dg + sg * sa);
            out_b = rfvp_clamp01(db + sb * sa);
            out_a = rfvp_clamp01(da + sa);
            break;
        case RAW_BLEND_MODE_MULTIPLY:
            out_r = dr * (1.0f - sa + sr * sa);
            out_g = dg * (1.0f - sa + sg * sa);
            out_b = db * (1.0f - sa + sb * sa);
            out_a = rfvp_clamp01(sa + da * (1.0f - sa));
            break;
        case RAW_BLEND_MODE_SCREEN:
            out_r = dr * (1.0f - sa) + (1.0f - (1.0f - sr) * (1.0f - dr)) * sa;
            out_g = dg * (1.0f - sa) + (1.0f - (1.0f - sg) * (1.0f - dg)) * sa;
            out_b = db * (1.0f - sa) + (1.0f - (1.0f - sb) * (1.0f - db)) * sa;
            out_a = rfvp_clamp01(sa + da * (1.0f - sa));
            break;
        case RAW_BLEND_MODE_ALPHA:
        default:
            out_r = sr * sa + dr * (1.0f - sa);
            out_g = sg * sa + dg * (1.0f - sa);
            out_b = sb * sa + db * (1.0f - sa);
            out_a = rfvp_clamp01(sa + da * (1.0f - sa));
            break;
    }

    dst[0] = rfvp_clamp_u8_from_float(out_r);
    dst[1] = rfvp_clamp_u8_from_float(out_g);
    dst[2] = rfvp_clamp_u8_from_float(out_b);
    dst[3] = rfvp_clamp_u8_from_float(out_a);
}

static int32_t rfvp_c_begin_frame(void *ctx, uint32_t width, uint32_t height, const RawColorRgba *clear) {
    (void)ctx;
    if (width == 0u || height == 0u) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    RfvpPsvRendererState *renderer = &g_rfvp_psv.renderer;
    size_t stride = (size_t)width * 4u;
    size_t size = stride * (size_t)height;
    if (renderer->width != width || renderer->height != height || renderer->backbuffer == NULL) {
        uint8_t *new_buffer = (uint8_t *)malloc(size);
        if (new_buffer == NULL) {
            return RFVP_PSV_OUT_OF_MEMORY;
        }
        free(renderer->backbuffer);
        renderer->backbuffer = new_buffer;
        renderer->width = width;
        renderer->height = height;
        renderer->stride_bytes = (uint32_t)stride;
    }

    uint8_t clear_pixel[4] = {0u, 0u, 0u, 0u};
    if (clear != NULL) {
        clear_pixel[0] = rfvp_clamp_u8_from_float(clear->r);
        clear_pixel[1] = rfvp_clamp_u8_from_float(clear->g);
        clear_pixel[2] = rfvp_clamp_u8_from_float(clear->b);
        clear_pixel[3] = rfvp_clamp_u8_from_float(clear->a);
    }
    for (uint32_t y = 0; y < height; ++y) {
        uint8_t *row = renderer->backbuffer + (size_t)y * (size_t)renderer->stride_bytes;
        for (uint32_t x = 0; x < width; ++x) {
            memcpy(row + (size_t)x * 4u, clear_pixel, 4u);
        }
    }
    return RFVP_PSV_OK;
}

static void rfvp_clip_rect(int32_t *x0, int32_t *y0, int32_t *x1, int32_t *y1, const RawRectI32 *scissor, uint8_t has_scissor) {
    if (*x0 < 0) *x0 = 0;
    if (*y0 < 0) *y0 = 0;
    if (*x1 > (int32_t)g_rfvp_psv.renderer.width) *x1 = (int32_t)g_rfvp_psv.renderer.width;
    if (*y1 > (int32_t)g_rfvp_psv.renderer.height) *y1 = (int32_t)g_rfvp_psv.renderer.height;
    if (has_scissor && scissor != NULL) {
        int32_t sx0 = scissor->x;
        int32_t sy0 = scissor->y;
        int32_t sx1 = scissor->x + scissor->width;
        int32_t sy1 = scissor->y + scissor->height;
        if (*x0 < sx0) *x0 = sx0;
        if (*y0 < sy0) *y0 = sy0;
        if (*x1 > sx1) *x1 = sx1;
        if (*y1 > sy1) *y1 = sy1;
    }
}

static int32_t rfvp_c_draw_solid(void *ctx, const RawDrawSolidCommand *command) {
    (void)ctx;
    if (command == NULL || g_rfvp_psv.renderer.backbuffer == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    int32_t x0 = command->rect.x;
    int32_t y0 = command->rect.y;
    int32_t x1 = command->rect.x + command->rect.width;
    int32_t y1 = command->rect.y + command->rect.height;
    rfvp_clip_rect(&x0, &y0, &x1, &y1, &command->scissor, command->has_scissor);
    if (x1 <= x0 || y1 <= y0) {
        return RFVP_PSV_OK;
    }

    uint8_t src[4] = {
        rfvp_clamp_u8_from_float(command->color.r),
        rfvp_clamp_u8_from_float(command->color.g),
        rfvp_clamp_u8_from_float(command->color.b),
        rfvp_clamp_u8_from_float(command->color.a),
    };
    for (int32_t y = y0; y < y1; ++y) {
        uint8_t *row = g_rfvp_psv.renderer.backbuffer + (size_t)y * (size_t)g_rfvp_psv.renderer.stride_bytes;
        for (int32_t x = x0; x < x1; ++x) {
            rfvp_put_pixel_blend(row + (size_t)x * 4u, src, command->blend);
        }
    }
    return RFVP_PSV_OK;
}


static float rfvp_min3(float a, float b, float c) {
    float m = a < b ? a : b;
    return m < c ? m : c;
}

static float rfvp_max3(float a, float b, float c) {
    float m = a > b ? a : b;
    return m > c ? m : c;
}

static int32_t rfvp_floor_to_i32(float value) {
    int32_t i = (int32_t)value;
    return ((float)i > value) ? i - 1 : i;
}

static int32_t rfvp_ceil_to_i32(float value) {
    int32_t i = (int32_t)value;
    return ((float)i < value) ? i + 1 : i;
}

static float rfvp_edge(float ax, float ay, float bx, float by, float px, float py) {
    return (px - ax) * (by - ay) - (py - ay) * (bx - ax);
}

static void rfvp_sample_texture_nearest(const RfvpPsvTexture *texture, float u, float v, uint8_t out[4]) {
    u = rfvp_clamp01(u);
    v = rfvp_clamp01(v);
    uint32_t x = (uint32_t)(u * (float)(texture->width - 1u) + 0.5f);
    uint32_t y = (uint32_t)(v * (float)(texture->height - 1u) + 0.5f);
    memcpy(out, texture->rgba8 + ((size_t)y * (size_t)texture->width + (size_t)x) * 4u, 4u);
}

static void rfvp_sample_texture_linear(const RfvpPsvTexture *texture, float u, float v, uint8_t out[4]) {
    u = rfvp_clamp01(u);
    v = rfvp_clamp01(v);
    float fx = u * (float)(texture->width - 1u);
    float fy = v * (float)(texture->height - 1u);
    uint32_t x0 = (uint32_t)fx;
    uint32_t y0 = (uint32_t)fy;
    uint32_t x1 = x0 + 1u < texture->width ? x0 + 1u : x0;
    uint32_t y1 = y0 + 1u < texture->height ? y0 + 1u : y0;
    float tx = fx - (float)x0;
    float ty = fy - (float)y0;
    const uint8_t *p00 = texture->rgba8 + ((size_t)y0 * (size_t)texture->width + (size_t)x0) * 4u;
    const uint8_t *p10 = texture->rgba8 + ((size_t)y0 * (size_t)texture->width + (size_t)x1) * 4u;
    const uint8_t *p01 = texture->rgba8 + ((size_t)y1 * (size_t)texture->width + (size_t)x0) * 4u;
    const uint8_t *p11 = texture->rgba8 + ((size_t)y1 * (size_t)texture->width + (size_t)x1) * 4u;
    for (int i = 0; i < 4; ++i) {
        uint8_t a = rfvp_lerp_u8(p00[i], p10[i], tx);
        uint8_t b = rfvp_lerp_u8(p01[i], p11[i], tx);
        out[i] = rfvp_lerp_u8(a, b, ty);
    }
}

static void rfvp_draw_triangle(const RfvpPsvTexture *texture, const RawVertex2D *a, const RawVertex2D *b, const RawVertex2D *c, RawTextureFilter filter, RawBlendMode blend, const RawRectI32 *scissor, uint8_t has_scissor) {
    float ax = a->position[0];
    float ay = a->position[1];
    float bx = b->position[0];
    float by = b->position[1];
    float cx = c->position[0];
    float cy = c->position[1];

    float area = rfvp_edge(ax, ay, bx, by, cx, cy);
    if (area == 0.0f) {
        return;
    }

    float min_xf = rfvp_min3(ax, bx, cx);
    float min_yf = rfvp_min3(ay, by, cy);
    float max_xf = rfvp_max3(ax, bx, cx);
    float max_yf = rfvp_max3(ay, by, cy);
    int32_t x0 = rfvp_floor_to_i32(min_xf);
    int32_t y0 = rfvp_floor_to_i32(min_yf);
    int32_t x1 = rfvp_ceil_to_i32(max_xf);
    int32_t y1 = rfvp_ceil_to_i32(max_yf);
    rfvp_clip_rect(&x0, &y0, &x1, &y1, scissor, has_scissor);
    if (x1 <= x0 || y1 <= y0) {
        return;
    }

    for (int32_t y = y0; y < y1; ++y) {
        uint8_t *row = g_rfvp_psv.renderer.backbuffer + (size_t)y * (size_t)g_rfvp_psv.renderer.stride_bytes;
        for (int32_t x = x0; x < x1; ++x) {
            float px = (float)x + 0.5f;
            float py = (float)y + 0.5f;
            float w0 = rfvp_edge(bx, by, cx, cy, px, py) / area;
            float w1 = rfvp_edge(cx, cy, ax, ay, px, py) / area;
            float w2 = rfvp_edge(ax, ay, bx, by, px, py) / area;
            if (w0 < 0.0f || w1 < 0.0f || w2 < 0.0f) {
                continue;
            }
            float u = a->tex_coord[0] * w0 + b->tex_coord[0] * w1 + c->tex_coord[0] * w2;
            float v = a->tex_coord[1] * w0 + b->tex_coord[1] * w1 + c->tex_coord[1] * w2;
            uint8_t texel[4];
            if (filter == RAW_TEXTURE_FILTER_LINEAR) {
                rfvp_sample_texture_linear(texture, u, v, texel);
            } else {
                rfvp_sample_texture_nearest(texture, u, v, texel);
            }

            float cr = a->color.r * w0 + b->color.r * w1 + c->color.r * w2;
            float cg = a->color.g * w0 + b->color.g * w1 + c->color.g * w2;
            float cb = a->color.b * w0 + b->color.b * w1 + c->color.b * w2;
            float ca = a->color.a * w0 + b->color.a * w1 + c->color.a * w2;
            uint8_t src[4] = {
                rfvp_clamp_u8_from_float(((float)texel[0] / 255.0f) * cr),
                rfvp_clamp_u8_from_float(((float)texel[1] / 255.0f) * cg),
                rfvp_clamp_u8_from_float(((float)texel[2] / 255.0f) * cb),
                rfvp_clamp_u8_from_float(((float)texel[3] / 255.0f) * ca),
            };
            rfvp_put_pixel_blend(row + (size_t)x * 4u, src, blend);
        }
    }
}

static int32_t rfvp_c_draw_sprite(void *ctx, const RawDrawSpriteCommand *command) {
    (void)ctx;
    if (command == NULL || g_rfvp_psv.renderer.backbuffer == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    RfvpPsvTexture *texture = rfvp_texture_by_id(command->texture_id);
    if (texture == NULL || texture->used == 0u || texture->rgba8 == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    rfvp_draw_triangle(texture, &command->vertices[0], &command->vertices[1], &command->vertices[2], command->filter, command->blend, &command->scissor, command->has_scissor);
    rfvp_draw_triangle(texture, &command->vertices[0], &command->vertices[2], &command->vertices[3], command->filter, command->blend, &command->scissor, command->has_scissor);
    return RFVP_PSV_OK;
}

static int32_t rfvp_c_end_frame(void *ctx) {
    (void)ctx;
    return RFVP_PSV_OK;
}

void rfvp_psv_c_set_present_callback(RfvpPsvPresentCallback callback, void *ctx) {
    g_rfvp_psv.renderer.present_callback = callback;
    g_rfvp_psv.renderer.present_callback_ctx = ctx;
}

void rfvp_psv_c_set_external_framebuffer_rgba8(uint8_t *pixels, uint32_t width, uint32_t height, uint32_t stride_bytes) {
    g_rfvp_psv.renderer.external_framebuffer = pixels;
    g_rfvp_psv.renderer.external_width = width;
    g_rfvp_psv.renderer.external_height = height;
    g_rfvp_psv.renderer.external_stride_bytes = stride_bytes;
}

const uint8_t *rfvp_psv_c_backbuffer_rgba8(uint32_t *out_width, uint32_t *out_height, uint32_t *out_stride_bytes) {
    if (out_width != NULL) {
        *out_width = g_rfvp_psv.renderer.width;
    }
    if (out_height != NULL) {
        *out_height = g_rfvp_psv.renderer.height;
    }
    if (out_stride_bytes != NULL) {
        *out_stride_bytes = g_rfvp_psv.renderer.stride_bytes;
    }
    return g_rfvp_psv.renderer.backbuffer;
}

static int32_t rfvp_c_present(void *ctx) {
    (void)ctx;
    RfvpPsvRendererState *renderer = &g_rfvp_psv.renderer;
    if (renderer->backbuffer == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    if (renderer->present_callback != NULL) {
        int32_t status = renderer->present_callback(
            renderer->present_callback_ctx,
            renderer->backbuffer,
            renderer->width,
            renderer->height,
            renderer->stride_bytes
        );
        if (status != RFVP_PSV_OK) {
            return status;
        }
    }
    if (renderer->external_framebuffer != NULL) {
        if (renderer->external_width < renderer->width || renderer->external_height < renderer->height || renderer->external_stride_bytes < renderer->stride_bytes) {
            return RFVP_PSV_INVALID_ARGUMENT;
        }
        for (uint32_t y = 0; y < renderer->height; ++y) {
            memcpy(
                renderer->external_framebuffer + (size_t)y * (size_t)renderer->external_stride_bytes,
                renderer->backbuffer + (size_t)y * (size_t)renderer->stride_bytes,
                renderer->stride_bytes
            );
        }
    }
    return RFVP_PSV_OK;
}

void rfvp_psv_c_renderer_shutdown(void) {
    RfvpPsvRendererState *renderer = &g_rfvp_psv.renderer;
    for (uint32_t i = 0; i < RFVP_PSV_MAX_TEXTURES; ++i) {
        free(renderer->textures[i].rgba8);
        renderer->textures[i].used = 0u;
        renderer->textures[i].width = 0u;
        renderer->textures[i].height = 0u;
        renderer->textures[i].rgba8 = NULL;
    }
    free(renderer->backbuffer);
    renderer->backbuffer = NULL;
    renderer->width = 0u;
    renderer->height = 0u;
    renderer->stride_bytes = 0u;
    renderer->external_framebuffer = NULL;
    renderer->external_width = 0u;
    renderer->external_height = 0u;
    renderer->external_stride_bytes = 0u;
    renderer->present_callback = NULL;
    renderer->present_callback_ctx = NULL;
}

static RfvpPsvAudioStream *rfvp_audio_stream_by_id(uint32_t stream_id) {
    if (stream_id >= RFVP_PSV_MAX_AUDIO_STREAMS) {
        return NULL;
    }
    return &g_rfvp_psv.audio.streams[stream_id];
}

static uint64_t rfvp_audio_phase_step(uint32_t sample_rate) {
    return (((uint64_t)sample_rate) << 32) / (uint64_t)RFVP_PSV_AUDIO_OUTPUT_RATE;
}

static int32_t rfvp_audio_supported_rate(uint32_t sample_rate) {
    switch (sample_rate) {
        case 8000u:
        case 11025u:
        case 12000u:
        case 16000u:
        case 22050u:
        case 24000u:
        case 32000u:
        case 44100u:
        case 48000u:
            return 1;
        default:
            return 0;
    }
}

static int16_t rfvp_audio_f32_to_i16(float sample) {
    if (sample <= -1.0f) {
        return (int16_t)-32768;
    }
    if (sample >= 1.0f) {
        return (int16_t)32767;
    }
    float scaled = sample * 32767.0f;
    if (scaled >= 0.0f) {
        return (int16_t)(scaled + 0.5f);
    }
    return (int16_t)(scaled - 0.5f);
}

static int16_t rfvp_audio_saturate_i32(int value) {
    if (value > 32767) {
        return 32767;
    }
    if (value < -32768) {
        return -32768;
    }
    return (int16_t)value;
}

static int32_t rfvp_audio_ensure_port(void) {
#if defined(RFVP_PSV_VITASDK_BACKEND)
    if (g_rfvp_psv.audio.port_open) {
        return RFVP_PSV_OK;
    }
    int port = sceAudioOutOpenPort(
        SCE_AUDIO_OUT_PORT_TYPE_MAIN,
        (int)RFVP_PSV_AUDIO_PORT_LEN,
        (int)RFVP_PSV_AUDIO_OUTPUT_RATE,
        SCE_AUDIO_OUT_MODE_STEREO
    );
    if (port < 0) {
        return RFVP_PSV_BACKEND;
    }
    g_rfvp_psv.audio.port = port;
    g_rfvp_psv.audio.port_open = 1;
    int vol[2] = { SCE_AUDIO_VOLUME_0DB, SCE_AUDIO_VOLUME_0DB };
    if (sceAudioOutSetVolume(port, SCE_AUDIO_VOLUME_FLAG_L_CH | SCE_AUDIO_VOLUME_FLAG_R_CH, vol) < 0) {
        sceAudioOutReleasePort(port);
        g_rfvp_psv.audio.port = -1;
        g_rfvp_psv.audio.port_open = 0;
        return RFVP_PSV_BACKEND;
    }
    return RFVP_PSV_OK;
#else
    return RFVP_PSV_OK;
#endif
}

void rfvp_psv_c_audio_shutdown(void) {
    for (uint32_t i = 0; i < RFVP_PSV_MAX_AUDIO_STREAMS; ++i) {
        RfvpPsvAudioStream *stream = &g_rfvp_psv.audio.streams[i];
        free(stream->samples);
        memset(stream, 0, sizeof(*stream));
    }
#if defined(RFVP_PSV_VITASDK_BACKEND)
    if (g_rfvp_psv.audio.port_open) {
        sceAudioOutOutput(g_rfvp_psv.audio.port, NULL);
        sceAudioOutReleasePort(g_rfvp_psv.audio.port);
        g_rfvp_psv.audio.port = -1;
        g_rfvp_psv.audio.port_open = 0;
    }
#endif
}

static int32_t rfvp_c_audio_create_stream(void *ctx, uint32_t stream_id, RawAudioStreamDesc desc) {
    (void)ctx;
    RfvpPsvAudioStream *stream = rfvp_audio_stream_by_id(stream_id);
    if (stream == NULL) {
        return RFVP_PSV_CAPACITY_EXCEEDED;
    }
    if (!rfvp_audio_supported_rate(desc.sample_rate) || (desc.channels != 1u && desc.channels != 2u)) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    if (desc.sample_format != RAW_AUDIO_SAMPLE_FORMAT_I16 && desc.sample_format != RAW_AUDIO_SAMPLE_FORMAT_F32) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    free(stream->samples);
    memset(stream, 0, sizeof(*stream));
    stream->used = 1u;
    stream->sample_rate = desc.sample_rate;
    stream->channels = desc.channels;
    stream->format = desc.sample_format;
    stream->params.volume = 1.0f;
    stream->params.pan = 0.0f;
    stream->params.repeat = 0u;
    stream->phase_step = rfvp_audio_phase_step(desc.sample_rate);
    return rfvp_audio_ensure_port();
}

static int32_t rfvp_audio_append_i16(RfvpPsvAudioStream *stream, const int16_t *samples, size_t sample_count) {
    if (sample_count == 0u) {
        return RFVP_PSV_OK;
    }
    if (samples == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    if (sample_count > ((size_t)-1) / sizeof(int16_t) - stream->sample_count) {
        return RFVP_PSV_CAPACITY_EXCEEDED;
    }
    size_t new_count = stream->sample_count + sample_count;
    int16_t *new_samples = (int16_t *)realloc(stream->samples, new_count * sizeof(int16_t));
    if (new_samples == NULL) {
        return RFVP_PSV_OUT_OF_MEMORY;
    }
    memcpy(new_samples + stream->sample_count, samples, sample_count * sizeof(int16_t));
    stream->samples = new_samples;
    stream->sample_count = new_count;
    return RFVP_PSV_OK;
}

static int32_t rfvp_c_audio_submit_i16(void *ctx, uint32_t stream_id, const int16_t *samples, size_t sample_count) {
    (void)ctx;
    RfvpPsvAudioStream *stream = rfvp_audio_stream_by_id(stream_id);
    if (stream == NULL || !stream->used) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    if ((sample_count % (size_t)stream->channels) != 0u) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    return rfvp_audio_append_i16(stream, samples, sample_count);
}

static int32_t rfvp_c_audio_submit_f32(void *ctx, uint32_t stream_id, const float *samples, size_t sample_count) {
    (void)ctx;
    RfvpPsvAudioStream *stream = rfvp_audio_stream_by_id(stream_id);
    if (stream == NULL || !stream->used || (samples == NULL && sample_count != 0u)) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    if ((sample_count % (size_t)stream->channels) != 0u) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    if (sample_count == 0u) {
        return RFVP_PSV_OK;
    }
    int16_t *tmp = (int16_t *)malloc(sample_count * sizeof(int16_t));
    if (tmp == NULL) {
        return RFVP_PSV_OUT_OF_MEMORY;
    }
    for (size_t i = 0; i < sample_count; ++i) {
        tmp[i] = rfvp_audio_f32_to_i16(samples[i]);
    }
    int32_t status = rfvp_audio_append_i16(stream, tmp, sample_count);
    free(tmp);
    return status;
}

static int32_t rfvp_c_audio_play(void *ctx, uint32_t stream_id, RawAudioParams params) {
    (void)ctx;
    RfvpPsvAudioStream *stream = rfvp_audio_stream_by_id(stream_id);
    if (stream == NULL || !stream->used || stream->samples == NULL || stream->sample_count == 0u) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    int32_t status = rfvp_audio_ensure_port();
    if (status != RFVP_PSV_OK) {
        return status;
    }
    stream->params = params;
    stream->phase = 0u;
    stream->playing = 1u;
    stream->stopping = 0u;
    stream->fade_total_us = 0u;
    stream->fade_remaining_us = 0u;
    stream->fade_start_volume = params.volume;
    return RFVP_PSV_OK;
}

static int32_t rfvp_c_audio_stop(void *ctx, uint32_t stream_id, uint32_t fade_ms) {
    (void)ctx;
    RfvpPsvAudioStream *stream = rfvp_audio_stream_by_id(stream_id);
    if (stream == NULL || !stream->used) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    if (fade_ms == 0u || !stream->playing) {
        stream->playing = 0u;
        stream->stopping = 0u;
        return RFVP_PSV_OK;
    }
    stream->stopping = 1u;
    stream->fade_total_us = (uint64_t)fade_ms * 1000ull;
    stream->fade_remaining_us = stream->fade_total_us;
    stream->fade_start_volume = stream->params.volume;
    return RFVP_PSV_OK;
}

static int32_t rfvp_c_audio_set_params(void *ctx, uint32_t stream_id, RawAudioParams params) {
    (void)ctx;
    RfvpPsvAudioStream *stream = rfvp_audio_stream_by_id(stream_id);
    if (stream == NULL || !stream->used) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    stream->params = params;
    return RFVP_PSV_OK;
}

static void rfvp_c_audio_destroy_stream(void *ctx, uint32_t stream_id) {
    (void)ctx;
    RfvpPsvAudioStream *stream = rfvp_audio_stream_by_id(stream_id);
    if (stream == NULL) {
        return;
    }
    free(stream->samples);
    memset(stream, 0, sizeof(*stream));
}

static void rfvp_audio_mix_stream(RfvpPsvAudioStream *stream, int32_t *mix_l, int32_t *mix_r, size_t frames) {
    size_t source_frames = stream->sample_count / (size_t)stream->channels;
    if (!stream->playing || source_frames == 0u) {
        return;
    }

    float volume = stream->params.volume;
    if (volume < 0.0f) {
        volume = 0.0f;
    }
    if (volume > 1.0f) {
        volume = 1.0f;
    }
    float pan = stream->params.pan;
    if (pan < -1.0f) {
        pan = -1.0f;
    }
    if (pan > 1.0f) {
        pan = 1.0f;
    }
    float left_gain = volume * (pan <= 0.0f ? 1.0f : 1.0f - pan);
    float right_gain = volume * (pan >= 0.0f ? 1.0f : 1.0f + pan);

    for (size_t frame = 0; frame < frames; ++frame) {
        size_t src_frame = (size_t)(stream->phase >> 32);
        if (src_frame >= source_frames) {
            if (stream->params.repeat) {
                stream->phase = 0u;
                src_frame = 0u;
            } else {
                stream->playing = 0u;
                stream->stopping = 0u;
                break;
            }
        }

        size_t base = src_frame * (size_t)stream->channels;
        int16_t src_l = stream->samples[base];
        int16_t src_r = stream->channels == 2u ? stream->samples[base + 1u] : src_l;
        mix_l[frame] += (int32_t)((float)src_l * left_gain);
        mix_r[frame] += (int32_t)((float)src_r * right_gain);
        stream->phase += stream->phase_step;
    }
}

static void rfvp_audio_compact_stream(RfvpPsvAudioStream *stream) {
    if (!stream->used || stream->params.repeat || stream->channels == 0u || stream->sample_count == 0u) {
        return;
    }
    size_t consumed_frames = (size_t)(stream->phase >> 32);
    if (consumed_frames == 0u) {
        return;
    }
    size_t source_frames = stream->sample_count / (size_t)stream->channels;
    if (consumed_frames >= source_frames) {
        stream->sample_count = 0u;
        stream->phase = 0u;
        return;
    }
    size_t consumed_samples = consumed_frames * (size_t)stream->channels;
    size_t remaining_samples = stream->sample_count - consumed_samples;
    memmove(stream->samples, stream->samples + consumed_samples, remaining_samples * sizeof(int16_t));
    stream->sample_count = remaining_samples;
    stream->phase -= ((uint64_t)consumed_frames << 32);
}

static int32_t rfvp_c_audio_tick(void *ctx, uint64_t delta_us) {
    (void)ctx;
    uint8_t has_playing = 0u;
    for (uint32_t i = 0; i < RFVP_PSV_MAX_AUDIO_STREAMS; ++i) {
        RfvpPsvAudioStream *stream = &g_rfvp_psv.audio.streams[i];
        if (!stream->used || !stream->playing) {
            continue;
        }
        has_playing = 1u;
        if (stream->stopping) {
            if (stream->fade_remaining_us <= delta_us) {
                stream->params.volume = 0.0f;
                stream->playing = 0u;
                stream->stopping = 0u;
                continue;
            }
            stream->fade_remaining_us -= delta_us;
            float ratio = (float)stream->fade_remaining_us / (float)stream->fade_total_us;
            stream->params.volume = stream->fade_start_volume * ratio;
        }
    }

    if (!has_playing) {
        return RFVP_PSV_OK;
    }
    int32_t status = rfvp_audio_ensure_port();
    if (status != RFVP_PSV_OK) {
        return status;
    }

    int32_t mix_l[RFVP_PSV_AUDIO_PORT_LEN];
    int32_t mix_r[RFVP_PSV_AUDIO_PORT_LEN];
    memset(mix_l, 0, sizeof(mix_l));
    memset(mix_r, 0, sizeof(mix_r));

    for (uint32_t i = 0; i < RFVP_PSV_MAX_AUDIO_STREAMS; ++i) {
        rfvp_audio_mix_stream(&g_rfvp_psv.audio.streams[i], mix_l, mix_r, RFVP_PSV_AUDIO_PORT_LEN);
    }
    for (uint32_t i = 0; i < RFVP_PSV_MAX_AUDIO_STREAMS; ++i) {
        rfvp_audio_compact_stream(&g_rfvp_psv.audio.streams[i]);
    }

    for (size_t i = 0; i < RFVP_PSV_AUDIO_PORT_LEN; ++i) {
        g_rfvp_psv.audio.mix_buffer[i * 2u] = rfvp_audio_saturate_i32(mix_l[i]);
        g_rfvp_psv.audio.mix_buffer[i * 2u + 1u] = rfvp_audio_saturate_i32(mix_r[i]);
    }

#if defined(RFVP_PSV_VITASDK_BACKEND)
    int rc = sceAudioOutOutput(g_rfvp_psv.audio.port, g_rfvp_psv.audio.mix_buffer);
    if (rc < 0) {
        return RFVP_PSV_BACKEND;
    }
#endif
    return RFVP_PSV_OK;
}

void rfvp_psv_c_clock_set_ticks_us(uint64_t ticks_us) {
    g_rfvp_psv.clock.manual_ticks_us = ticks_us;
    g_rfvp_psv.clock.use_manual_ticks = 1u;
}

void rfvp_psv_c_clock_advance_us(uint64_t delta_us) {
    g_rfvp_psv.clock.manual_ticks_us += delta_us;
    g_rfvp_psv.clock.use_manual_ticks = 1u;
}

static uint64_t rfvp_c_ticks_us(void *ctx) {
    (void)ctx;
    if (g_rfvp_psv.clock.use_manual_ticks) {
        return g_rfvp_psv.clock.manual_ticks_us;
    }
#if defined(RFVP_PSV_VITASDK_BACKEND)
    return (uint64_t)sceKernelGetProcessTimeWide();
#elif defined(CLOCK_MONOTONIC)
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC, &ts) == 0) {
        return (uint64_t)ts.tv_sec * 1000000ull + (uint64_t)ts.tv_nsec / 1000ull;
    }
    return g_rfvp_psv.clock.manual_ticks_us;
#else
    return g_rfvp_psv.clock.manual_ticks_us;
#endif
}

static void rfvp_c_log(void *ctx, uint32_t level, const uint8_t *message, size_t message_len) {
    (void)ctx;
    (void)level;
    if (message == NULL || message_len == 0u) {
        return;
    }
    fwrite(message, 1u, message_len, stderr);
    fputc('\n', stderr);
}

int32_t rfvp_psv_make_raw_host(RawPsvHost *out_host) {
    if (out_host == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }

    RawFileSystemVTable fs = {
        .open = rfvp_c_open,
        .close = rfvp_c_close,
        .read_at = rfvp_c_read_at,
        .len = rfvp_c_len,
        .metadata = rfvp_c_metadata,
        .write_all = rfvp_c_write_all,
        .enumerate_by_extension = rfvp_c_enumerate_by_extension,
    };
    RawRendererVTable renderer = {
        .create_texture = rfvp_c_create_texture,
        .update_texture = rfvp_c_update_texture,
        .destroy_texture = rfvp_c_destroy_texture,
        .begin_frame = rfvp_c_begin_frame,
        .draw_sprite = rfvp_c_draw_sprite,
        .draw_solid = rfvp_c_draw_solid,
        .end_frame = rfvp_c_end_frame,
        .present = rfvp_c_present,
    };
    RawAudioVTable audio = {
        .create_stream = rfvp_c_audio_create_stream,
        .submit_i16 = rfvp_c_audio_submit_i16,
        .submit_f32 = rfvp_c_audio_submit_f32,
        .play = rfvp_c_audio_play,
        .stop = rfvp_c_audio_stop,
        .set_params = rfvp_c_audio_set_params,
        .destroy_stream = rfvp_c_audio_destroy_stream,
        .tick = rfvp_c_audio_tick,
    };
    RawClockVTable clock = {
        .ticks_us = rfvp_c_ticks_us,
    };

    out_host->fs_ctx = &g_rfvp_psv.fs;
    out_host->fs = fs;
    out_host->renderer_ctx = &g_rfvp_psv.renderer;
    out_host->renderer = renderer;
    out_host->audio_ctx = NULL;
    out_host->audio = audio;
    out_host->clock_ctx = &g_rfvp_psv.clock;
    out_host->clock = clock;
    out_host->log_ctx = NULL;
    out_host->log = rfvp_c_log;
    return RFVP_PSV_OK;
}

#ifndef RFVP_PSV_VITASDK_BACKEND
int32_t rfvp_psv_platform_poll(PsvApp *app) {
    (void)app;
    return RFVP_PSV_OK;
}

int32_t rfvp_psv_platform_should_exit(void) {
    return g_rfvp_psv.exit_requested ? 1 : 0;
}
#endif

void rfvp_psv_c_request_exit(void) {
    g_rfvp_psv.exit_requested = 1u;
}

void rfvp_psv_c_clear_exit_request(void) {
    g_rfvp_psv.exit_requested = 0u;
}

void rfvp_psv_platform_fatal_error(uint32_t code, const uint8_t *message, size_t message_len) {
    (void)code;
    char msg[512];
    size_t copy_len = message_len;
    if (copy_len >= sizeof(msg)) {
        copy_len = sizeof(msg) - 1u;
    }
    if (message != NULL && copy_len != 0u) {
        memcpy(msg, message, copy_len);
    }
    msg[copy_len] = '\0';

#if defined(RFVP_PSV_VITASDK_BACKEND)
    SceMsgDialogUserMessageParam user_msg;
    memset(&user_msg, 0, sizeof(user_msg));
    user_msg.buttonType = SCE_MSG_DIALOG_BUTTON_TYPE_OK;
    user_msg.msg = msg;

    SceMsgDialogParam param;
    sceMsgDialogParamInit(&param);
    param.mode = SCE_MSG_DIALOG_MODE_USER_MSG;
    param.userMsgParam = &user_msg;
    (void)sceMsgDialogInit(&param);
#else
    if (copy_len != 0u) {
        fwrite(msg, 1u, copy_len, stderr);
        fputc('\n', stderr);
    }
#endif
    rfvp_psv_c_request_exit();
}
