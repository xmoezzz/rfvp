#ifndef RFVP_PSV_C_H
#define RFVP_PSV_C_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct PsvApp PsvApp;

typedef struct RawFileHandle {
    uint64_t value;
} RawFileHandle;

typedef enum RawFileKind {
    RAW_FILE_KIND_FILE = 0,
    RAW_FILE_KIND_DIRECTORY = 1,
    RAW_FILE_KIND_OTHER = 2,
} RawFileKind;

typedef struct RawFileInfo {
    uint64_t len;
    RawFileKind kind;
} RawFileInfo;

typedef int32_t (*RawOpenFileFn)(void *ctx, const uint8_t *path, size_t path_len, RawFileHandle *out_handle);
typedef void (*RawCloseFileFn)(void *ctx, RawFileHandle handle);
typedef int32_t (*RawReadAtFn)(void *ctx, RawFileHandle handle, uint64_t offset, uint8_t *buf, size_t len, size_t *out_read);
typedef int32_t (*RawFileLenFn)(void *ctx, RawFileHandle handle, uint64_t *out_len);
typedef int32_t (*RawMetadataFn)(void *ctx, const uint8_t *path, size_t path_len, RawFileInfo *out_info);
typedef int32_t (*RawWriteAllFn)(void *ctx, const uint8_t *path, size_t path_len, const uint8_t *bytes, size_t byte_len);
typedef int32_t (*RawEnumerateByExtensionVisitorFn)(void *visitor_ctx, const uint8_t *path, size_t path_len, RawFileInfo info);
typedef int32_t (*RawEnumerateByExtensionFn)(void *ctx, const uint8_t *root, size_t root_len, const uint8_t *extension, size_t extension_len, void *visitor_ctx, RawEnumerateByExtensionVisitorFn visitor);

typedef struct RawFileSystemVTable {
    RawOpenFileFn open;
    RawCloseFileFn close;
    RawReadAtFn read_at;
    RawFileLenFn len;
    RawMetadataFn metadata;
    RawWriteAllFn write_all;
    RawEnumerateByExtensionFn enumerate_by_extension;
} RawFileSystemVTable;

typedef enum RawPixelFormat {
    RAW_PIXEL_FORMAT_RGBA8 = 0,
    RAW_PIXEL_FORMAT_BGRA8 = 1,
    RAW_PIXEL_FORMAT_RGB8 = 2,
    RAW_PIXEL_FORMAT_LUMA8 = 3,
    RAW_PIXEL_FORMAT_LUMA_A8 = 4,
    RAW_PIXEL_FORMAT_ALPHA8 = 5,
} RawPixelFormat;

typedef enum RawBlendMode {
    RAW_BLEND_MODE_OPAQUE = 0,
    RAW_BLEND_MODE_ALPHA = 1,
    RAW_BLEND_MODE_ADD = 2,
    RAW_BLEND_MODE_MULTIPLY = 3,
    RAW_BLEND_MODE_SCREEN = 4,
} RawBlendMode;

typedef enum RawTextureFilter {
    RAW_TEXTURE_FILTER_NEAREST = 0,
    RAW_TEXTURE_FILTER_LINEAR = 1,
} RawTextureFilter;

typedef struct RawTextureDesc {
    uint32_t width;
    uint32_t height;
    RawPixelFormat format;
    uint8_t mip_count;
    uint8_t _padding[3];
} RawTextureDesc;

typedef struct RawTextureRect {
    uint32_t x;
    uint32_t y;
    uint32_t width;
    uint32_t height;
} RawTextureRect;

typedef struct RawColorRgba {
    float r;
    float g;
    float b;
    float a;
} RawColorRgba;

typedef struct RawRectI32 {
    int32_t x;
    int32_t y;
    int32_t width;
    int32_t height;
} RawRectI32;

typedef struct RawVertex2D {
    float position[2];
    float tex_coord[2];
    RawColorRgba color;
} RawVertex2D;

typedef struct RawDrawSpriteCommand {
    uint32_t texture_id;
    RawVertex2D vertices[4];
    RawBlendMode blend;
    RawTextureFilter filter;
    uint8_t has_scissor;
    uint8_t _padding[3];
    RawRectI32 scissor;
} RawDrawSpriteCommand;

typedef struct RawDrawSolidCommand {
    RawRectI32 rect;
    RawColorRgba color;
    RawBlendMode blend;
    uint8_t has_scissor;
    uint8_t _padding[3];
    RawRectI32 scissor;
} RawDrawSolidCommand;

typedef int32_t (*RawCreateTextureFn)(void *ctx, uint32_t texture_id, RawTextureDesc desc, const uint8_t *pixels, size_t pixels_len);
typedef int32_t (*RawUpdateTextureFn)(void *ctx, uint32_t texture_id, RawTextureRect rect, const uint8_t *pixels, size_t pixels_len);
typedef void (*RawDestroyTextureFn)(void *ctx, uint32_t texture_id);
typedef int32_t (*RawBeginFrameFn)(void *ctx, uint32_t width, uint32_t height, const RawColorRgba *clear);
typedef int32_t (*RawDrawSpriteFn)(void *ctx, const RawDrawSpriteCommand *command);
typedef int32_t (*RawDrawSolidFn)(void *ctx, const RawDrawSolidCommand *command);
typedef int32_t (*RawEndFrameFn)(void *ctx);
typedef int32_t (*RawPresentFn)(void *ctx);

