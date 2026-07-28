import Cocoa
import Darwin
import Foundation
import SwiftUI

private func redirectStdoutStderrToLogFile() {
    // When launched from Finder, stdout/stderr often isn't visible.
    // Mirror logs to ~/Library/Logs/RFVPLauncher/console.log for debugging.
    let fm = FileManager.default
    guard let library = fm.urls(for: .libraryDirectory, in: .userDomainMask).first else { return }
    let dir = library.appendingPathComponent("Logs/RFVPLauncher", isDirectory: true)
    try? fm.createDirectory(at: dir, withIntermediateDirectories: true)

    let logFile = dir.appendingPathComponent("console.log")
    logFile.path.withCString { path in
        _ = freopen(path, "a+", stdout)
        _ = freopen(path, "a+", stderr)
    }

    setvbuf(stdout, nil, _IONBF, 0)
    setvbuf(stderr, nil, _IONBF, 0)
    print("\n---- RFVPLauncher start: \(Date()) ----")
}

private typealias RfvpRunEntryFn = @convention(c) (
    UnsafePointer<CChar>,
    UnsafePointer<CChar>
) -> Int32
private typealias RfvpSetTextHidpiEnabledFn = @convention(c) (Int32) -> Void

/// Loads the Rust engine from the dylib embedded in this app bundle.
///
/// The launcher intentionally does not link against `librfvp.dylib` at build time.
/// Loading the exact bundle path avoids Xcode library-search ambiguity and allows the
/// packaging script to validate both Universal Binary slices before the app is built.
private final class RfvpDynamicLibrary {
    static let shared = RfvpDynamicLibrary()

    private let handle: UnsafeMutableRawPointer?

    private init() {
        guard let frameworksPath = Bundle.main.privateFrameworksPath else {
            print("[launcher] error: app bundle has no Frameworks directory")
            self.handle = nil
            return
        }

        let dylibPath = URL(fileURLWithPath: frameworksPath)
            .appendingPathComponent("librfvp.dylib", isDirectory: false)
            .path

        dlerror()
        self.handle = dylibPath.withCString { path in
            dlopen(path, RTLD_NOW | RTLD_LOCAL)
        }

        if self.handle == nil {
            let detail = dlerror().map { String(cString: $0) } ?? "unknown dynamic loader error"
            print("[launcher] error: failed to load \(dylibPath): \(detail)")
        }
    }

    private func symbol(named name: String) -> UnsafeMutableRawPointer? {
        guard let handle else {
            return nil
        }

        dlerror()
        let symbol = name.withCString { dlsym(handle, $0) }
        if symbol == nil {
            let detail = dlerror().map { String(cString: $0) } ?? "symbol not found"
            print("[launcher] error: \(name) unavailable: \(detail)")
        }
        return symbol
    }

    func runEntry() -> RfvpRunEntryFn? {
        guard let symbol = symbol(named: "rfvp_run_entry") else {
            return nil
        }
        return unsafeBitCast(symbol, to: RfvpRunEntryFn.self)
    }

    func setTextHidpiEnabled(_ enabled: Bool) {
        guard let symbol = symbol(named: "rfvp_set_text_hidpi_enabled") else {
            return
        }
        let setter = unsafeBitCast(symbol, to: RfvpSetTextHidpiEnabledFn.self)
        setter(enabled ? 1 : 0)
    }
}

final class LauncherWindowDelegate: NSObject, NSWindowDelegate {
    private let onClose: () -> Void

    init(onClose: @escaping () -> Void) {
        self.onClose = onClose
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        onClose()
        return true
    }
}

@MainActor
final class LauncherHost {
    let library: GameLibrary

    private(set) var shouldQuit: Bool = false
    private var selected: GameEntry? = nil

    private let window: NSWindow
	// Must be a stored property to keep the delegate alive.
	// Optional with a default value so `self` can be captured safely later in `init`.
	private var windowDelegate: LauncherWindowDelegate? = nil

    init() {
        self.library = GameLibrary()

        let rootView = ContentView().environmentObject(library)
        let hosting = NSHostingView(rootView: rootView)

        self.window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 980, height: 620),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        self.window.title = "RFVP"
        self.window.isReleasedWhenClosed = false
        self.window.center()
        self.window.contentView = hosting

		self.windowDelegate = LauncherWindowDelegate { [weak self] in
            guard let self else { return }
            Task { @MainActor in
                self.shouldQuit = true
            }
        }
		self.window.delegate = self.windowDelegate

        self.library.onLaunchRequest = { [weak self] game in
            guard let self else { return }
            Task { @MainActor in
                self.selected = game
            }
        }
    }

    /// Selection stage event-loop integration.
    ///
    /// Important: we intentionally **avoid** calling `NSApp.run()` / `NSApp.runModal()` / `NSApp.finishLaunching()` here.
    /// Winit expects to own the AppKit lifecycle when `rfvp_run_entry()` starts; pre-launching the app via AppKit
    /// can prevent winit from receiving its expected launch notifications.
    func runPumpSelection() -> GameEntry? {
        selected = nil
        shouldQuit = false

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)

        while selected == nil && !shouldQuit {
            autoreleasepool {
                // Wait briefly for the next event.
                let until = Date(timeIntervalSinceNow: 1.0 / 60.0)
                if let event = NSApp.nextEvent(matching: .any,
                                              until: until,
                                              inMode: .default,
                                              dequeue: true) {
                    NSApp.sendEvent(event)
                }
                NSApp.updateWindows()
            }
        }

        window.orderOut(nil)
        return selected
    }

    func setTextHidpiEnabled(_ enabled: Bool) {
        RfvpDynamicLibrary.shared.setTextHidpiEnabled(enabled)
    }

    func runGame(_ game: GameEntry) -> Int32 {
        guard let runEntry = RfvpDynamicLibrary.shared.runEntry() else {
            return 126
        }

        // Avoid manual allocation/free here; the strings are only needed for the duration of the call.
        return game.rootPath.withCString { gameC in
            game.nls.withCString { nlsC in
                runEntry(gameC, nlsC)
            }
        }
    }
}

@main
@MainActor
struct RFVPLauncherMain {
    static func main() {
        let app = NSApplication.shared
        app.setActivationPolicy(.regular)

        redirectStdoutStderrToLogFile()
        // Do NOT call `finishLaunching()` here.
        // The Rust side (winit) expects to own the AppKit launch sequence.

        let host = LauncherHost()

        if host.shouldQuit {
            app.terminate(nil)
            return
        }

        guard let game = host.runPumpSelection(), !host.shouldQuit else {
            app.terminate(nil)
            return
        }

        let textHidpiEnabled = UserDefaults.standard.object(forKey: "rfvp.textHidpiEnabled") as? Bool ?? true
        host.setTextHidpiEnabled(textHidpiEnabled)

        print("[launcher] -> rfvp_run_entry(game_root=\(game.rootPath), nls=\(game.nls), text_hidpi=\(textHidpiEnabled)); NSApp.isRunning=\(NSApp.isRunning)")
        let rc = host.runGame(game)
        print("[launcher] <- rfvp_run_entry returned \(rc); exiting process")

        // IMPORTANT:
        // We do NOT return to the launcher UI after a game finishes.
        // Returning to AppKit/SwiftUI after winit has owned the macOS app lifecycle is fragile and
        // can crash during teardown (double-runloop / invalid NSApp state / release ordering).
        // Exit the process immediately after the game returns.
        fflush(stdout)
        fflush(stderr)
        _exit(rc)
    }
}
