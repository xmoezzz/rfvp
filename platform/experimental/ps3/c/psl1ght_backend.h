#ifndef RFVP_PSL1GHT_BACKEND_H
#define RFVP_PSL1GHT_BACKEND_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum {
    RFVP_PS3_OK = 0,
    RFVP_PS3_IO = -1,
    RFVP_PS3_NOT_FOUND = -2,
    RFVP_PS3_INVALID_DATA = -3,
    RFVP_PS3_INVALID_ARGUMENT = -4,
    RFVP_PS3_UNSUPPORTED = -5,
    RFVP_PS3_OUT_OF_MEMORY = -6,
    RFVP_PS3_CAPACITY_EXCEEDED = -7,
    RFVP_PS3_END_OF_FILE = -8,
    RFVP_PS3_BACKEND = -9
};

typedef struct RawPS3InputState {
    uint32_t buttons;
    int32_t left_stick_x;
    int32_t left_stick_y;
} RawPS3InputState;

typedef struct RawPS3FileHandle {
    uint64_t value;
} RawPS3FileHandle;

typedef enum RawPS3FileKind {
    RawPS3FileKind_File = 0,
    RawPS3FileKind_Directory = 1,
    RawPS3FileKind_Other = 2
} RawPS3FileKind;

typedef struct RawPS3FileInfo {
    uint64_t len;
    RawPS3FileKind kind;
} RawPS3FileInfo;

typedef struct RawRgba8 {
    uint8_t r;
    uint8_t g;
    uint8_t b;
    uint8_t a;
} RawRgba8;

typedef int (*RawEnumerateVisitorFn)(void *, const uint8_t *, size_t, RawPS3FileInfo);

int rfvp_ps3_platform_init(int argc, char **argv);
void rfvp_ps3_platform_fini(void);
int rfvp_ps3_platform_should_exit(void);
void rfvp_ps3_platform_sleep_frame(void);
void rfvp_ps3_platform_log(uint32_t level, const uint8_t *message, size_t message_len);
void rfvp_ps3_platform_fatal(uint32_t code, const uint8_t *message, size_t message_len) __attribute__((noreturn));
uint64_t rfvp_ps3_platform_ticks_us(void);
int rfvp_ps3_platform_poll_input(RawPS3InputState *out_state);
int rfvp_ps3_platform_present_rgba8(const RawRgba8 *pixels, uint32_t width, uint32_t height);

int rfvp_ps3_platform_fs_open(const uint8_t *path, size_t path_len, RawPS3FileHandle *out_handle);
void rfvp_ps3_platform_fs_close(RawPS3FileHandle handle);
int rfvp_ps3_platform_fs_read_at(RawPS3FileHandle handle, uint64_t offset, uint8_t *buf, size_t len, size_t *out_read);
int rfvp_ps3_platform_fs_len(RawPS3FileHandle handle, uint64_t *out_len);
int rfvp_ps3_platform_fs_metadata(const uint8_t *path, size_t path_len, RawPS3FileInfo *out_info);
int rfvp_ps3_platform_fs_write_all(const uint8_t *path, size_t path_len, const uint8_t *bytes, size_t byte_len);
int rfvp_ps3_platform_fs_enumerate_by_extension(
    const uint8_t *root,
    size_t root_len,
    const uint8_t *extension,
    size_t extension_len,
    void *visitor_ctx,
    RawEnumerateVisitorFn visitor);

int rfvp_ps3_app_main(void);

#ifdef __cplusplus
}
#endif

#endif
