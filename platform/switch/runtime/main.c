/* The Switch runtime: libnx owns the process, input, audio and the display; the rfvp engine
 * (Rust, linked in as librfvp_horizon.a) runs one frame per call and draws it into the
 * framebuffer handed to it. */

#include <dirent.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>

#include <switch.h>

#define SCREEN_WIDTH 1280
#define SCREEN_HEIGHT 720

static const char* const GameRoot = "sdmc:/switch/rfvp/game";

typedef struct {
    uint64_t buttons;
    int32_t stick_x;
    int32_t stick_y;
    uint32_t touch_count;
    uint32_t touch_id;
    int32_t touch_x;
    int32_t touch_y;
} RfvpSwitchInput;

typedef struct {
    uint16_t year;
    uint8_t month;
    uint8_t day;
    uint8_t day_of_week;
    uint8_t hour;
    uint8_t minute;
} RfvpSwitchWallTime;

/* The engine (platform/switch/host/src/ffi.rs). */
extern void* rfvp_switch_start(const char* game_root);
extern int rfvp_switch_frame(void* app, const RfvpSwitchInput* input, uint32_t* framebuffer,
                             uint32_t stride_bytes);
extern void rfvp_switch_audio_render(void* app, int16_t* dst, size_t frames);
extern void rfvp_switch_stop(void* app);

/* ---- Services for the engine ---------------------------------------------------------------- */

void rfvp_switch_log(const char* message, size_t len) {
    /* An emulator shows these in its log; on a console they go to the debug output. */
    svcOutputDebugString(message, len);
}

uint64_t rfvp_switch_ticks_us(void) {
    return armTicksToNs(armGetSystemTick()) / 1000;
}

bool rfvp_switch_wall_time(RfvpSwitchWallTime* out) {
    time_t now = time(NULL);
    struct tm* local = localtime(&now);
    if (local == NULL) return false;
    out->year = (uint16_t) (local->tm_year + 1900);
    out->month = (uint8_t) (local->tm_mon + 1);
    out->day = (uint8_t) local->tm_mday;
    out->day_of_week = (uint8_t) local->tm_wday;
    out->hour = (uint8_t) local->tm_hour;
    out->minute = (uint8_t) local->tm_min;
    return true;
}

typedef void (*RfvpDirVisitor)(void* ctx, const char* name, bool is_dir, uint64_t size);

/* List a directory; Rust does not know newlib's dirent and stat layouts. */
int rfvp_switch_list_dir(const char* path, RfvpDirVisitor visit, void* ctx) {
    DIR* dir = opendir(path);
    if (dir == NULL) return -1;
    char child[FS_MAX_PATH];
    struct dirent* entry;
    while ((entry = readdir(dir)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) continue;
        snprintf(child, sizeof(child), "%s/%s", path, entry->d_name);
        struct stat st;
        if (stat(child, &st) != 0) continue;
        visit(ctx, entry->d_name, S_ISDIR(st.st_mode), (uint64_t) st.st_size);
    }
    closedir(dir);
    return 0;
}

/* 1 for a file, 2 for a directory, 0 if nothing is there. */
int rfvp_switch_path_kind(const char* path, uint64_t* size) {
    struct stat st;
    if (stat(path, &st) != 0) return 0;
    if (size != NULL) *size = (uint64_t) st.st_size;
    return S_ISDIR(st.st_mode) ? 2 : 1;
}

/* ---- Audio ---------------------------------------------------------------------------------- */

enum {
    /* One buffer is one page of 16-bit stereo (about 21 ms); six of them ride out stalls of
     * over 100 ms without running dry. */
    AudioBufferFrames = 1024,
    AudioBufferCount = 6,
};
static AudioDriver audio_driver;
static AudioDriverWaveBuf audio_wavebufs[AudioBufferCount];
static int16_t audio_buffers[AudioBufferCount][AudioBufferFrames * 2]
    __attribute__((aligned(0x1000)));
static bool audio_renderer_ready;
static bool audio_driver_ready;

