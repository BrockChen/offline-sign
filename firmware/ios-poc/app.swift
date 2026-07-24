// iOS 12 UIKit PoC：调用 Rust esp-signer-core 的 escore_probe，把 BIP-84 首地址显示到屏幕。
// 无 SwiftUI / 无 async（iOS 12.2 兼容）。期望显示 bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu
import UIKit

final class RootVC: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .white

        var buf = [UInt8](repeating: 0, count: 128)
        let n = escore_probe(&buf, 128)
        let body: String
        if n > 0 {
            let addr = String(bytes: buf[0..<Int(n)], encoding: .utf8) ?? "<utf8?>"
            body = "Rust 密钥核心已在 iOS 运行 ✓\n\nBIP-84[0]:\n\(addr)"
        } else {
            body = "escore_probe 失败: \(n)"
        }

        let label = UILabel()
        label.numberOfLines = 0
        label.textAlignment = .center
        label.font = .systemFont(ofSize: 15)
        label.text = body
        label.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(label)
        NSLayoutConstraint.activate([
            label.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            label.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
            label.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -20),
        ])
    }
}

final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    func application(_ application: UIApplication,
                     didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        let w = UIWindow(frame: UIScreen.main.bounds)
        w.rootViewController = RootVC()
        w.makeKeyAndVisible()
        window = w
        return true
    }
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil,
                  NSStringFromClass(AppDelegate.self))
