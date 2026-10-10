#ifndef RFVP_WUT_BACKEND_H
#define RFVP_WUT_BACKEND_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum {
    RFVP_WIIU_OK = 0,
    RFVP_WIIU_IO = -1,
    RFVP_WIIU_NOT_FOUND = -2,
    RFVP_WIIU_INVALID_DATA = -3,
    RFVP_WIIU_INVALID_ARGUMENT = -4,
    RFVP_WIIU_UNSUPPORTED = -5,
    RFVP_WIIU_OUT_OF_MEMORY = -6,
    RFVP_WIIU_CAPACITY_EXCEEDED = -7,
    RFVP_WIIU_END_OF_FILE = -8,
    RFVP_WIIU_BACKEND = -9
};

typedef struct RawWiiUInputState {
    uint32_t buttons;
    int32_t left_stick_x;
    int32_t left_stick_y;
} RawWiiUInputState;

typedef struct RawWiiUFileHandle {
    uint64_t value;
} RawWiiUFileHandle;

typedef enum RawWiiUFileKind {
    RawWiiUFileKind_File = 0,
    RawWiiUFileKind_Directory = 1,
    RawWiiUFileKind_Other = 2
} RawWiiUFileKind;

typedef struct RawWiiUFileInfo {
    uint64_t len;
    RawWiiUFileKind kind;
} RawWiiUFileInfo;

typedef struct RawRgba8 {
    uint8_t r;
    uint8_t g;
    uint8_t b;
    uint8_t a;
} RawRgba8;

typedef int (*RawEnumerateVisitorFn)(void *, const uint8_t *, size_t, RawWiiUFileInfo);

int rfvp_wiiu_platform_init(int argc, char **argv);
void rfvp_wiiu_platform_fini(void);
int rfvp_wiiu_platform_should_exit(void);
void rfvp_wiiu_platform_sleep_frame(void);
void rfvp_wiiu_platform_log(uint32_t level, const uint8_t *message, size_t message_len);
void rfvp_wiiu_platform_fatal(uint32_t code, const uint8_t *message, size_t message_len);
uint64_t rfvp_wiiu_platform_ticks_us(void);
int rfvp_wiiu_platform_poll_input(RawWiiUInputState *out_state);
int rfvp_wiiu_platform_present_rgba8(const RawRgba8 *pixels, uint32_t width, uint32_t height);

int rfvp_wiiu_platform_fs_open(const uint8_t *path, size_t path_len, RawWiiUFileHandle *out_handle);
void rfvp_wiiu_platform_fs_close(RawWiiUFileHandle handle);
int rfvp_wiiu_platform_fs_read_at(RawWiiUFileHandle handle, uint64_t offset, uint8_t *buf, size_t len, size_t *out_read);
int rfvp_wiiu_platform_fs_len(RawWiiUFileHandle handle, uint64_t *out_len);
int rfvp_wiiu_platform_fs_metadata(const uint8_t *path, size_t path_len, RawWiiUFileInfo *out_info);
int rfvp_wiiu_platform_fs_write_all(const uint8_t *path, size_t path_len, const uint8_t *bytes, size_t byte_len);
int rfvp_wiiu_platform_fs_enumerate_by_extension(
    const uint8_t *root,
    size_t root_len,
    const uint8_t *extension,
    size_t extension_len,
    void *visitor_ctx,
    RawEnumerateVisitorFn visitor);

int rfvp_wiiu_app_main(void);

#ifdef __cplusplus
}
#endif

#endif
