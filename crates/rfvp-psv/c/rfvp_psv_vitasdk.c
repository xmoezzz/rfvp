#include "rfvp_psv_c.h"

#include <stdint.h>
#include <stddef.h>
#include <string.h>

#include <psp2/ctrl.h>
#include <psp2/display.h>
#include <psp2/kernel/processmgr.h>
#include <psp2/touch.h>

#ifndef RFVP_PSV_VITASDK_SCREEN_WIDTH
#define RFVP_PSV_VITASDK_SCREEN_WIDTH 960u
#endif

#ifndef RFVP_PSV_VITASDK_SCREEN_HEIGHT
#define RFVP_PSV_VITASDK_SCREEN_HEIGHT 544u
#endif

#ifndef RFVP_PSV_VITASDK_ASSET_ROOT
#define RFVP_PSV_VITASDK_ASSET_ROOT "app0:/"
#endif

#ifndef RFVP_PSV_VITASDK_CURSOR_STEP
#define RFVP_PSV_VITASDK_CURSOR_STEP 8
#endif

#ifndef RFVP_PSV_VITASDK_TOUCH_MAX_X
#define RFVP_PSV_VITASDK_TOUCH_MAX_X 1919u
#endif

#ifndef RFVP_PSV_VITASDK_TOUCH_MAX_Y
#define RFVP_PSV_VITASDK_TOUCH_MAX_Y 1087u
#endif

#define RFVP_PSV_OK 0
#define RFVP_PSV_INVALID_ARGUMENT (-4)
#define RFVP_PSV_BACKEND (-9)

typedef struct RfvpPsvVitasdkState {
    int display_ready;
    int touch_active;
    int exit_requested;
    int32_t touch_x;
    int32_t touch_y;
    int32_t cursor_x;
    int32_t cursor_y;
    uint32_t previous_buttons;
} RfvpPsvVitasdkState;

static uint32_t g_rfvp_psv_display[RFVP_PSV_VITASDK_SCREEN_WIDTH * RFVP_PSV_VITASDK_SCREEN_HEIGHT] __attribute__((aligned(64)));

static RfvpPsvVitasdkState g_rfvp_psv_vitasdk = {
    .cursor_x = (int32_t)(RFVP_PSV_VITASDK_SCREEN_WIDTH / 2u),
    .cursor_y = (int32_t)(RFVP_PSV_VITASDK_SCREEN_HEIGHT / 2u),
};

static int32_t rfvp_psv_vitasdk_present_callback(
    void *ctx,
    const uint8_t *rgba8,
    uint32_t width,
    uint32_t height,
    uint32_t stride_bytes
) {
    (void)ctx;
    if (rgba8 == NULL || width == 0u || height == 0u || stride_bytes < width * 4u) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    if (!g_rfvp_psv_vitasdk.display_ready) {
        return RFVP_PSV_BACKEND;
    }

    const uint32_t out_w = RFVP_PSV_VITASDK_SCREEN_WIDTH;
    const uint32_t out_h = RFVP_PSV_VITASDK_SCREEN_HEIGHT;
    uint8_t *dst = (uint8_t *)g_rfvp_psv_display;
    const uint32_t dst_stride = out_w * 4u;

    if (width == out_w && height == out_h && stride_bytes == dst_stride) {
        memcpy(dst, rgba8, (size_t)out_h * (size_t)dst_stride);
    } else {
        for (uint32_t y = 0u; y < out_h; ++y) {
            uint32_t src_y = (uint32_t)(((uint64_t)y * (uint64_t)height) / (uint64_t)out_h);
            const uint8_t *src_row = rgba8 + (size_t)src_y * (size_t)stride_bytes;
            uint8_t *dst_row = dst + (size_t)y * (size_t)dst_stride;
            for (uint32_t x = 0u; x < out_w; ++x) {
                uint32_t src_x = (uint32_t)(((uint64_t)x * (uint64_t)width) / (uint64_t)out_w);
                const uint8_t *src_px = src_row + (size_t)src_x * 4u;
                uint8_t *dst_px = dst_row + (size_t)x * 4u;
                dst_px[0] = src_px[0];
                dst_px[1] = src_px[1];
                dst_px[2] = src_px[2];
                dst_px[3] = src_px[3];
            }
        }
    }

    SceDisplayFrameBuf framebuf;
    memset(&framebuf, 0, sizeof(framebuf));
    framebuf.size = sizeof(SceDisplayFrameBuf);
    framebuf.base = (void *)g_rfvp_psv_display;
    framebuf.pitch = RFVP_PSV_VITASDK_SCREEN_WIDTH;
    framebuf.pixelformat = SCE_DISPLAY_PIXELFORMAT_A8B8G8R8;
    framebuf.width = RFVP_PSV_VITASDK_SCREEN_WIDTH;
    framebuf.height = RFVP_PSV_VITASDK_SCREEN_HEIGHT;

    int rc = sceDisplaySetFrameBuf(&framebuf, SCE_DISPLAY_SETBUF_NEXTFRAME);
    if (rc < 0) {
        return RFVP_PSV_BACKEND;
    }
    sceDisplayWaitVblankStart();
    return RFVP_PSV_OK;
}

