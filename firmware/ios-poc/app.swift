// btc-wallate iOS 离线签名机（UIKit，iOS 12：无 SwiftUI / 无 async）。
// 流程：导入助记词 → (进入即)解锁 → 概览 → 扫码/粘贴 → 屏幕核对 → Touch ID → 签名 → 二维码。
// 设置页：网络(默认主网/测试网)、语言(跟随系统/中/英)、重置钱包(需口令确认)。
// 密码学在 Rust(esp-signer-core)；私钥/种子只在 Rust 内瞬时存在。
import UIKit
import AVFoundation
import LocalAuthentication

// MARK: - i18n
enum L10n {
    static var isEn: Bool {
        if let v = UserDefaults.standard.string(forKey: "lang") {
            return v == "en"
        }
        let sys = Locale.preferredLanguages.first ?? "en"
        return !sys.lowercased().hasPrefix("zh")
    }
    /// nil=跟随系统, "zh", "en"
    static func setLang(_ v: String?) {
        if let v = v { UserDefaults.standard.set(v, forKey: "lang") }
        else { UserDefaults.standard.removeObject(forKey: "lang") }
    }
    static var langIndex: Int { // 0 系统 1 中 2 英
        switch UserDefaults.standard.string(forKey: "lang") { case "zh": return 1; case "en": return 2; default: return 0 }
    }
}
func t(_ zh: String, _ en: String) -> String { L10n.isEn ? en : zh }

// MARK: - Rust FFI 封装
enum Core {
    private static func buf(_ f: (UnsafeMutablePointer<UInt8>, Int) -> Int32) -> (ok: Bool, data: Data) {
        var b = [UInt8](repeating: 0, count: 16384)
        let n = b.withUnsafeMutableBufferPointer { f($0.baseAddress!, $0.count) }
        return (n >= 0, Data(b[0..<min(Int(n.magnitude), b.count)]))
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
        let lang: UInt8 = L10n.isEn ? 1 : 0
        return unsigned.withUnsafeBytes { ub in
            text { o, c in escore_summarize(net, lang, ub.bindMemory(to: UInt8.self).baseAddress, unsigned.count, o, c) } }
    }
    static func sign(_ unsigned: Data, _ ks: Data, _ pw: String, _ pass: String) -> (Bool, String) {
        unsigned.withUnsafeBytes { ub in ks.withUnsafeBytes { kb in pw.withCString { pp in pass.withCString { fp in
            text { o, c in escore_sign(ub.bindMemory(to: UInt8.self).baseAddress, unsigned.count,
                                       kb.bindMemory(to: UInt8.self).baseAddress, ks.count, pp, fp, o, c) }
        } } } }
    }
    static func sampleUnsigned() -> String { text { o, c in escore_sample_unsigned(o, c) }.s }
}

// MARK: - Keychain
enum KC {
    static let acct = "keystore.blob"
    static func save(_ d: Data) {
        let base: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrAccount as String: acct]
        SecItemDelete(base as CFDictionary)
        var add = base; add[kSecValueData as String] = d
        add[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        SecItemAdd(add as CFDictionary, nil)
    }
    static func load() -> Data? {
        let q: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrAccount as String: acct,
                                kSecReturnData as String: true, kSecMatchLimit as String: kSecMatchLimitOne]
        var out: AnyObject?
        return SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess ? out as? Data : nil
    }
    static func clear() {
        SecItemDelete([kSecClass as String: kSecClassGenericPassword, kSecAttrAccount as String: acct] as CFDictionary)
    }
}

// MARK: - 会话
final class Session {
    static let shared = Session()
    var ks: Data?
    var password = ""
    var passphrase = ""
    var net: UInt8 = 0 // 默认主网；启动时从 UserDefaults 覆盖
    func loadNet() { net = UInt8(UserDefaults.standard.integer(forKey: "net")) } // 缺省 0=主网
    func setNet(_ n: UInt8) { net = n; UserDefaults.standard.set(Int(n), forKey: "net") }
    func lock() { ks = nil; password = ""; passphrase = "" }
}

