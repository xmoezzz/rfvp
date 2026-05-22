#pragma once

#ifdef __cplusplus
extern "C" {
#endif

// Legacy entry point (winit runloop). Do NOT use on iOS embedded/SwiftUI.
void rfvp_run_entry(const char* game_root_utf8, const char* nls_utf8);

// iOS host-mode entry points (SwiftUI/UIKit drives the runloop).
// `ui_view` must be a UIView* whose backing layer is CAMetalLayer.
void* rfvp_ios_create(
    void* ui_view,
    unsigned int width_px,
    unsigned int height_px,
    double native_scale_factor,
    const char* game_root_utf8,
    const char* nls_utf8
);

// Return 1 => exit requested, 0 => continue.
int rfvp_ios_step(void* handle, unsigned int dt_ms);

void rfvp_ios_resize(void* handle, unsigned int width_px, unsigned int height_px);

// phase: 0 began, 1 moved, 2 ended, 3 cancelled
void rfvp_ios_touch(void* handle, int phase, double x_points, double y_points);

// button: 0 left, 1 right. phase: 0 down, 1 move, 2 up, 3 cancelled/up.
void rfvp_ios_mouse_button(void* handle, int button, int phase, double x_points, double y_points);

// Wheel delta is forwarded to the engine wheel accumulator.
void rfvp_ios_mouse_wheel(void* handle, int delta, double x_points, double y_points);

// key: 0 Escape. phase: 0 down, 2 up, 3 cancelled/up.
void rfvp_ios_key(void* handle, int key, int phase);

void rfvp_ios_destroy(void* handle);

#ifdef __cplusplus
} // extern "C"
#endif
