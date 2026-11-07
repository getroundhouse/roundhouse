import UIKit

// The containment pattern follows gpui-pre-mobile's example/ios/App.swift.
//
// Set POC_HOST=native to prove UIKit and GPUI can share a screen: a native navigation bar and a
// native text field surround the GPUI view. Without it, GPUI fills the safe area, which matches
// the screen used on path A.

@main
final class AppDelegate: UIResponder, UIApplicationDelegate {
    func application(_ application: UIApplication,
                     didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        true
    }
}

/// UIKit traps at launch if an app built with the iOS 27 SDK has no scene lifecycle. The fork's
/// example still uses the older window-in-app-delegate shape, so this host differs from it here.
final class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
        guard let windowScene = scene as? UIWindowScene else { return }
        let native = ProcessInfo.processInfo.environment["POC_HOST"] == "native"
        let content = HostController(nativeChrome: native)
        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = native ? UINavigationController(rootViewController: content) : content
        window.overrideUserInterfaceStyle = .dark
        window.makeKeyAndVisible()
        self.window = window
    }

    func sceneDidBecomeActive(_ scene: UIScene) {
        gpui_ios_did_become_active(nil)
    }

    func sceneWillResignActive(_ scene: UIScene) {
        gpui_ios_will_resign_active(nil)
    }
}

/// UIKit owns this view's frame. GPUI owns only what is drawn inside it.
final class GPUIHostView: UIView {
    private let gpuiWindow: UnsafeMutableRawPointer
    let contentController: UIViewController

    override init(frame: CGRect) {
        gpui_ios_set_embedded()
        poc_start()
        guard let window = gpui_ios_get_window(),
              let controller = gpui_ios_view_controller(window) else {
            fatalError("Could not create the GPUI view")
        }
        gpuiWindow = window
        contentController = Unmanaged<UIViewController>.fromOpaque(controller).takeUnretainedValue()
        super.init(frame: frame)
        clipsToBounds = true
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }

    func attach(to parent: UIViewController) {
        parent.addChild(contentController)
        addSubview(contentController.view)
        contentController.didMove(toParent: parent)
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        guard bounds.width > 0, bounds.height > 0,
              contentController.view.frame != bounds else { return }
        // Publish the new geometry and its content together, or Core Animation can stretch the
        // previous drawable until the next tick.
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        contentController.view.frame = bounds
        gpui_ios_layout_view(gpuiWindow)
        drawFrame()
        CATransaction.commit()
    }

    func drawFrame() { _ = gpui_ios_request_frame(gpuiWindow) }
}

final class HostController: UIViewController {
    private let nativeChrome: Bool
    private var gpui: GPUIHostView!
    private var displayLink: CADisplayLink?

    init(nativeChrome: Bool) {
        self.nativeChrome = nativeChrome
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) { fatalError("init(coder:) is unsupported") }

    override var preferredStatusBarStyle: UIStatusBarStyle { .lightContent }

    override func viewDidLoad() {
        super.viewDidLoad()
        overrideUserInterfaceStyle = .dark
        view.backgroundColor = UIColor(red: 0.04, green: 0.04, blue: 0.04, alpha: 1)
        title = "Native UIKit title"

        gpui = GPUIHostView(frame: .zero)
        gpui.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(gpui)
        gpui.attach(to: self)

        var constraints = [
            gpui.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            gpui.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            gpui.trailingAnchor.constraint(equalTo: view.trailingAnchor),
        ]
        if nativeChrome {
            let field = UITextField()
            field.borderStyle = .roundedRect
            field.placeholder = "Native UITextField"
            field.translatesAutoresizingMaskIntoConstraints = false
            view.addSubview(field)
            constraints += [
                field.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 12),
                field.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -12),
                field.bottomAnchor.constraint(equalTo: view.keyboardLayoutGuide.topAnchor, constant: -8),
                gpui.bottomAnchor.constraint(equalTo: field.topAnchor, constant: -8),
            ]
        } else {
            constraints.append(gpui.bottomAnchor.constraint(equalTo: view.keyboardLayoutGuide.topAnchor))
        }
        NSLayoutConstraint.activate(constraints)
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        displayLink = CADisplayLink(target: self, selector: #selector(renderFrame))
        displayLink?.add(to: .main, forMode: .common)
    }

    override func viewWillDisappear(_ animated: Bool) {
        displayLink?.invalidate()
        displayLink = nil
        super.viewWillDisappear(animated)
    }

    @objc private func renderFrame() { gpui.drawFrame() }
}
