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

void rfvp_ios_destroy(void* handle);

#ifdef __cplusplus
} // extern "C"
#endif
