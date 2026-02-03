import SwiftUI
import UIKit
import QuartzCore

// MARK: - Rust FFI (iOS host-mode)

@_silgen_name("rfvp_ios_create")
private func rfvp_ios_create(
    _ uiView: UnsafeMutableRawPointer,
    _ widthPx: UInt32,
    _ heightPx: UInt32,
    _ nativeScaleFactor: Double,
    _ gameRootUtf8: UnsafePointer<CChar>,
    _ nlsUtf8: UnsafePointer<CChar>
) -> UnsafeMutableRawPointer?

@_silgen_name("rfvp_ios_step")
private func rfvp_ios_step(_ handle: UnsafeMutableRawPointer?, _ dtMs: UInt32) -> Int32

@_silgen_name("rfvp_ios_resize")
private func rfvp_ios_resize(_ handle: UnsafeMutableRawPointer?, _ widthPx: UInt32, _ heightPx: UInt32) -> Void

@_silgen_name("rfvp_ios_destroy")
private func rfvp_ios_destroy(_ handle: UnsafeMutableRawPointer?) -> Void

@_silgen_name("rfvp_ios_touch")
private func rfvp_ios_touch(_ handle: UnsafeMutableRawPointer?, _ phase: Int32, _ xPoints: Double, _ yPoints: Double) -> Void

// MARK: - Metal-backed UIView for wgpu

final class RFVPMetalView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }

    // phase: 0 began, 1 moved, 2 ended, 3 cancelled
    var onTouch: ((Int32, Double, Double) -> Void)?
    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = true
        backgroundColor = .black
        isUserInteractionEnabled = true
        isMultipleTouchEnabled = false
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        isOpaque = true
        backgroundColor = .black
        isUserInteractionEnabled = true
        isMultipleTouchEnabled = false
    }

    func configureScale(_ scale: CGFloat) {
        contentScaleFactor = scale
        if let layer = self.layer as? CAMetalLayer {
            layer.contentsScale = scale
        }
    }

    private func send(_ phase: Int32, _ touches: Set<UITouch>) {
        guard let t = touches.first else { return }
        let p = t.location(in: self) // points
        onTouch?(phase, Double(p.x), Double(p.y))
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) { send(0, touches) }
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) { send(1, touches) }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) { send(2, touches) }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) { send(3, touches) }
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

    private var lastDrawableSizePx: (UInt32, UInt32) = (0, 0)
    private var lastScale: Double = 0.0

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

        metalView.onTouch = { [weak self] phase, x, y in
            guard let self = self else { return }
            guard let h = self.handle else { return }
            rfvp_ios_touch(h, phase, x, y)
        }
    }

    override func viewDidLayoutSubviews() {
        super.viewDidLayoutSubviews()

        let sizePoints = view.bounds.size
        if sizePoints.width <= 0 || sizePoints.height <= 0 { return }

        let scale = (view.window?.screen.nativeScale ?? UIScreen.main.nativeScale)
        metalView.configureScale(CGFloat(scale))

        let wPx = UInt32(max(1.0, (sizePoints.width * scale).rounded(.toNearestOrAwayFromZero)))
        let hPx = UInt32(max(1.0, (sizePoints.height * scale).rounded(.toNearestOrAwayFromZero)))

        if handle == nil {
            createEngineIfNeeded(wPx: wPx, hPx: hPx, scale: scale)
        } else {
            if wPx != lastDrawableSizePx.0 || hPx != lastDrawableSizePx.1 || scale != lastScale {
                lastDrawableSizePx = (wPx, hPx)
                lastScale = scale
                rfvp_ios_resize(handle, wPx, hPx)
            }
        }
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)

        if #available(iOS 16.0, *) {
            if let scene = view.window?.windowScene {
                scene.requestGeometryUpdate(.iOS(interfaceOrientations: .landscape))
            }
        }

        setNeedsStatusBarAppearanceUpdate()
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

    private func createEngineIfNeeded(wPx: UInt32, hPx: UInt32, scale: Double) {
        let viewPtr = UnsafeMutableRawPointer(Unmanaged.passUnretained(metalView).toOpaque())

        gameRoot.withCString { gameC in
            nls.withCString { nlsC in
                let hnd = rfvp_ios_create(viewPtr, wPx, hPx, scale, gameC, nlsC)
                self.handle = hnd
                self.lastDrawableSizePx = (wPx, hPx)
                self.lastScale = scale
            }
        }

        if handle == nil {
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

        let clamped = min(max(dtSec, 0.0), 0.2)
        let dtMs = UInt32((clamped * 1000.0).rounded(.toNearestOrAwayFromZero))

        let status = rfvp_ios_step(handle, dtMs)
        if status != 0 {
            onExit()
        }
    }

    // MARK: - Fullscreen / orientation (mobile semantics)

    override var prefersStatusBarHidden: Bool { true }
    override var prefersHomeIndicatorAutoHidden: Bool { true }

    override var supportedInterfaceOrientations: UIInterfaceOrientationMask { .landscape }
    override var preferredInterfaceOrientationForPresentation: UIInterfaceOrientation { .landscapeRight }
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
        .statusBarHidden(true)
    }
}
