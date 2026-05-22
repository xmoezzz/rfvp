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

@_silgen_name("rfvp_ios_mouse_button")
private func rfvp_ios_mouse_button(
    _ handle: UnsafeMutableRawPointer?,
    _ button: Int32,
    _ phase: Int32,
    _ xPoints: Double,
    _ yPoints: Double
) -> Void

@_silgen_name("rfvp_ios_mouse_wheel")
private func rfvp_ios_mouse_wheel(
    _ handle: UnsafeMutableRawPointer?,
    _ delta: Int32,
    _ xPoints: Double,
    _ yPoints: Double
) -> Void

// MARK: - Metal-backed UIView for wgpu

final class RFVPMetalView: UIView {
    override class var layerClass: AnyClass { CAMetalLayer.self }

    private enum MouseButton {
        static let right: Int32 = 1
    }

    private let wheelStepPoints: CGFloat = 40.0

    // phase: 0 began/down, 1 moved, 2 ended/up, 3 cancelled/up
    var onTouch: ((Int32, Double, Double) -> Void)?
    var onMouseButton: ((Int32, Int32, Double, Double) -> Void)?
    var onMouseWheel: ((Int32, Double, Double) -> Void)?

    private var activeSingleTouch: UITouch?
    private var wheelRemainderPoints: CGFloat = 0.0

    override init(frame: CGRect) {
        super.init(frame: frame)
        configureView()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        configureView()
    }

    private func configureView() {
        isOpaque = true
        backgroundColor = .black
        isUserInteractionEnabled = true
        isMultipleTouchEnabled = true
        installGestureRecognizers()
    }

    private func installGestureRecognizers() {
        let twoFingerTap = UITapGestureRecognizer(target: self, action: #selector(handleTwoFingerTap(_:)))
        twoFingerTap.numberOfTapsRequired = 1
        twoFingerTap.numberOfTouchesRequired = 2
        twoFingerTap.cancelsTouchesInView = true
        addGestureRecognizer(twoFingerTap)

        let twoFingerPan = UIPanGestureRecognizer(target: self, action: #selector(handleTwoFingerPan(_:)))
        twoFingerPan.minimumNumberOfTouches = 2
        twoFingerPan.maximumNumberOfTouches = 2
        twoFingerPan.cancelsTouchesInView = true
        addGestureRecognizer(twoFingerPan)
    }

    func configureScale(_ scale: CGFloat) {
        contentScaleFactor = scale
        if let layer = self.layer as? CAMetalLayer {
            layer.contentsScale = scale
        }
    }

    private func activeTouchCount(_ event: UIEvent?, fallback touches: Set<UITouch>) -> Int {
        event?.allTouches?.count ?? touches.count
    }

    private func send(_ phase: Int32, _ touch: UITouch) {
        let p = touch.location(in: self) // points
        onTouch?(phase, Double(p.x), Double(p.y))
    }

    private func cancelActiveSingleTouch() {
        guard let activeSingleTouch else { return }
        send(3, activeSingleTouch)
        self.activeSingleTouch = nil
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        let count = activeTouchCount(event, fallback: touches)
        if count == 1, activeSingleTouch == nil, let t = touches.first {
            activeSingleTouch = t
            send(0, t)
        } else if activeSingleTouch != nil {
            cancelActiveSingleTouch()
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let activeSingleTouch else { return }
        if activeTouchCount(event, fallback: touches) != 1 {
            cancelActiveSingleTouch()
            return
        }
        if touches.contains(where: { $0 === activeSingleTouch }) {
            send(1, activeSingleTouch)
        }
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let activeSingleTouch else { return }
        if touches.contains(where: { $0 === activeSingleTouch }) {
            send(2, activeSingleTouch)
            self.activeSingleTouch = nil
        }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let activeSingleTouch else { return }
        if touches.contains(where: { $0 === activeSingleTouch }) {
            send(3, activeSingleTouch)
            self.activeSingleTouch = nil
        }
    }

    @objc private func handleTwoFingerTap(_ recognizer: UITapGestureRecognizer) {
        guard recognizer.state == .ended else { return }
        cancelActiveSingleTouch()
        let p = recognizer.location(in: self)
        onMouseButton?(MouseButton.right, 0, Double(p.x), Double(p.y))
        onMouseButton?(MouseButton.right, 2, Double(p.x), Double(p.y))
    }

    @objc private func handleTwoFingerPan(_ recognizer: UIPanGestureRecognizer) {
        switch recognizer.state {
        case .began:
            cancelActiveSingleTouch()
            wheelRemainderPoints = 0.0
            recognizer.setTranslation(.zero, in: self)
        case .changed:
            let translation = recognizer.translation(in: self)
            recognizer.setTranslation(.zero, in: self)

            if abs(translation.y) < abs(translation.x) {
                return
            }

            wheelRemainderPoints += translation.y
            let steps = Int(wheelRemainderPoints / wheelStepPoints)
            if steps == 0 { return }

            wheelRemainderPoints -= CGFloat(steps) * wheelStepPoints
            let p = recognizer.location(in: self)
            onMouseWheel?(Int32(-steps), Double(p.x), Double(p.y))
        case .ended, .cancelled, .failed:
            wheelRemainderPoints = 0.0
        default:
            break
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

        metalView.onMouseButton = { [weak self] button, phase, x, y in
            guard let self = self else { return }
            guard let h = self.handle else { return }
            rfvp_ios_mouse_button(h, button, phase, x, y)
        }

        metalView.onMouseWheel = { [weak self] delta, x, y in
            guard let self = self else { return }
            guard let h = self.handle else { return }
            rfvp_ios_mouse_wheel(h, delta, x, y)
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
