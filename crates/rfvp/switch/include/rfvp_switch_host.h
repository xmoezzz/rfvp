#ifndef RFVP_SWITCH_HOST_H
#define RFVP_SWITCH_HOST_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define RFVP_SWITCH_BUTTON_A     (1u << 0)
#define RFVP_SWITCH_BUTTON_B     (1u << 1)
#define RFVP_SWITCH_BUTTON_X     (1u << 2)
#define RFVP_SWITCH_BUTTON_Y     (1u << 3)
#define RFVP_SWITCH_BUTTON_L     (1u << 4)
#define RFVP_SWITCH_BUTTON_R     (1u << 5)
#define RFVP_SWITCH_BUTTON_ZL    (1u << 6)
#define RFVP_SWITCH_BUTTON_ZR    (1u << 7)
#define RFVP_SWITCH_BUTTON_PLUS  (1u << 8)
#define RFVP_SWITCH_BUTTON_MINUS (1u << 9)
#define RFVP_SWITCH_BUTTON_UP    (1u << 10)
#define RFVP_SWITCH_BUTTON_DOWN  (1u << 11)
#define RFVP_SWITCH_BUTTON_LEFT  (1u << 12)
#define RFVP_SWITCH_BUTTON_RIGHT (1u << 13)

typedef struct RfvpSwitchInputFrame {
    uint32_t buttons_down;
    uint32_t buttons_up;
    uint32_t buttons_held;
    uint32_t touch_active;
    uint32_t touch_down;
    uint32_t touch_up;
    int32_t touch_x;
    int32_t touch_y;
} RfvpSwitchInputFrame;

typedef struct RfvpSwitchCoreStats {
    uint32_t abi_version;
    uint64_t frame_no;
    int32_t last_status;
    uint32_t forced_yield;
    uint32_t forced_yield_contexts;
    uint32_t main_thread_exited;
    uint32_t game_should_exit;
} RfvpSwitchCoreStats;

typedef struct RfvpSwitchTextureId {
    uint32_t value;
} RfvpSwitchTextureId;

typedef struct RfvpSwitchTextureDesc {
    RfvpSwitchTextureId id;
    uint32_t width;
    uint32_t height;
} RfvpSwitchTextureDesc;

typedef struct RfvpSwitchTextureUploadRgba8 {
    RfvpSwitchTextureDesc desc;
    const uint8_t *data;
    size_t byte_len;
    uint64_t generation;
} RfvpSwitchTextureUploadRgba8;

typedef struct RfvpSwitchRectF32 {
    float x;
    float y;
    float w;
    float h;
} RfvpSwitchRectF32;

typedef struct RfvpSwitchColorF32 {
    float r;
    float g;
    float b;
    float a;
} RfvpSwitchColorF32;

typedef struct RfvpSwitchMat4F32 {
    float cols[4][4];
} RfvpSwitchMat4F32;

typedef struct RfvpSwitchTexturedQuad {
    RfvpSwitchTextureId texture;
    RfvpSwitchRectF32 dst;
    RfvpSwitchRectF32 uv;
    RfvpSwitchColorF32 color;
    RfvpSwitchMat4F32 transform;
} RfvpSwitchTexturedQuad;

typedef struct RfvpSwitchFillQuad {
    RfvpSwitchRectF32 dst;
    RfvpSwitchColorF32 color;
    RfvpSwitchMat4F32 transform;
} RfvpSwitchFillQuad;

typedef enum RfvpSwitchRenderCommandKind {
    RFVP_SWITCH_RENDER_NONE = 0,
    RFVP_SWITCH_RENDER_BEGIN_FRAME = 1,
    RFVP_SWITCH_RENDER_END_FRAME = 2,
    RFVP_SWITCH_RENDER_CLEAR = 3,
    RFVP_SWITCH_RENDER_UPLOAD_TEXTURE_RGBA8 = 4,
    RFVP_SWITCH_RENDER_DRAW_TEXTURED_QUAD = 5,
    RFVP_SWITCH_RENDER_DRAW_FILL_QUAD = 6,
} RfvpSwitchRenderCommandKind;

typedef union RfvpSwitchRenderCommandPayload {
    RfvpSwitchColorF32 color;
    RfvpSwitchTextureDesc texture;
    RfvpSwitchTextureUploadRgba8 texture_upload;
    RfvpSwitchTexturedQuad textured_quad;
    RfvpSwitchFillQuad fill_quad;
    uint8_t empty[160];
} RfvpSwitchRenderCommandPayload;

typedef struct RfvpSwitchRenderCommand {
    RfvpSwitchRenderCommandKind kind;
    RfvpSwitchRenderCommandPayload payload;
} RfvpSwitchRenderCommand;

uint32_t rfvp_switch_host_api_version(void);
uint32_t rfvp_switch_host_render_api_version(void);
uint32_t rfvp_switch_host_audio_api_version(void);
uint32_t rfvp_switch_host_core_abi_version(void);

int32_t rfvp_switch_host_global_init(void);
int32_t rfvp_switch_host_global_begin_frame(void);
int32_t rfvp_switch_host_global_end_frame(void);
uint64_t rfvp_switch_host_global_frame_no(void);

int32_t rfvp_switch_host_global_load_game_root(
    const char *game_root_utf8,
    const char *nls_utf8,
    uint32_t width,
    uint32_t height
);
int32_t rfvp_switch_host_global_tick(uint32_t frame_time_ms, const RfvpSwitchInputFrame *input);
int32_t rfvp_switch_host_global_core_status(void);
int32_t rfvp_switch_host_global_core_stats(RfvpSwitchCoreStats *out);
void rfvp_switch_host_global_destroy_core(void);

size_t rfvp_switch_host_global_render_command_count(void);
const RfvpSwitchRenderCommand *rfvp_switch_host_global_render_commands(void);
size_t rfvp_switch_host_global_audio_queued_samples(void);
size_t rfvp_switch_host_global_audio_pop_i16(int16_t *out, size_t len);

#ifdef __cplusplus
}
#endif

#endif /* RFVP_SWITCH_HOST_H */
