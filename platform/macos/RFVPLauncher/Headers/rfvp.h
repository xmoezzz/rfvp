#pragma once

#ifdef __cplusplus
extern "C" {
#endif

// Unified entry point across macOS/iOS/Android.
// Arguments are UTF-8 strings.
void rfvp_run_entry(const char* game_root_utf8, const char* nls_utf8);

#ifdef __cplusplus
} // extern "C"
#endif
