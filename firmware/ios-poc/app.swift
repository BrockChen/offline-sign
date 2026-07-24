// btc-wallate iOS 离线签名机（UIKit，iOS 12 兼容：无 SwiftUI / 无 async）。
// 流程：导入助记词 → 解锁+概览 → 扫码/粘贴未签名 → 屏幕核对 → Touch ID → 签名 → 二维码。
// 所有密码学在 Rust(esp-signer-core) 里；私钥/种子只在 Rust 内瞬时存在。
import UIKit
import AVFoundation
import LocalAuthentication

// MARK: - Rust FFI 封装
enum Core {
    private static func buf(_ f: (UnsafeMutablePointer<UInt8>, Int) -> Int32) -> (ok: Bool, data: Data) {
        var b = [UInt8](repeating: 0, count: 16384)
        let n = b.withUnsafeMutableBufferPointer { f($0.baseAddress!, $0.count) }
        let len = min(Int(n.magnitude), b.count)
        return (n >= 0, Data(b[0..<len]))
    }
    private static func text(_ f: (UnsafeMutablePointer<UInt8>, Int) -> Int32) -> (ok: Bool, s: String) {
        let r = buf(f); return (r.ok, String(decoding: r.data, as: UTF8.self))
    }

    static func importMnemonic(_ m: String, _ pw: String) -> Data? {
        let r = m.withCString { mp in pw.withCString { pp in
            buf { o, c in escore_import_mnemonic(mp, pp, o, c) } } }
        return r.ok ? r.data : nil
    }
    static func walletInfo(_ ks: Data, _ pw: String, _ pass: String, _ net: UInt8) -> (Bool, String) {
        ks.withUnsafeBytes { kb in pw.withCString { pp in pass.withCString { fp in
            text { o, c in escore_wallet_info(kb.bindMemory(to: UInt8.self).baseAddress, ks.count, pp, fp, net, o, c) }
        } } }
    }
    static func summarize(_ net: UInt8, _ unsigned: Data) -> (Bool, String) {
        unsigned.withUnsafeBytes { ub in
            text { o, c in escore_summarize(net, ub.bindMemory(to: UInt8.self).baseAddress, unsigned.count, o, c) } }
    }
    static func sign(_ unsigned: Data, _ ks: Data, _ pw: String, _ pass: String) -> (Bool, String) {
        unsigned.withUnsafeBytes { ub in ks.withUnsafeBytes { kb in pw.withCString { pp in pass.withCString { fp in
            text { o, c in escore_sign(ub.bindMemory(to: UInt8.self).baseAddress, unsigned.count,
                                       kb.bindMemory(to: UInt8.self).baseAddress, ks.count, pp, fp, o, c) }
        } } } }
    }
    static func sampleUnsigned() -> String { text { o, c in escore_sample_unsigned(o, c) }.s }
}

// MARK: - Keychain（存加密 keystore blob）
enum KC {
    static let acct = "keystore.blob"
    static func save(_ d: Data) {
        let base: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                    kSecAttrAccount as String: acct]
        SecItemDelete(base as CFDictionary)
        var add = base; add[kSecValueData as String] = d
        add[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        SecItemAdd(add as CFDictionary, nil)
    }
    static func load() -> Data? {
        let q: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                                kSecAttrAccount as String: acct,
                                kSecReturnData as String: true,
                                kSecMatchLimit as String: kSecMatchLimitOne]
        var out: AnyObject?
        return SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess ? out as? Data : nil
    }
    static func clear() {
        SecItemDelete([kSecClass as String: kSecClassGenericPassword,
                       kSecAttrAccount as String: acct] as CFDictionary)
    }
}

// MARK: - 会话状态
final class Session {
    static let shared = Session()
    var ks: Data?
    var password = ""
    var passphrase = ""
    var net: UInt8 = 1 // 0=Mainnet, 1=Test（默认测试网）
}

