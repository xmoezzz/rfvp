import Foundation

// RFVP (Rust) entry point exported from the static library / XCFramework.
// This function is expected to start the Winit event loop and not return.
@_silgen_name("start_winit_app")
func start_winit_app()

start_winit_app()