typedef struct RawRendererVTable {
    RawCreateTextureFn create_texture;
    RawUpdateTextureFn update_texture;
    RawDestroyTextureFn destroy_texture;
    RawBeginFrameFn begin_frame;
    RawDrawSpriteFn draw_sprite;
    RawDrawSolidFn draw_solid;
    RawEndFrameFn end_frame;
    RawPresentFn present;
} RawRendererVTable;

typedef enum RawAudioSampleFormat {
    RAW_AUDIO_SAMPLE_FORMAT_I16 = 0,
    RAW_AUDIO_SAMPLE_FORMAT_F32 = 1,
} RawAudioSampleFormat;

typedef struct RawAudioStreamDesc {
    uint32_t sample_rate;
    uint16_t channels;
    RawAudioSampleFormat sample_format;
} RawAudioStreamDesc;

typedef struct RawAudioParams {
    float volume;
    float pan;
    uint8_t repeat;
    uint8_t _padding[3];
} RawAudioParams;

typedef int32_t (*RawCreateAudioStreamFn)(void *ctx, uint32_t stream_id, RawAudioStreamDesc desc);
typedef int32_t (*RawSubmitI16Fn)(void *ctx, uint32_t stream_id, const int16_t *samples, size_t sample_count);
typedef int32_t (*RawSubmitF32Fn)(void *ctx, uint32_t stream_id, const float *samples, size_t sample_count);
typedef int32_t (*RawPlayAudioFn)(void *ctx, uint32_t stream_id, RawAudioParams params);
typedef int32_t (*RawStopAudioFn)(void *ctx, uint32_t stream_id, uint32_t fade_ms);
typedef int32_t (*RawSetAudioParamsFn)(void *ctx, uint32_t stream_id, RawAudioParams params);
typedef void (*RawDestroyAudioStreamFn)(void *ctx, uint32_t stream_id);
typedef int32_t (*RawAudioTickFn)(void *ctx, uint64_t delta_us);

typedef struct RawAudioVTable {
    RawCreateAudioStreamFn create_stream;
    RawSubmitI16Fn submit_i16;
    RawSubmitF32Fn submit_f32;
    RawPlayAudioFn play;
    RawStopAudioFn stop;
    RawSetAudioParamsFn set_params;
    RawDestroyAudioStreamFn destroy_stream;
    RawAudioTickFn tick;
} RawAudioVTable;

typedef uint64_t (*RawTicksUsFn)(void *ctx);

typedef struct RawClockVTable {
    RawTicksUsFn ticks_us;
} RawClockVTable;

typedef void (*RawPsvLogFn)(void *ctx, uint32_t level, const uint8_t *message, size_t message_len);

typedef struct RawPsvHost {
    void *fs_ctx;
    RawFileSystemVTable fs;
    void *renderer_ctx;
    RawRendererVTable renderer;
    void *audio_ctx;
    RawAudioVTable audio;
    void *clock_ctx;
    RawClockVTable clock;
    void *log_ctx;
    RawPsvLogFn log;
} RawPsvHost;

typedef int32_t (*RfvpPsvPresentCallback)(void *ctx, const uint8_t *rgba8, uint32_t width, uint32_t height, uint32_t stride_bytes);

void *rfvp_psv_alloc(size_t size, size_t align);
void rfvp_psv_dealloc(void *ptr, size_t size, size_t align);

int32_t rfvp_psv_make_raw_host(RawPsvHost *out_host);
int32_t rfvp_psv_platform_poll(PsvApp *app);
int32_t rfvp_psv_platform_should_exit(void);
void rfvp_psv_vitasdk_init(void);
void rfvp_psv_vitasdk_fini(void);
void rfvp_psv_c_request_exit(void);
void rfvp_psv_c_clear_exit_request(void);
void rfvp_psv_c_renderer_shutdown(void);
void rfvp_psv_c_audio_shutdown(void);
void rfvp_psv_platform_fatal_error(uint32_t code, const uint8_t *message, size_t message_len);

int32_t rfvp_psv_c_set_asset_root(const char *root);
void rfvp_psv_c_set_present_callback(RfvpPsvPresentCallback callback, void *ctx);
void rfvp_psv_c_set_external_framebuffer_rgba8(uint8_t *pixels, uint32_t width, uint32_t height, uint32_t stride_bytes);
const uint8_t *rfvp_psv_c_backbuffer_rgba8(uint32_t *out_width, uint32_t *out_height, uint32_t *out_stride_bytes);
void rfvp_psv_c_clock_set_ticks_us(uint64_t ticks_us);
void rfvp_psv_c_clock_advance_us(uint64_t delta_us);

int32_t rfvp_psv_app_push_quit(PsvApp *app);
int32_t rfvp_psv_app_push_pointer_move(PsvApp *app, int32_t x, int32_t y, uint8_t in_screen);
int32_t rfvp_psv_app_push_pointer_down(PsvApp *app, uint32_t button, int32_t x, int32_t y);
int32_t rfvp_psv_app_push_pointer_up(PsvApp *app, uint32_t button, int32_t x, int32_t y);
int32_t rfvp_psv_app_push_wheel(PsvApp *app, int32_t delta_x, int32_t delta_y);
int32_t rfvp_psv_app_push_touch(PsvApp *app, uint32_t phase, uint64_t id, int32_t x, int32_t y);

#ifdef __cplusplus
}
#endif

#endif
