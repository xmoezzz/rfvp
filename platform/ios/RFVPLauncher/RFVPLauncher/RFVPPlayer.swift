import SwiftUI
import UIKit
import QuartzCore

// MARK: - Rust FFI (iOS host-mode)

@_silgen_name("rfvp_ios_create")
private func rfvp_ios_create(
    _ uiView: UnsafeMutableRawPointer,
    _ widthPoints: UInt32,
    _ heightPoints: UInt32,
    _ nativeScaleFactor: Double,
    _ gameRootUtf8: UnsafePointer<CChar>,
    _ nlsUtf8: UnsafePointer<CChar>
) -> UnsafeMutableRawPointer?

@_silgen_name("rfvp_ios_step")
private func rfvp_ios_step(_ handle: UnsafeMutableRawPointer?, _ dtMs: UInt32) -> Int32

@_silgen_name("rfvp_ios_resize")
private func rfvp_ios_resize(_ handle: UnsafeMutableRawPointer?, _ widthPoints: UInt32, _ heightPoints: UInt32) -> Void

@_silgen_name("rfvp_ios_destroy")
private func rfvp_ios_destroy(_ handle: UnsafeMutableRawPointer?) -> Void

// MARK: - Metal-backed UIView for wgpu

final class RFVPMetalView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }

    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = true
        backgroundColor = .black
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        isOpaque = true
        backgroundColor = .black
    }

    func configureScale(_ scale: CGFloat) {
        contentScaleFactor = scale
        if let layer = self.layer as? CAMetalLayer {
            layer.contentsScale = scale
            // wgpu will set device/pixelFormat as needed.
        }
    }
}

// MARK: - UIViewController that owns the engine + CADisplayLink

final class RFVPPlayerViewController: UIViewController {
    private let gameRoot: String
    private let nls: String
    private let onExit: () -> Void

    private var metalView: RFVPMetalView { view as! RFVPMetalView }

    private var handle: UnsafeMutableRawPointer? = nil
    private var displayLink: CADisplayLink? = nil
    private var lastTimestamp: CFTimeInterval? = nil

    private var lastSizePoints: CGSize = .zero

    init(gameRoot: String, nls: String, onExit: @escaping () -> Void) {
        self.gameRoot = gameRoot
        self.nls = nls
        self.onExit = onExit
        super.init(nibName: nil, bundle: nil)
        modalPresentationStyle = .fullScreen
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = RFVPMetalView(frame: .zero)
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .black
    }

    override var prefersStatusBarHidden: Bool { true }
    override var prefersHomeIndicatorAutoHidden: Bool { true }
    override var supportedInterfaceOrientations: UIInterfaceOrientationMask { .landscape }
    override var preferredInterfaceOrientationForPresentation: UIInterfaceOrientation { .landscapeRight }
    override var shouldAutorotate: Bool { true }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()
        let size = view.bounds.size
        if size.width <= 0 || size.height <= 0 {
            return
        }

        // Scale: prefer the view's window if available.
        let scale = (view.window?.screen.scale ?? UIScreen.main.scale)
        metalView.configureScale(scale)

        if handle == nil {
            createEngineIfNeeded(sizePoints: size, scale: Double(scale))
        } else if size != lastSizePoints {
            lastSizePoints = size
            rfvp_ios_resize(handle, UInt32(size.width.rounded()), UInt32(size.height.rounded()))
        }
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        // Request landscape on iOS 16+. (Info.plist also restricts to landscape.)
        if #available(iOS 16.0, *) {
            if let scene = view.window?.windowScene {
                try? scene.requestGeometryUpdate(.iOS(interfaceOrientations: .landscape))
            }
        }

        startDisplayLink()
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        stopDisplayLink()
    }

    deinit {
        stopDisplayLink()
        if handle != nil {
            rfvp_ios_destroy(handle)
            handle = nil
        }
    }

    private func createEngineIfNeeded(sizePoints: CGSize, scale: Double) {
        let w = UInt32(max(1, sizePoints.width.rounded()))
        let h = UInt32(max(1, sizePoints.height.rounded()))

        let viewPtr = UnsafeMutableRawPointer(Unmanaged.passUnretained(metalView).toOpaque())

        gameRoot.withCString { gameC in
            nls.withCString { nlsC in
                let hnd = rfvp_ios_create(viewPtr, w, h, scale, gameC, nlsC)
                self.handle = hnd
                self.lastSizePoints = sizePoints
            }
        }

        if handle == nil {
            // Failed to create engine; exit immediately.
            onExit()
        }
    }

    private func startDisplayLink() {
        if displayLink != nil { return }
        let link = CADisplayLink(target: self, selector: #selector(onDisplayLink(_:)))
        link.add(to: .main, forMode: .common)
        displayLink = link
        lastTimestamp = nil
    }

    private func stopDisplayLink() {
        displayLink?.invalidate()
        displayLink = nil
        lastTimestamp = nil
    }

    @objc private func onDisplayLink(_ link: CADisplayLink) {
        guard let handle else { return }

        let now = link.timestamp
        let dtSec: Double
        if let last = lastTimestamp {
            dtSec = now - last
        } else {
            dtSec = link.duration
        }
        lastTimestamp = now

        // Clamp dt to avoid huge jumps after interruptions.
        let clamped = min(max(dtSec, 0.0), 0.2)
        let dtMs = UInt32((clamped * 1000.0).rounded())

        let status = rfvp_ios_step(handle, dtMs)
        if status != 0 {
            onExit()
        }
    }
}

// MARK: - SwiftUI bridge

struct RFVPPlayerContainer: UIViewControllerRepresentable {
    let gameRoot: String
    let nls: String
    let onExit: () -> Void

    func makeUIViewController(context: Context) -> RFVPPlayerViewController {
        RFVPPlayerViewController(gameRoot: gameRoot, nls: nls, onExit: onExit)
    }

    func updateUIViewController(_ uiViewController: RFVPPlayerViewController, context: Context) {
        // No-op
    }
}

struct RFVPPlayerScreen: View {
    @EnvironmentObject var library: GameLibrary
    let game: GameEntry

    var body: some View {
        RFVPPlayerContainer(
            gameRoot: game.rootPath,
            nls: GameEntry.normalizeNls(game.nls),
            onExit: {
                DispatchQueue.main.async {
                    library.activeGame = nil
                }
            }
        )
        .ignoresSafeArea()
        .statusBar(hidden: true)
    }
}