static int32_t rfvp_psv_vitasdk_push_cursor_move(PsvApp *app) {
    return rfvp_psv_app_push_pointer_move(
        app,
        g_rfvp_psv_vitasdk.cursor_x,
        g_rfvp_psv_vitasdk.cursor_y,
        1
    );
}

static int32_t rfvp_psv_vitasdk_clamp_cursor_and_push(PsvApp *app) {
    if (g_rfvp_psv_vitasdk.cursor_x < 0) {
        g_rfvp_psv_vitasdk.cursor_x = 0;
    }
    if (g_rfvp_psv_vitasdk.cursor_y < 0) {
        g_rfvp_psv_vitasdk.cursor_y = 0;
    }
    if (g_rfvp_psv_vitasdk.cursor_x >= (int32_t)RFVP_PSV_VITASDK_SCREEN_WIDTH) {
        g_rfvp_psv_vitasdk.cursor_x = (int32_t)RFVP_PSV_VITASDK_SCREEN_WIDTH - 1;
    }
    if (g_rfvp_psv_vitasdk.cursor_y >= (int32_t)RFVP_PSV_VITASDK_SCREEN_HEIGHT) {
        g_rfvp_psv_vitasdk.cursor_y = (int32_t)RFVP_PSV_VITASDK_SCREEN_HEIGHT - 1;
    }
    return rfvp_psv_vitasdk_push_cursor_move(app);
}

