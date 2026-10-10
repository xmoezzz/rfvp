#ifndef RFVP_PS2SDK_BACKEND_H
#define RFVP_PS2SDK_BACKEND_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

enum {
    RFVP_PS2_OK = 0,
    RFVP_PS2_IO = -1,
    RFVP_PS2_NOT_FOUND = -2,
    RFVP_PS2_INVALID_DATA = -3,
    RFVP_PS2_INVALID_ARGUMENT = -4,
    RFVP_PS2_UNSUPPORTED = -5,
    RFVP_PS2_OUT_OF_MEMORY = -6,
    RFVP_PS2_CAPACITY_EXCEEDED = -7,
    RFVP_PS2_END_OF_FILE = -8,
    RFVP_PS2_BACKEND = -9
};

typedef struct RawFileHandle {
    uint64_t value;
} RawFileHandle;

typedef enum RawFileKind {
    RawFileKind_File = 0,
    RawFileKind_Directory = 1,
    RawFileKind_Other = 2
} RawFileKind;

typedef struct RawFileInfo {
    uint64_t len;
    RawFileKind kind;
} RawFileInfo;

typedef enum RawPixelFormat {
    RawPixelFormat_Rgba8 = 0,
    RawPixelFormat_Bgra8 = 1,
    RawPixelFormat_Rgb8 = 2,
    RawPixelFormat_Luma8 = 3,
    RawPixelFormat_LumaA8 = 4,
    RawPixelFormat_Alpha8 = 5
} RawPixelFormat;

typedef struct RawTextureDesc {
    uint32_t width;
    uint32_t height;
    RawPixelFormat format;
    uint8_t mip_count;
    uint8_t padding[3];
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

typedef enum RawBlendMode {
    RawBlendMode_Opaque = 0,
    RawBlendMode_Alpha = 1,
    RawBlendMode_Add = 2,
    RawBlendMode_Multiply = 3,
    RawBlendMode_Screen = 4
} RawBlendMode;

typedef enum RawTextureFilter {
    RawTextureFilter_Nearest = 0,
    RawTextureFilter_Linear = 1
} RawTextureFilter;

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
    uint8_t padding[3];
    RawRectI32 scissor;
} RawDrawSpriteCommand;

typedef struct RawDrawSolidCommand {
    RawRectI32 rect;
    RawColorRgba color;
    RawBlendMode blend;
    uint8_t has_scissor;
    uint8_t padding[3];
    RawRectI32 scissor;
} RawDrawSolidCommand;

typedef struct RawAudioParams {
    float volume;
    float pan;
    uint8_t repeat;
    uint8_t padding[3];
} RawAudioParams;

typedef int (*RawOpenFileFn)(void *, const uint8_t *, size_t, RawFileHandle *);
typedef void (*RawCloseFileFn)(void *, RawFileHandle);
typedef int (*RawReadAtFn)(void *, RawFileHandle, uint64_t, uint8_t *, size_t, size_t *);
typedef int (*RawFileLenFn)(void *, RawFileHandle, uint64_t *);
typedef int (*RawMetadataFn)(void *, const uint8_t *, size_t, RawFileInfo *);
typedef int (*RawWriteAllFn)(void *, const uint8_t *, size_t, const uint8_t *, size_t);
typedef int (*RawEnumerateVisitorFn)(void *, const uint8_t *, size_t, RawFileInfo);
typedef int (*RawEnumerateByExtensionFn)(
    void *,
    const uint8_t *,
    size_t,
    const uint8_t *,
    size_t,
    void *,
    RawEnumerateVisitorFn);

typedef struct RawFileSystemVTable {
    RawOpenFileFn open;
    RawCloseFileFn close;
    RawReadAtFn read_at;
    RawFileLenFn len;
    RawMetadataFn metadata;
    RawWriteAllFn write_all;
    RawEnumerateByExtensionFn enumerate_by_extension;
} RawFileSystemVTable;

