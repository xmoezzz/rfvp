#pragma once

#ifdef __cplusplus
extern "C" {
#endif

// Unified entry point across macOS/iOS/Android.
// Arguments are UTF-8 strings.
void rfvp_run_entry(const char* game_root_utf8, const char* nls_utf8);

// Pump-mode entry points (for embedding into an existing UI runloop).
// Returns an opaque handle, or NULL on failure.
void* rfvp_pump_create(const char* game_root_utf8, const char* nls_utf8);

// Steps the pump. timeout_ms should be 0 for non-blocking usage on the UI thread.
// Returns 0 to continue, non-zero to stop (exited / fatal error).
int rfvp_pump_step(void* handle, unsigned int timeout_ms);

// Destroys the pump handle.
void rfvp_pump_destroy(void* handle);

#ifdef __cplusplus
} // extern "C"
#endif