static int32_t rfvp_psv_vitasdk_poll_pad(PsvApp *app) {
    SceCtrlData pad;
    memset(&pad, 0, sizeof(pad));

    int read_count = sceCtrlPeekBufferPositive(0, &pad, 1);
    if (read_count < 0) {
        return RFVP_PSV_BACKEND;
    }

    uint32_t buttons = pad.buttons;
    uint32_t down = buttons & ~g_rfvp_psv_vitasdk.previous_buttons;
    uint32_t up = g_rfvp_psv_vitasdk.previous_buttons & ~buttons;
    g_rfvp_psv_vitasdk.previous_buttons = buttons;

    if ((down & SCE_CTRL_START) != 0u) {
        g_rfvp_psv_vitasdk.exit_requested = 1;
        rfvp_psv_c_request_exit();
        return rfvp_psv_app_push_quit(app);
    }

    int moved = 0;
    if ((buttons & SCE_CTRL_LEFT) != 0u) {
        g_rfvp_psv_vitasdk.cursor_x -= RFVP_PSV_VITASDK_CURSOR_STEP;
        moved = 1;
    }
    if ((buttons & SCE_CTRL_RIGHT) != 0u) {
        g_rfvp_psv_vitasdk.cursor_x += RFVP_PSV_VITASDK_CURSOR_STEP;
        moved = 1;
    }
    if ((buttons & SCE_CTRL_UP) != 0u) {
        g_rfvp_psv_vitasdk.cursor_y -= RFVP_PSV_VITASDK_CURSOR_STEP;
        moved = 1;
    }
    if ((buttons & SCE_CTRL_DOWN) != 0u) {
        g_rfvp_psv_vitasdk.cursor_y += RFVP_PSV_VITASDK_CURSOR_STEP;
        moved = 1;
    }

    int stick_dx = (int)pad.lx - 128;
    int stick_dy = (int)pad.ly - 128;
    if (stick_dx > 24 || stick_dx < -24 || stick_dy > 24 || stick_dy < -24) {
        g_rfvp_psv_vitasdk.cursor_x += stick_dx / 16;
        g_rfvp_psv_vitasdk.cursor_y += stick_dy / 16;
        moved = 1;
    }

    if (moved) {
        int32_t status = rfvp_psv_vitasdk_clamp_cursor_and_push(app);
        if (status != RFVP_PSV_OK) {
            return status;
        }
    }

    if ((down & SCE_CTRL_CROSS) != 0u) {
        int32_t status = rfvp_psv_app_push_pointer_down(app, 0, g_rfvp_psv_vitasdk.cursor_x, g_rfvp_psv_vitasdk.cursor_y);
        if (status != RFVP_PSV_OK) {
            return status;
        }
    }
    if ((up & SCE_CTRL_CROSS) != 0u) {
        int32_t status = rfvp_psv_app_push_pointer_up(app, 0, g_rfvp_psv_vitasdk.cursor_x, g_rfvp_psv_vitasdk.cursor_y);
        if (status != RFVP_PSV_OK) {
            return status;
        }
    }
    if ((down & SCE_CTRL_CIRCLE) != 0u) {
        int32_t status = rfvp_psv_app_push_pointer_down(app, 1, g_rfvp_psv_vitasdk.cursor_x, g_rfvp_psv_vitasdk.cursor_y);
        if (status != RFVP_PSV_OK) {
            return status;
        }
    }
    if ((up & SCE_CTRL_CIRCLE) != 0u) {
        int32_t status = rfvp_psv_app_push_pointer_up(app, 1, g_rfvp_psv_vitasdk.cursor_x, g_rfvp_psv_vitasdk.cursor_y);
        if (status != RFVP_PSV_OK) {
            return status;
        }
    }

    return RFVP_PSV_OK;
}

static int32_t rfvp_psv_vitasdk_poll_touch(PsvApp *app) {
    SceTouchData touch;
    memset(&touch, 0, sizeof(touch));

    int read_count = sceTouchPeek(SCE_TOUCH_PORT_FRONT, &touch, 1);
    if (read_count < 0) {
        return RFVP_PSV_BACKEND;
    }

    if (touch.reportNum > 0) {
        const SceTouchReport *report = &touch.report[0];
        int32_t x = (int32_t)(((uint64_t)report->x * (uint64_t)RFVP_PSV_VITASDK_SCREEN_WIDTH) / ((uint64_t)RFVP_PSV_VITASDK_TOUCH_MAX_X + 1ull));
        int32_t y = (int32_t)(((uint64_t)report->y * (uint64_t)RFVP_PSV_VITASDK_SCREEN_HEIGHT) / ((uint64_t)RFVP_PSV_VITASDK_TOUCH_MAX_Y + 1ull));

        if (x < 0) {
            x = 0;
        }
        if (y < 0) {
            y = 0;
        }
        if (x >= (int32_t)RFVP_PSV_VITASDK_SCREEN_WIDTH) {
            x = (int32_t)RFVP_PSV_VITASDK_SCREEN_WIDTH - 1;
        }
        if (y >= (int32_t)RFVP_PSV_VITASDK_SCREEN_HEIGHT) {
            y = (int32_t)RFVP_PSV_VITASDK_SCREEN_HEIGHT - 1;
        }

        g_rfvp_psv_vitasdk.cursor_x = x;
        g_rfvp_psv_vitasdk.cursor_y = y;
        g_rfvp_psv_vitasdk.touch_x = x;
        g_rfvp_psv_vitasdk.touch_y = y;

        int32_t status = rfvp_psv_app_push_pointer_move(app, x, y, 1);
        if (status != RFVP_PSV_OK) {
            return status;
        }
        if (!g_rfvp_psv_vitasdk.touch_active) {
            status = rfvp_psv_app_push_touch(app, 0, 0, x, y);
            if (status != RFVP_PSV_OK) {
                return status;
            }
            status = rfvp_psv_app_push_pointer_down(app, 0, x, y);
            if (status != RFVP_PSV_OK) {
                return status;
            }
        } else {
            status = rfvp_psv_app_push_touch(app, 1, 0, x, y);
            if (status != RFVP_PSV_OK) {
                return status;
            }
        }
        g_rfvp_psv_vitasdk.touch_active = 1;
        return RFVP_PSV_OK;
    }

    if (g_rfvp_psv_vitasdk.touch_active) {
        g_rfvp_psv_vitasdk.touch_active = 0;
        int32_t status = rfvp_psv_app_push_touch(app, 2, 0, g_rfvp_psv_vitasdk.touch_x, g_rfvp_psv_vitasdk.touch_y);
        if (status != RFVP_PSV_OK) {
            return status;
        }
        return rfvp_psv_app_push_pointer_up(app, 0, g_rfvp_psv_vitasdk.touch_x, g_rfvp_psv_vitasdk.touch_y);
    }

    return RFVP_PSV_OK;
}