static void log_result(const char* what, Result rc) {
    char message[96];
    int len = snprintf(message, sizeof(message), "rfvp: %s failed: 0x%08x\n", what,
                       (unsigned) rc);
    rfvp_switch_log(message, (size_t) len);
}

static void audio_init(void) {
    static const AudioRendererConfig config = {
        .output_rate = AudioRendererOutputRate_48kHz,
        .num_voices = 2,
        .num_effects = 0,
        .num_sinks = 1,
        .num_mix_objs = 1,
        .num_mix_buffers = 2,
    };
    Result rc = audrenInitialize(&config);
    if (R_FAILED(rc)) { log_result("audrenInitialize", rc); return; }
    audio_renderer_ready = true;
    rc = audrvCreate(&audio_driver, &config, 2);
    if (R_FAILED(rc)) { log_result("audrvCreate", rc); return; }
    audio_driver_ready = true;

    memset(audio_buffers, 0, sizeof(audio_buffers));
    armDCacheFlush(audio_buffers, sizeof(audio_buffers));
    int pool = audrvMemPoolAdd(&audio_driver, audio_buffers, sizeof(audio_buffers));
    if (pool < 0 || !audrvMemPoolAttach(&audio_driver, pool)) {
        rfvp_switch_log("rfvp: audio memory pool failed\n", 31);
        audio_driver_ready = false;
        return;
    }
    static const u8 sink_channels[] = { 0, 1 };
    audrvDeviceSinkAdd(&audio_driver, AUDREN_DEFAULT_DEVICE_NAME, 2, sink_channels);
    /* The pool and the sink have to be committed before a voice can use them. */
    rc = audrvUpdate(&audio_driver);
    if (R_FAILED(rc)) { log_result("audrvUpdate", rc); audio_driver_ready = false; return; }
    rc = audrenStartAudioRenderer();
    if (R_FAILED(rc)) { log_result("audrenStartAudioRenderer", rc); audio_driver_ready = false; return; }
    if (!audrvVoiceInit(&audio_driver, 0, 2, PcmFormat_Int16, 48000)) {
        rfvp_switch_log("rfvp: audrvVoiceInit failed\n", 28);
        audio_driver_ready = false;
        return;
    }
    audrvVoiceSetDestinationMix(&audio_driver, 0, AUDREN_FINAL_MIX_ID);
    audrvVoiceSetMixFactor(&audio_driver, 0, 1.0f, 0, 0);
    audrvVoiceSetMixFactor(&audio_driver, 0, 1.0f, 1, 1);
    audrvVoiceStart(&audio_driver, 0);
    audrvUpdate(&audio_driver);
}

/* Refill every buffer the device has finished with. */
static void audio_pump(void* app) {
    if (!audio_driver_ready) return;
    for (unsigned i = 0; i < AudioBufferCount; ++i) {
        AudioDriverWaveBuf* wavebuf = &audio_wavebufs[i];
        if (wavebuf->state != AudioDriverWaveBufState_Free &&
            wavebuf->state != AudioDriverWaveBufState_Done) continue;
        rfvp_switch_audio_render(app, audio_buffers[i], AudioBufferFrames);
        armDCacheFlush(audio_buffers[i], sizeof(audio_buffers[i]));
        wavebuf->data_raw = audio_buffers[i];
        wavebuf->size = sizeof(audio_buffers[i]);
        wavebuf->start_sample_offset = 0;
        wavebuf->end_sample_offset = AudioBufferFrames;
        wavebuf->is_looping = false;
        audrvVoiceAddWaveBuf(&audio_driver, 0, wavebuf);
    }
    audrvUpdate(&audio_driver);
}

static void audio_exit(void) {
    if (audio_driver_ready) audrvClose(&audio_driver);
    if (audio_renderer_ready) audrenExit();
}

/* ---- Main loop ------------------------------------------------------------------------------ */

/* Optional framebuffer capture, using the same debug.txt convention as other ports. */
static unsigned screenshot_frame(void) {
    FILE* file = fopen("sdmc:/switch/rfvp/debug.txt", "r");
    if (file == NULL) return UINT32_MAX;
    char line[128];
    unsigned frame = UINT32_MAX;
    while (fgets(line, sizeof(line), file)) {
        unsigned value;
        if (sscanf(line, "shot=%u", &value) == 1) frame = value;
    }
    fclose(file);
    return frame;
}