// MARK: - 工具
func makeQR(_ s: String) -> UIImage? {
    guard let d = s.data(using: .utf8), let f = CIFilter(name: "CIQRCodeGenerator") else { return nil }
    f.setValue(d, forKey: "inputMessage")
    f.setValue("M", forKey: "inputCorrectionLevel")
    guard let ci = f.outputImage?.transformed(by: CGAffineTransform(scaleX: 6, y: 6)) else { return nil }
    return UIImage(ciImage: ci)
}
func biometricGate(_ reason: String, _ done: @escaping (Bool) -> Void) {
    let ctx = LAContext()
    var e: NSError?
    if ctx.canEvaluatePolicy(.deviceOwnerAuthentication, error: &e) {
        ctx.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason) { ok, _ in
            DispatchQueue.main.async { done(ok) }
        }
    } else {
        done(true) // 无生物识别（如未设置的模拟器）→ 放行（仅 PoC）
    }
}
extension UIViewController {
    func alert(_ msg: String) {
        let a = UIAlertController(title: nil, message: msg, preferredStyle: .alert)
        a.addAction(UIAlertAction(title: "好", style: .default))
        present(a, animated: true)
    }
    func vstack(_ views: [UIView]) -> UIStackView {
        let s = UIStackView(arrangedSubviews: views)
        s.axis = .vertical; s.spacing = 12; s.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(s)
        NSLayoutConstraint.activate([
            s.centerYAnchor.constraint(equalTo: view.safeAreaLayoutGuide.centerYAnchor),
            s.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
            s.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -20),
        ])
        return s
    }
}