void rfvp_psv_vitasdk_init(void) {
    const char *asset_root = RFVP_PSV_VITASDK_ASSET_ROOT;
    int32_t root_status = rfvp_psv_c_set_asset_root(asset_root);
    if (root_status != RFVP_PSV_OK) {
        sceKernelExitProcess(root_status);
    }

    sceCtrlSetSamplingMode(SCE_CTRL_MODE_ANALOG);
    sceTouchSetSamplingState(SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_START);

    memset(g_rfvp_psv_display, 0, sizeof(g_rfvp_psv_display));

    SceDisplayFrameBuf framebuf;
    memset(&framebuf, 0, sizeof(framebuf));
    framebuf.size = sizeof(SceDisplayFrameBuf);
    framebuf.base = (void *)g_rfvp_psv_display;
    framebuf.pitch = RFVP_PSV_VITASDK_SCREEN_WIDTH;
    framebuf.pixelformat = SCE_DISPLAY_PIXELFORMAT_A8B8G8R8;
    framebuf.width = RFVP_PSV_VITASDK_SCREEN_WIDTH;
    framebuf.height = RFVP_PSV_VITASDK_SCREEN_HEIGHT;

    int rc = sceDisplaySetFrameBuf(&framebuf, SCE_DISPLAY_SETBUF_IMMEDIATE);
    if (rc < 0) {
        sceKernelExitProcess(rc);
    }

    g_rfvp_psv_vitasdk.display_ready = 1;
    rfvp_psv_c_set_present_callback(rfvp_psv_vitasdk_present_callback, NULL);
}

void rfvp_psv_vitasdk_fini(void) {
    rfvp_psv_c_audio_shutdown();
    rfvp_psv_c_renderer_shutdown();
    rfvp_psv_c_set_present_callback(NULL, NULL);
    sceTouchSetSamplingState(SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_STOP);
    sceDisplaySetFrameBuf(NULL, SCE_DISPLAY_SETBUF_IMMEDIATE);
    g_rfvp_psv_vitasdk.display_ready = 0;
}

int32_t rfvp_psv_platform_poll(PsvApp *app) {
    if (app == NULL) {
        return RFVP_PSV_INVALID_ARGUMENT;
    }
    if (g_rfvp_psv_vitasdk.exit_requested) {
        return rfvp_psv_app_push_quit(app);
    }

    int32_t status = rfvp_psv_vitasdk_poll_pad(app);
    if (status != RFVP_PSV_OK) {
        return status;
    }
    return rfvp_psv_vitasdk_poll_touch(app);
}

int32_t rfvp_psv_platform_should_exit(void) {
    return g_rfvp_psv_vitasdk.exit_requested ? 1 : 0;
}