static void save_screenshot(const uint8_t* pixels, uint32_t stride) {
    FILE* file = fopen("sdmc:/switch/rfvp/debug_screen.bmp", "wb");
    if (file == NULL) return;
    uint8_t header[54] = { 'B', 'M' };
    const uint32_t fields[][2] = {
        { 2, 54 + SCREEN_WIDTH * SCREEN_HEIGHT * 4 }, { 10, 54 }, { 14, 40 },
        { 18, SCREEN_WIDTH }, { 22, SCREEN_HEIGHT },
    };
    for (unsigned field = 0; field < sizeof(fields) / sizeof(fields[0]); field++) {
        for (unsigned byte = 0; byte < 4; byte++)
            header[fields[field][0] + byte] = (uint8_t) (fields[field][1] >> (byte * 8));
    }
    header[26] = 1;
    header[28] = 32;
    fwrite(header, 1, sizeof(header), file);
    uint8_t row[SCREEN_WIDTH * 4];
    for (int y = SCREEN_HEIGHT - 1; y >= 0; y--) {
        const uint8_t* source = pixels + (size_t) y * stride;
        for (unsigned x = 0; x < SCREEN_WIDTH; x++) {
            row[x * 4] = source[x * 4 + 2];
            row[x * 4 + 1] = source[x * 4 + 1];
            row[x * 4 + 2] = source[x * 4];
            row[x * 4 + 3] = 255;
        }
        fwrite(row, 1, sizeof(row), file);
    }
    fclose(file);
}

int main(void) {
    rfvp_switch_log("rfvp: starting\n", 15);

    Framebuffer fb;
    /* Double buffered; presenting waits for vsync, which paces the loop at 60 fps. */
    Result rc = framebufferCreate(&fb, nwindowGetDefault(), SCREEN_WIDTH, SCREEN_HEIGHT,
                                  PIXEL_FORMAT_RGBA_8888, 2);
    if (R_FAILED(rc)) { log_result("framebufferCreate", rc); return 1; }
    /* The engine writes rows of RGBA; libnx swizzles them to the GPU layout on present. */
    rc = framebufferMakeLinear(&fb);
    if (R_FAILED(rc)) { log_result("framebufferMakeLinear", rc); framebufferClose(&fb); return 1; }

    audio_init();

    padConfigureInput(1, HidNpadStyleSet_NpadStandard);
    PadState pad;
    padInitializeDefault(&pad);
    hidInitializeTouchScreen();

    void* app = rfvp_switch_start(GameRoot);
    if (app == NULL) {
        /* The reason is in sdmc:/switch/rfvp/last_error.txt. */
        audio_exit();
        framebufferClose(&fb);
        return 1;
    }

    const unsigned shot = screenshot_frame();
    unsigned frame = 0;
    while (appletMainLoop()) {
        padUpdate(&pad);
        RfvpSwitchInput input = { 0 };
        input.buttons = padGetButtons(&pad);
        HidAnalogStickState stick = padGetStickPos(&pad, 0);
        input.stick_x = stick.x;
        input.stick_y = stick.y;
        HidTouchScreenState touch = { 0 };
        if (hidGetTouchScreenStates(&touch, 1) && touch.count > 0) {
            input.touch_count = (uint32_t) touch.count;
            input.touch_id = touch.touches[0].finger_id;
            input.touch_x = (int32_t) touch.touches[0].x;
            input.touch_y = (int32_t) touch.touches[0].y;
        }

        u32 stride = 0;
        uint32_t* pixels = (uint32_t*) framebufferBegin(&fb, &stride);
        int status = rfvp_switch_frame(app, &input, pixels, stride);
        if (frame++ == shot) save_screenshot((const uint8_t*) pixels, stride);
        framebufferEnd(&fb);
        audio_pump(app);
        if (status != 0) break;
    }

    rfvp_switch_stop(app);
    audio_exit();
    framebufferClose(&fb);
    return 0;
}