// MARK: - 工具
func makeQR(_ s: String) -> UIImage? {
    guard let d = s.data(using: .utf8), let f = CIFilter(name: "CIQRCodeGenerator") else { return nil }
    f.setValue(d, forKey: "inputMessage"); f.setValue("M", forKey: "inputCorrectionLevel")
    guard let ci = f.outputImage?.transformed(by: CGAffineTransform(scaleX: 6, y: 6)) else { return nil }
    return UIImage(ciImage: ci)
}
func biometricGate(_ reason: String, _ done: @escaping (Bool) -> Void) {
    let ctx = LAContext(); var e: NSError?
    if ctx.canEvaluatePolicy(.deviceOwnerAuthentication, error: &e) {
        ctx.evaluatePolicy(.deviceOwnerAuthentication, localizedReason: reason) { ok, _ in
            DispatchQueue.main.async { done(ok) } }
    } else { done(true) } // 无生物识别(如未设置的模拟器) → 放行(仅 PoC)
}
extension UIViewController {
    func alert(_ msg: String) {
        let a = UIAlertController(title: nil, message: msg, preferredStyle: .alert)
        a.addAction(UIAlertAction(title: t("好", "OK"), style: .default)); present(a, animated: true)
    }
    func vstack(_ views: [UIView], center: Bool = true) -> UIStackView {
        let s = UIStackView(arrangedSubviews: views)
        s.axis = .vertical; s.spacing = 12; s.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(s)
        NSLayoutConstraint.activate([
            center ? s.centerYAnchor.constraint(equalTo: view.safeAreaLayoutGuide.centerYAnchor)
                   : s.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: 24),
            s.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
            s.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -20),
        ])
        return s
    }
    func settingsButton() {
        navigationItem.rightBarButtonItem = UIBarButtonItem(
            title: t("设置", "Settings"), style: .plain, target: self, action: #selector(openSettings))
    }
    @objc func openSettings() { navigationController?.pushViewController(SettingsVC(), animated: true) }
    func label(_ s: String, size: CGFloat = 15) -> UILabel {
        let l = UILabel(); l.numberOfLines = 0; l.text = s; l.font = .systemFont(ofSize: size); return l
    }
    func button(_ title: String, _ sel: Selector, red: Bool = false) -> UIButton {
        let b = UIButton(type: .system); b.setTitle(title, for: .normal)
        if red { b.setTitleColor(.systemRed, for: .normal) }
        b.addTarget(self, action: sel, for: .touchUpInside); return b
    }
}

// MARK: - App 入口
@UIApplicationMain
final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    func application(_ application: UIApplication,
                     didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        Session.shared.loadNet()
        let w = UIWindow(frame: UIScreen.main.bounds)
        w.rootViewController = UINavigationController(rootViewController: makeRoot())
        w.makeKeyAndVisible(); window = w
        return true
    }
    func makeRoot() -> UIViewController {
        if KC.load() == nil { return SetupVC() }
        if Session.shared.ks != nil { return HomeVC() }
        return UnlockVC() // 钱包存在 → 进入即解锁
    }
    func rebuildRoot() {
        window?.rootViewController = UINavigationController(rootViewController: makeRoot())
    }
}
func rebuildRoot() { (UIApplication.shared.delegate as? AppDelegate)?.rebuildRoot() }

