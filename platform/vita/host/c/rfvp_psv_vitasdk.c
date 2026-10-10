#include "rfvp_psv_c.h"

int32_t rfvp_psv_gpu_init(void);
void rfvp_psv_gpu_fini(void);

#include <stdint.h>
#include <stddef.h>
#include <string.h>

#include <psp2/audioout.h>
#include <psp2/ctrl.h>
#include <psp2/display.h>
#include <psp2/kernel/processmgr.h>
#include <psp2/kernel/sysmem.h>
#include <psp2/kernel/threadmgr.h>
#include <psp2/rtc.h>
#include <psp2/touch.h>

#ifndef RFVP_PSV_VITASDK_SCREEN_WIDTH
#define RFVP_PSV_VITASDK_SCREEN_WIDTH 960u
#endif

#ifndef RFVP_PSV_VITASDK_SCREEN_HEIGHT
#define RFVP_PSV_VITASDK_SCREEN_HEIGHT 544u
#endif

#ifndef RFVP_PSV_VITASDK_ASSET_ROOT
#define RFVP_PSV_VITASDK_ASSET_ROOT "ux0:data/rfvp/game/"
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


static RfvpPsvVitasdkState g_rfvp_psv_vitasdk = {
    .cursor_x = (int32_t)(RFVP_PSV_VITASDK_SCREEN_WIDTH / 2u),
    .cursor_y = (int32_t)(RFVP_PSV_VITASDK_SCREEN_HEIGHT / 2u),
};

/* ---- Audio thread -------------------------------------------------------------------------
 * sceAudioOutOutput blocks until the device takes the next buffer (about 21 ms here). Called from
 * the game loop it held every frame to that, which with the vblank wait gave 30 fps; so a thread
 * of its own pulls mixed audio from the engine (rfvp_psv_audio_pull) and feeds the device. */

#define RFVP_PSV_AUDIO_THREAD_FRAMES 1024
extern void rfvp_psv_audio_pull(int16_t *dst, size_t frames);

static volatile int g_rfvp_audio_run;
static SceUID g_rfvp_audio_thread = -1;
static int16_t g_rfvp_audio_buffers[2][RFVP_PSV_AUDIO_THREAD_FRAMES * 2] __attribute__((aligned(64)));

static int rfvp_psv_audio_thread(SceSize args, void *argp) {
    (void)args;
    (void)argp;
    int port = sceAudioOutOpenPort(SCE_AUDIO_OUT_PORT_TYPE_MAIN, RFVP_PSV_AUDIO_THREAD_FRAMES, 48000,
                                   SCE_AUDIO_OUT_MODE_STEREO);
    if (port < 0) {
        return sceKernelExitDeleteThread(0);
    }
    int vol[2] = { SCE_AUDIO_VOLUME_0DB, SCE_AUDIO_VOLUME_0DB };
    sceAudioOutSetVolume(port, SCE_AUDIO_VOLUME_FLAG_L_CH | SCE_AUDIO_VOLUME_FLAG_R_CH, vol);
    int which = 0;
    while (g_rfvp_audio_run) {
        rfvp_psv_audio_pull(g_rfvp_audio_buffers[which], RFVP_PSV_AUDIO_THREAD_FRAMES);
        sceAudioOutOutput(port, g_rfvp_audio_buffers[which]);
        which ^= 1;
    }
    sceAudioOutOutput(port, NULL);
    sceAudioOutReleasePort(port);
    return sceKernelExitDeleteThread(0);
}

void rfvp_psv_audio_thread_start(void) {
    if (g_rfvp_audio_thread >= 0) {
        return;
    }
    g_rfvp_audio_run = 1;
    /* Above the game loop's priority, so audio keeps up when a frame is slow. */
    SceUID thread = sceKernelCreateThread("rfvp_audio", rfvp_psv_audio_thread, 0x10000100 - 10, 0x10000,
                                          0, 0, NULL);
    if (thread < 0) {
        g_rfvp_audio_run = 0;
        return;
    }
    g_rfvp_audio_thread = thread;
    sceKernelStartThread(thread, 0, NULL);
}

void rfvp_psv_audio_thread_stop(void) {
    if (g_rfvp_audio_thread < 0) {
        return;
    }
    g_rfvp_audio_run = 0;
    sceKernelWaitThreadEnd(g_rfvp_audio_thread, NULL, NULL);
    g_rfvp_audio_thread = -1;
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

    /* Keys: the shoulder buttons hold Ctrl (skip text), the face buttons are Return, Space and
     * Escape. Pointer clicks stay on cross (left) and circle (right). */
    static const struct {
        uint32_t button;
        uint32_t key;
    } key_map[] = {
        {SCE_CTRL_LTRIGGER, 0u},
        {SCE_CTRL_RTRIGGER, 0u},
        {SCE_CTRL_SQUARE, 1u},
        {SCE_CTRL_SELECT, 2u},
        {SCE_CTRL_TRIANGLE, 3u},
    };
    for (size_t i = 0u; i < sizeof(key_map) / sizeof(key_map[0]); ++i) {
        int32_t key_status = RFVP_PSV_OK;
        if ((down & key_map[i].button) != 0u) {
            key_status = rfvp_psv_app_push_key(app, key_map[i].key, 1);
        } else if ((up & key_map[i].button) != 0u) {
            key_status = rfvp_psv_app_push_key(app, key_map[i].key, 0);
        }
        if (key_status != RFVP_PSV_OK) {
            return key_status;
        }
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

    if (rfvp_psv_gpu_init() != 0) {
        sceKernelExitProcess(-1);
    }
    g_rfvp_psv_vitasdk.display_ready = 1;
}

void rfvp_psv_vitasdk_fini(void) {
    rfvp_psv_c_audio_shutdown();
    rfvp_psv_c_renderer_shutdown();

    sceTouchSetSamplingState(SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_STOP);
    rfvp_psv_gpu_fini();
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

/* Leave the process at once; used after a panic, when nothing else can be trusted to run. */
void rfvp_psv_vitasdk_abort(void) {
    sceKernelExitProcess(1);
    for (;;) {
    }
}

/* The local date and time: year, month, day, day of week (Sunday = 0), hour, minute.
 * Returns 0 on success. */
int32_t rfvp_psv_vitasdk_wall_time(uint16_t out[6]) {
    SceDateTime now;
    if (sceRtcGetCurrentClockLocalTime(&now) < 0) {
        return -1;
    }
    out[0] = (uint16_t)now.year;
    out[1] = (uint16_t)now.month;
    out[2] = (uint16_t)now.day;
    int dow = sceRtcGetDayOfWeek(now.year, now.month, now.day);
    out[3] = (uint16_t)(dow < 0 ? 0 : dow);
    out[4] = (uint16_t)now.hour;
    out[5] = (uint16_t)now.minute;
    return 0;
}