typedef int (*RawRendererInitFn)(void *, uint32_t, uint32_t);
typedef void (*RawRendererShutdownFn)(void *);
typedef int (*RawCreateTextureFn)(void *, uint32_t, RawTextureDesc, const uint8_t *, size_t, size_t);
typedef int (*RawUpdateTextureFn)(void *, uint32_t, RawTextureRect, const uint8_t *, size_t, size_t);
typedef void (*RawDestroyTextureFn)(void *, uint32_t);
typedef int (*RawBeginFrameFn)(void *, uint32_t, uint32_t, const RawColorRgba *);
typedef int (*RawDrawSpriteFn)(void *, const RawDrawSpriteCommand *);
typedef int (*RawDrawSolidFn)(void *, const RawDrawSolidCommand *);
typedef int (*RawEndFrameFn)(void *);
typedef int (*RawPresentFn)(void *);

typedef struct RawRendererVTable {
    RawRendererInitFn init;
    RawRendererShutdownFn shutdown;
    RawCreateTextureFn create_texture;
    RawUpdateTextureFn update_texture;
    RawDestroyTextureFn destroy_texture;
    RawBeginFrameFn begin_frame;
    RawDrawSpriteFn draw_sprite;
    RawDrawSolidFn draw_solid;
    RawEndFrameFn end_frame;
    RawPresentFn present;
} RawRendererVTable;

typedef int (*RawLoadNativeAudioFn)(void *, uint32_t, const uint8_t *, size_t);
typedef int (*RawPlayNativeAudioFn)(void *, uint32_t, RawAudioParams, uint32_t);
typedef int (*RawStopNativeAudioFn)(void *, uint32_t, uint32_t);
typedef int (*RawPauseNativeAudioFn)(void *, uint32_t);
typedef int (*RawResumeNativeAudioFn)(void *, uint32_t);
typedef int (*RawSetNativeAudioParamsFn)(void *, uint32_t, RawAudioParams);
typedef void (*RawDestroyNativeAudioFn)(void *, uint32_t);
typedef int (*RawAudioTickFn)(void *, uint64_t);

typedef struct RawAudioVTable {
    RawLoadNativeAudioFn load_native;
    RawPlayNativeAudioFn play;
    RawStopNativeAudioFn stop;
    RawPauseNativeAudioFn pause;
    RawResumeNativeAudioFn resume;
    RawSetNativeAudioParamsFn set_params;
    RawDestroyNativeAudioFn destroy;
    RawAudioTickFn tick;
} RawAudioVTable;

typedef uint64_t (*RawTicksUsFn)(void *);

typedef struct RawClockVTable {
    RawTicksUsFn ticks_us;
} RawClockVTable;

typedef void (*RawPs2LogFn)(void *, uint32_t, const uint8_t *, size_t);
typedef void (*RawPs2FatalFn)(void *, uint32_t, const uint8_t *, size_t);

typedef struct RawPs2Host {
    void *fs_ctx;
    RawFileSystemVTable fs;
    void *renderer_ctx;
    RawRendererVTable renderer;
    void *audio_ctx;
    RawAudioVTable audio;
    void *clock_ctx;
    RawClockVTable clock;
    void *log_ctx;
    RawPs2LogFn log;
    void *fatal_ctx;
    RawPs2FatalFn fatal;
} RawPs2Host;

int rfvp_ps2_platform_init(int argc, char **argv);
void rfvp_ps2_platform_fini(void);
int rfvp_ps2_make_raw_host(RawPs2Host *out_host);
int rfvp_ps2_platform_poll(void *app);
int rfvp_ps2_platform_should_exit(void);

int rfvp_ps2_app_main(const RawPs2Host *host);
int rfvp_ps2_app_push_key(void *app, uint32_t key_id, int pressed);
int rfvp_ps2_app_push_quit(void *app);

#ifdef __cplusplus
}
#endif

#endif
