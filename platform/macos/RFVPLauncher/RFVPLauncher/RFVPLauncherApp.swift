import SwiftUI

// NOTE: This file used to provide the SwiftUI @main entry point.
// The launcher now uses a custom main.swift so we can stop the launcher UI
// and then call into rfvp (winit) on the main thread.

struct RFVPLauncherPreviewRoot: View {
    @StateObject private var library = GameLibrary()

    var body: some View {
        ContentView()
            .environmentObject(library)
    }
}