// MARK: - 导入
final class SetupVC: UIViewController {
    let mnemonic = UITextView(); let pw = UITextField()
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("导入钱包", "Import Wallet"); view.backgroundColor = .white
        mnemonic.text = "test test test test test test test test test test test junk" // PoC 预填
        mnemonic.font = .systemFont(ofSize: 14); mnemonic.layer.borderWidth = 1
        mnemonic.layer.borderColor = UIColor.lightGray.cgColor
        mnemonic.heightAnchor.constraint(equalToConstant: 90).isActive = true
        pw.placeholder = t("keystore 口令", "keystore password"); pw.isSecureTextEntry = true
        pw.borderStyle = .roundedRect; pw.text = "pw"
        _ = vstack([label(t("输入 BIP-39 助记词（12/24 词）:", "Enter BIP-39 mnemonic (12/24 words):")),
                    mnemonic, pw, button(t("导入并加密保存", "Import & encrypt"), #selector(doImport))])
    }
    @objc func doImport() {
        let m = (mnemonic.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        guard let blob = Core.importMnemonic(m, pw.text ?? "") else { alert(t("助记词无效", "Invalid mnemonic")); return }
        KC.save(blob)
        Session.shared.ks = blob; Session.shared.password = pw.text ?? "" // 已知口令，直接解锁
        navigationController?.setViewControllers([HomeVC()], animated: true)
    }
}

// MARK: - 解锁（钱包存在时的入口）
final class UnlockVC: UIViewController {
    let pw = UITextField()
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("解锁", "Unlock"); view.backgroundColor = .white; settingsButton()
        pw.placeholder = t("keystore 口令", "keystore password"); pw.isSecureTextEntry = true; pw.borderStyle = .roundedRect
        pw.text = "pw"
        _ = vstack([label(t("输入口令解锁钱包", "Enter password to unlock")), pw,
                    button(t("解锁", "Unlock"), #selector(doUnlock))])
    }
    override func viewDidAppear(_ a: Bool) { super.viewDidAppear(a); pw.becomeFirstResponder() }
    @objc func doUnlock() {
        guard let ks = KC.load() else { rebuildRoot(); return }
        let (ok, s) = Core.walletInfo(ks, pw.text ?? "", "", Session.shared.net)
        if ok {
            Session.shared.ks = ks; Session.shared.password = pw.text ?? ""
            navigationController?.setViewControllers([HomeVC()], animated: true)
        } else { alert(t("解锁失败: ", "Unlock failed: ") + s) }
    }
}

// MARK: - 主页（概览 + 入口）
final class HomeVC: UIViewController {
    let info = UILabel()
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("btc-wallate 签名机", "btc-wallate Signer"); view.backgroundColor = .white
        settingsButton()
        info.numberOfLines = 0; info.font = UIFont(name: "Menlo", size: 13) ?? .systemFont(ofSize: 13)
        info.textColor = .darkGray
        _ = vstack([label(t("钱包概览", "Wallet overview"), size: 17), info,
                    button(t("扫码签名", "Scan to sign"), #selector(goScan)),
                    button(t("粘贴 UR 签名（测试）", "Paste UR (test)"), #selector(goPaste))], center: false)
    }
    override func viewWillAppear(_ a: Bool) {
        super.viewWillAppear(a)
        guard let ks = Session.shared.ks else { rebuildRoot(); return }
        let (ok, s) = Core.walletInfo(ks, Session.shared.password, Session.shared.passphrase, Session.shared.net)
        info.text = ok ? s : t("读取失败", "read failed")
    }
    @objc func goScan() { navigationController?.pushViewController(ScanVC(), animated: true) }
    @objc func goPaste() { navigationController?.pushViewController(PasteVC(), animated: true) }
}

// MARK: - 设置
final class SettingsVC: UIViewController {
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("设置", "Settings"); view.backgroundColor = .white
        let net = UISegmentedControl(items: [t("主网", "Mainnet"), t("测试网", "Testnet")])
        net.selectedSegmentIndex = Int(Session.shared.net)
        net.addTarget(self, action: #selector(netChanged(_:)), for: .valueChanged)
        let lang = UISegmentedControl(items: [t("跟随系统", "System"), "中文", "English"])
        lang.selectedSegmentIndex = L10n.langIndex
        lang.addTarget(self, action: #selector(langChanged(_:)), for: .valueChanged)
        _ = vstack([label(t("网络", "Network")), net,
                    label(t("语言", "Language")), lang,
                    button(t("重置钱包", "Reset wallet"), #selector(reset), red: true)], center: false)
    }
    @objc func netChanged(_ s: UISegmentedControl) { Session.shared.setNet(UInt8(s.selectedSegmentIndex)) }
    @objc func langChanged(_ s: UISegmentedControl) {
        L10n.setLang(s.selectedSegmentIndex == 1 ? "zh" : s.selectedSegmentIndex == 2 ? "en" : nil)
        rebuildRoot() // 重建界面以应用语言
    }
    @objc func reset() {
        let a = UIAlertController(title: t("重置钱包", "Reset wallet"),
                                  message: t("输入口令确认删除本机钱包", "Enter password to confirm deletion"),
                                  preferredStyle: .alert)
        a.addTextField { $0.isSecureTextEntry = true; $0.placeholder = t("keystore 口令", "keystore password") }
        a.addAction(UIAlertAction(title: t("取消", "Cancel"), style: .cancel))
        a.addAction(UIAlertAction(title: t("确定删除", "Delete"), style: .destructive) { _ in
            let pw = a.textFields?.first?.text ?? ""
            guard let ks = KC.load() else { self.doClear(); return }
            let (ok, _) = Core.walletInfo(ks, pw, "", Session.shared.net)
            if ok { self.doClear() } else { self.alert(t("口令错误", "Wrong password")) }
        })
        present(a, animated: true)
    }
    func doClear() { KC.clear(); Session.shared.lock(); rebuildRoot() }
}

// MARK: - 粘贴 UR（无摄像头/模拟器测试）
final class PasteVC: UIViewController {
    let tv = UITextView()
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("粘贴待签数据", "Paste unsigned data"); view.backgroundColor = .white
        tv.font = .systemFont(ofSize: 12); tv.layer.borderWidth = 1; tv.layer.borderColor = UIColor.lightGray.cgColor
        tv.text = Core.sampleUnsigned()
        tv.heightAnchor.constraint(equalToConstant: 160).isActive = true
        _ = vstack([label(t("粘贴 ur:crypto-psbt / ur:eth-sign-request 或 base64 PSBT：",
                            "Paste ur:crypto-psbt / ur:eth-sign-request or base64 PSBT:")),
                    tv, button(t("核对", "Review"), #selector(go))], center: false)
    }
    @objc func go() {
        let data = (tv.text ?? "").data(using: .utf8) ?? Data()
        let (ok, s) = Core.summarize(Session.shared.net, data)
        if ok { navigationController?.pushViewController(VerifyVC(unsigned: data, summary: s), animated: true) }
        else { alert(t("解析失败: ", "Parse failed: ") + s) }
    }
}

// MARK: - 摄像头扫码（真机）
final class ScanVC: UIViewController, AVCaptureMetadataOutputObjectsDelegate {
    let session = AVCaptureSession(); var frames = Set<String>(); let status = UILabel()
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("扫码", "Scan"); view.backgroundColor = .black
        status.textColor = .white; status.numberOfLines = 0; status.textAlignment = .center
        status.frame = view.bounds; status.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        guard let dev = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: dev), session.canAddInput(input) else {
            status.text = t("无法访问摄像头", "Camera unavailable"); view.addSubview(status); return
        }
        session.addInput(input)
        let out = AVCaptureMetadataOutput(); session.addOutput(out)
        out.setMetadataObjectsDelegate(self, queue: .main); out.metadataObjectTypes = [.qr]
        let preview = AVCaptureVideoPreviewLayer(session: session)
        preview.frame = view.bounds; preview.videoGravity = .resizeAspectFill; view.layer.addSublayer(preview)
        status.text = t("对准手机上的二维码", "Point at the QR on your phone"); view.addSubview(status)
        DispatchQueue.global().async { self.session.startRunning() }
    }
    func metadataOutput(_ o: AVCaptureMetadataOutput, didOutput objs: [AVMetadataObject], from c: AVCaptureConnection) {
        for m in objs { if let r = m as? AVMetadataMachineReadableCodeObject, let s = r.stringValue { frames.insert(s) } }
        let data = frames.joined(separator: "\n").data(using: .utf8) ?? Data()
        let (ok, sum) = Core.summarize(Session.shared.net, data)
        status.text = String(format: t("已收 %d 帧…", "Got %d frames…"), frames.count)
        if ok { session.stopRunning()
            navigationController?.pushViewController(VerifyVC(unsigned: data, summary: sum), animated: true) }
    }
    override func viewWillDisappear(_ a: Bool) { super.viewWillDisappear(a); if session.isRunning { session.stopRunning() } }
}

// MARK: - 核对 + Touch ID + 签名
final class VerifyVC: UIViewController {
    let unsigned: Data; let summary: String
    init(unsigned: Data, summary: String) { self.unsigned = unsigned; self.summary = summary
        super.init(nibName: nil, bundle: nil) }
    required init?(coder: NSCoder) { fatalError() }
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("核对交易", "Review"); view.backgroundColor = .white
        let l = UILabel(); l.numberOfLines = 0; l.font = UIFont(name: "Menlo", size: 13) ?? .systemFont(ofSize: 13); l.text = summary
        _ = vstack([label(t("请逐项核对（防止被入侵设备偷换收款地址）：",
                            "Verify each item (a compromised device may swap the recipient):")),
                    l, button(t("✅ Touch ID 确认并签名", "✅ Confirm with Touch ID & sign"), #selector(doSign))], center: false)
    }
    @objc func doSign() {
        guard let ks = Session.shared.ks else { alert(t("未解锁", "Not unlocked")); return }
        biometricGate(t("授权签名", "Authorize signing")) { ok in
            guard ok else { self.alert(t("生物识别未通过", "Biometrics failed")); return }
            let (sok, s) = Core.sign(self.unsigned, ks, Session.shared.password, Session.shared.passphrase)
            if sok { self.navigationController?.pushViewController(ResultVC(ur: s), animated: true) }
            else { self.alert(t("签名失败: ", "Sign failed: ") + s) }
        }
    }
}

// MARK: - 结果二维码
final class ResultVC: UIViewController {
    let ur: String
    init(ur: String) { self.ur = ur; super.init(nibName: nil, bundle: nil) }
    required init?(coder: NSCoder) { fatalError() }
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("签名结果", "Signature"); view.backgroundColor = .white
        let iv = UIImageView(image: makeQR(ur)); iv.contentMode = .scaleAspectFit
        iv.heightAnchor.constraint(equalToConstant: 260).isActive = true
        let tip = UILabel(); tip.numberOfLines = 0; tip.textAlignment = .center; tip.font = .systemFont(ofSize: 12)
        tip.text = t("用手机钱包扫描此二维码广播", "Scan this QR with your wallet to broadcast")
        _ = vstack([tip, iv, button(t("完成", "Done"), #selector(finish))])
    }
    @objc func finish() { navigationController?.setViewControllers([HomeVC()], animated: true) }
}