// MARK: - 1. 导入助记词
final class SetupVC: UIViewController {
    let mnemonic = UITextView()
    let pw = UITextField()
    override func viewDidLoad() {
        super.viewDidLoad(); title = "导入钱包"; view.backgroundColor = .white
        mnemonic.text = "test test test test test test test test test test test junk" // PoC 预填
        mnemonic.font = .systemFont(ofSize: 14); mnemonic.layer.borderWidth = 1
        mnemonic.layer.borderColor = UIColor.lightGray.cgColor
        mnemonic.heightAnchor.constraint(equalToConstant: 90).isActive = true
        pw.placeholder = "keystore 口令"; pw.isSecureTextEntry = true; pw.borderStyle = .roundedRect; pw.text = "pw"
        let b = UIButton(type: .system); b.setTitle("导入并加密保存", for: .normal)
        b.addTarget(self, action: #selector(doImport), for: .touchUpInside)
        _ = vstack([label("输入 BIP-39 助记词（12/24 词）:"), mnemonic, pw, b])
    }
    func label(_ t: String) -> UILabel { let l = UILabel(); l.numberOfLines = 0; l.text = t; return l }
    @objc func doImport() {
        let m = (mnemonic.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        guard let blob = Core.importMnemonic(m, pw.text ?? "") else { alert("助记词无效"); return }
        KC.save(blob)
        navigationController?.setViewControllers([HomeVC()], animated: true)
    }
}

// MARK: - 2. 解锁 + 概览
final class HomeVC: UIViewController {
    let pw = UITextField()
    let net = UISegmentedControl(items: ["主网", "测试网"])
    let info = UILabel()
    override func viewDidLoad() {
        super.viewDidLoad(); title = "btc-wallate 签名机"; view.backgroundColor = .white
        pw.placeholder = "keystore 口令"; pw.isSecureTextEntry = true; pw.borderStyle = .roundedRect; pw.text = "pw"
        net.selectedSegmentIndex = 1
        info.numberOfLines = 0; info.font = .systemFont(ofSize: 13); info.textColor = .darkGray
        let unlock = UIButton(type: .system); unlock.setTitle("解锁并显示地址", for: .normal)
        unlock.addTarget(self, action: #selector(doUnlock), for: .touchUpInside)
        let scan = UIButton(type: .system); scan.setTitle("扫码签名", for: .normal)
        scan.addTarget(self, action: #selector(goScan), for: .touchUpInside)
        let paste = UIButton(type: .system); paste.setTitle("粘贴 UR 签名（测试）", for: .normal)
        paste.addTarget(self, action: #selector(goPaste), for: .touchUpInside)
        let reset = UIButton(type: .system); reset.setTitle("重置钱包", for: .normal)
        reset.setTitleColor(.systemRed, for: .normal)
        reset.addTarget(self, action: #selector(doReset), for: .touchUpInside)
        _ = vstack([net, pw, unlock, info, scan, paste, reset])
    }
    var netVal: UInt8 { UInt8(net.selectedSegmentIndex == 0 ? 0 : 1) }
    @objc func doUnlock() {
        guard let ks = KC.load() else { alert("无 keystore"); return }
        let (ok, s) = Core.walletInfo(ks, pw.text ?? "", "", netVal)
        if ok { Session.shared.ks = ks; Session.shared.password = pw.text ?? ""; Session.shared.net = netVal
                info.text = "已解锁 ✓\n" + s }
        else { info.text = "解锁失败: " + s }
    }
    func ensureUnlocked() -> Bool {
        if Session.shared.ks == nil { alert("请先解锁"); return false }; return true
    }
    @objc func goScan() { if ensureUnlocked() { navigationController?.pushViewController(ScanVC(), animated: true) } }
    @objc func goPaste() { if ensureUnlocked() { navigationController?.pushViewController(PasteVC(), animated: true) } }
    @objc func doReset() { KC.clear(); Session.shared.ks = nil
        navigationController?.setViewControllers([SetupVC()], animated: true) }
}

// MARK: - 3a. 粘贴 UR（无摄像头/模拟器测试）
final class PasteVC: UIViewController {
    let tv = UITextView()
    override func viewDidLoad() {
        super.viewDidLoad(); title = "粘贴待签数据"; view.backgroundColor = .white
        tv.font = .systemFont(ofSize: 12); tv.layer.borderWidth = 1; tv.layer.borderColor = UIColor.lightGray.cgColor
        tv.text = Core.sampleUnsigned() // 预填一个示例 eth-sign-request
        tv.heightAnchor.constraint(equalToConstant: 160).isActive = true
        let b = UIButton(type: .system); b.setTitle("核对", for: .normal)
        b.addTarget(self, action: #selector(go), for: .touchUpInside)
        _ = vstack([label(), tv, b])
    }
    func label() -> UILabel { let l = UILabel(); l.numberOfLines = 0
        l.text = "粘贴 ur:crypto-psbt / ur:eth-sign-request 或 base64 PSBT："; return l }
    @objc func go() {
        let data = (tv.text ?? "").data(using: .utf8) ?? Data()
        let (ok, s) = Core.summarize(Session.shared.net, data)
        if ok { navigationController?.pushViewController(VerifyVC(unsigned: data, summary: s), animated: true) }
        else { alert("解析失败: " + s) }
    }
}

// MARK: - 3b. 摄像头扫码（真机）
final class ScanVC: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
    let session = AVCaptureSession()
    var frames = Set<String>()
    let status = UILabel()
    override func viewDidLoad() {
        super.viewDidLoad(); title = "扫码"; view.backgroundColor = .black
        status.textColor = .white; status.numberOfLines = 0; status.textAlignment = .center
        status.frame = view.bounds; status.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        guard let dev = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: dev), session.canAddInput(input) else {
            status.text = "无法访问摄像头"; view.addSubview(status); return
        }
        session.addInput(input)
        let out = AVCaptureMetadataOutput(); session.addOutput(out)
        out.setMetadataObjectsDelegate(self, queue: .main); out.metadataObjectTypes = [.qr]
        let preview = AVCaptureVideoPreviewLayer(session: session)
        preview.frame = view.bounds; preview.videoGravity = .resizeAspectFill
        view.layer.addSublayer(preview)
        status.text = "对准手机上的二维码"; view.addSubview(status)
        DispatchQueue.global().async { self.session.startRunning() }
    }
    func metadataOutput(_ o: AVCaptureMetadataOutput, didOutput objs: [AVMetadataObject], from c: AVCaptureConnection) {
        for m in objs {
            guard let r = m as? AVMetadataMachineReadableCodeObject, let s = r.stringValue else { continue }
            frames.insert(s)
        }
        let joined = frames.joined(separator: "\n")
        let data = joined.data(using: .utf8) ?? Data()
        let (ok, sum) = Core.summarize(Session.shared.net, data)
        status.text = "已收 \(frames.count) 帧…"
        if ok {
            session.stopRunning()
            navigationController?.pushViewController(VerifyVC(unsigned: data, summary: sum), animated: true)
        }
    }
    override func viewWillDisappear(_ a: Bool) { super.viewWillDisappear(a); if session.isRunning { session.stopRunning() } }
}

// MARK: - 4. 核对 + Touch ID + 签名
final class VerifyVC: UIViewController {
    let unsigned: Data; let summary: String
    init(unsigned: Data, summary: String) { self.unsigned = unsigned; self.summary = summary
        super.init(nibName: nil, bundle: nil) }
    required init?(coder: NSCoder) { fatalError() }
    override func viewDidLoad() {
        super.viewDidLoad(); title = "核对交易"; view.backgroundColor = .white
        let l = UILabel(); l.numberOfLines = 0; l.font = UIFont(name: "Menlo", size: 13) ?? .systemFont(ofSize: 13); l.text = summary
        let b = UIButton(type: .system); b.setTitle("✅ Touch ID 确认并签名", for: .normal)
        b.addTarget(self, action: #selector(doSign), for: .touchUpInside)
        _ = vstack([label(), l, b])
    }
    func label() -> UILabel { let l = UILabel(); l.numberOfLines = 0
        l.text = "请逐项核对（防止被入侵设备偷换收款地址）："; return l }
    @objc func doSign() {
        guard let ks = Session.shared.ks else { alert("未解锁"); return }
        biometricGate("授权签名") { ok in
            guard ok else { self.alert("生物识别未通过"); return }
            let (sok, s) = Core.sign(self.unsigned, ks, Session.shared.password, Session.shared.passphrase)
            if sok { self.navigationController?.pushViewController(ResultVC(ur: s), animated: true) }
            else { self.alert("签名失败: " + s) }
        }
    }
}

// MARK: - 5. 结果二维码
final class ResultVC: UIViewController {
    let ur: String
    init(ur: String) { self.ur = ur; super.init(nibName: nil, bundle: nil) }
    required init?(coder: NSCoder) { fatalError() }
    override func viewDidLoad() {
        super.viewDidLoad(); title = "签名结果"; view.backgroundColor = .white
        let iv = UIImageView(image: makeQR(ur)); iv.contentMode = .scaleAspectFit
        iv.heightAnchor.constraint(equalToConstant: 260).isActive = true
        let tip = UILabel(); tip.numberOfLines = 0; tip.textAlignment = .center; tip.font = .systemFont(ofSize: 12)
        tip.text = "用手机钱包扫描此二维码广播"
        let done = UIButton(type: .system); done.setTitle("完成", for: .normal)
        done.addTarget(self, action: #selector(finish), for: .touchUpInside)
        _ = vstack([tip, iv, done])
    }
    @objc func finish() { navigationController?.setViewControllers([HomeVC()], animated: true) }
}

// MARK: - App 入口
@UIApplicationMain
final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    func application(_ application: UIApplication,
                     didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        let root: UIViewController = KC.load() == nil ? SetupVC() : HomeVC()
        let nav = UINavigationController(rootViewController: root)
        let w = UIWindow(frame: UIScreen.main.bounds)
        w.rootViewController = nav; w.makeKeyAndVisible(); window = w
        return true
    }
}
