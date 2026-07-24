// btc-wallate iOS 离线签名机（UIKit，iOS 12：无 SwiftUI / 无 async / 无 SF Symbols）。
// 流程：导入助记词 → (进入即)解锁 → 概览 → 扫码/粘贴 → 屏幕核对 → Touch ID → 签名 → 二维码。
// 设置页：网络(默认主网/测试网)、语言(跟随系统/中/英)、重置钱包(需口令确认)。
// 视觉：深色·硬件钱包风（定死一套主题；iOS 12 无系统深浅色）。
// 密码学在 Rust(esp-signer-core)；私钥/种子只在 Rust 内瞬时存在。App 全程不联网。
import UIKit
import AVFoundation
import LocalAuthentication

// MARK: - 设计令牌（深色硬件钱包风）
enum Theme {
    static let bg          = UIColor(hex: 0x0E1116) // 近黑藏蓝底
    static let card        = UIColor(hex: 0x1A1F2A) // 卡片
    static let cardBorder  = UIColor(hex: 0x2A3140)
    static let textPrimary = UIColor(hex: 0xF5F7FA)
    static let textSecond  = UIColor(hex: 0x9AA4B2)
    static let brand       = UIColor(hex: 0xF7931A) // 比特币橙（主强调）
    static let eth         = UIColor(hex: 0x627EEA)
    static let success     = UIColor(hex: 0x2ECC71)
    static let danger      = UIColor(hex: 0xFF5A5F)
    static let warn        = UIColor(hex: 0xFFB020) // 测试网/警示琥珀
    static let radius: CGFloat = 14
    static let btnH: CGFloat = 52
    static func mono(_ s: CGFloat) -> UIFont { UIFont(name: "Menlo", size: s) ?? .systemFont(ofSize: s) }
}
extension UIColor {
    convenience init(hex: UInt32, alpha: CGFloat = 1) {
        self.init(red: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
                  blue: CGFloat(hex & 0xFF) / 255, alpha: alpha)
    }
}

// MARK: - i18n
enum L10n {
    static var isEn: Bool {
        if let v = UserDefaults.standard.string(forKey: "lang") { return v == "en" }
        let sys = Locale.preferredLanguages.first ?? "en"
        return !sys.lowercased().hasPrefix("zh")
    }
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
    } else { done(true) } // 无生物识别(如未设置的模拟器) → 放行(仅本机测试环境)
}
/// 网络说明文案（测试网仅标 Signet；ETH 地址不分网络，按交易 chainId 区分）
func netDesc(_ isTest: Bool) -> String {
    isTest ? t("测试网 · BTC Signet（tb1）· ETH 地址同主网，按交易 chainId（如 Sepolia）",
               "Testnet · BTC Signet (tb1) · ETH same address as mainnet, by tx chainId (e.g. Sepolia)")
           : t("主网 · BTC bc1 · ETH 以太坊主网",
               "Mainnet · BTC bc1 · ETH Ethereum mainnet")
}
/// 地址中段省略：bc1qxy...k4f9
func ellipsize(_ s: String, head: Int = 12, tail: Int = 8) -> String {
    guard s.count > head + tail + 1 else { return s }
    return String(s.prefix(head)) + "…" + String(s.suffix(tail))
}

// MARK: - 品牌标识（程序化绘制盾牌+₿，不依赖图片资源，Setup/Unlock 用）
final class BrandMark: UIView {
    override init(frame: CGRect) { super.init(frame: frame); backgroundColor = .clear }
    required init?(coder: NSCoder) { super.init(coder: coder); backgroundColor = .clear }
    override func draw(_ rect: CGRect) {
        let w = rect.width, h = rect.height
        // 盾牌路径（居中，占 ~86% 宽）
        let sw = w * 0.86, sh = h * 0.92
        let cx = w / 2, top = (h - sh) / 2
        let p = UIBezierPath()
        p.move(to: CGPoint(x: cx - sw/2, y: top + sh*0.06))
        p.addLine(to: CGPoint(x: cx + sw/2, y: top + sh*0.06))
        p.addLine(to: CGPoint(x: cx + sw/2, y: top + sh*0.52))
        p.addQuadCurve(to: CGPoint(x: cx, y: top + sh),
                       controlPoint: CGPoint(x: cx + sw/2, y: top + sh*0.86))
        p.addQuadCurve(to: CGPoint(x: cx - sw/2, y: top + sh*0.52),
                       controlPoint: CGPoint(x: cx - sw/2, y: top + sh*0.86))
        p.close()
        Theme.card.setFill(); p.fill()
        Theme.brand.setStroke(); p.lineWidth = max(2, w * 0.045); p.stroke()
        // 居中 ₿
        let fs = h * 0.5
        let s = NSAttributedString(string: "₿", attributes: [
            .font: UIFont.systemFont(ofSize: fs, weight: .bold), .foregroundColor: Theme.brand])
        let sz = s.size()
        s.draw(at: CGPoint(x: cx - sz.width/2, y: top + sh*0.42 - sz.height/2))
    }
}

// MARK: - 复制按钮（携带载荷）
final class CopyButton: UIButton { var payload = "" }

// MARK: - 地址行（币种 chip + 截断地址 + 复制）
final class AddressRow: UIView {
    private let addr = UILabel()
    private let copy = CopyButton()
    init(chip: String, color: UIColor) {
        super.init(frame: .zero)
        let tag = UILabel()
        tag.text = " \(chip) "; tag.font = .systemFont(ofSize: 12, weight: .bold)
        tag.textColor = .white; tag.backgroundColor = color
        tag.layer.cornerRadius = 5; tag.layer.masksToBounds = true
        tag.textAlignment = .center
        tag.setContentHuggingPriority(.required, for: .horizontal)
        addr.font = Theme.mono(13); addr.textColor = Theme.textPrimary; addr.text = "—"
        copy.setTitle(t("复制", "Copy"), for: .normal)
        copy.setTitleColor(Theme.brand, for: .normal)
        copy.titleLabel?.font = .systemFont(ofSize: 13, weight: .semibold)
        copy.setContentHuggingPriority(.required, for: .horizontal)
        copy.addTarget(self, action: #selector(doCopy), for: .touchUpInside)
        let row = UIStackView(arrangedSubviews: [tag, addr, copy])
        row.axis = .horizontal; row.spacing = 10; row.alignment = .center
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)
        NSLayoutConstraint.activate([
            row.topAnchor.constraint(equalTo: topAnchor), row.bottomAnchor.constraint(equalTo: bottomAnchor),
            row.leadingAnchor.constraint(equalTo: leadingAnchor), row.trailingAnchor.constraint(equalTo: trailingAnchor),
            tag.heightAnchor.constraint(equalToConstant: 22),
        ])
    }
    required init?(coder: NSCoder) { fatalError() }
    func set(_ full: String) { addr.text = ellipsize(full); copy.payload = full }
    @objc private func doCopy() {
        UIPasteboard.general.string = copy.payload
        findVC()?.toast(t("已复制", "Copied"))
    }
    private func findVC() -> BaseVC? {
        var r: UIResponder? = self
        while let n = r?.next { if let vc = n as? BaseVC { return vc }; r = n }
        return nil
    }
}

// MARK: - 通用 VC（深色底 + 浅色状态栏 + 组件工厂）
class BaseVC: UIViewController {
    override var preferredStatusBarStyle: UIStatusBarStyle { .lightContent }
    override func viewDidLoad() { super.viewDidLoad(); view.backgroundColor = Theme.bg }

    /// 顶部对齐的纵向堆叠（避免键盘遮挡按钮）
    @discardableResult
    func stack(_ views: [UIView], spacing: CGFloat = 16, top: CGFloat = 20) -> UIStackView {
        let s = UIStackView(arrangedSubviews: views)
        s.axis = .vertical; s.spacing = spacing; s.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(s)
        NSLayoutConstraint.activate([
            s.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor, constant: top),
            s.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
            s.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -20),
        ])
        return s
    }
    func title(_ s: String) -> UILabel {
        let l = UILabel(); l.text = s; l.textColor = Theme.textPrimary
        l.font = .systemFont(ofSize: 22, weight: .bold); l.numberOfLines = 0; return l
    }
    func body(_ s: String, color: UIColor? = nil) -> UILabel {
        let l = UILabel(); l.text = s; l.textColor = color ?? Theme.textSecond
        l.font = .systemFont(ofSize: 15); l.numberOfLines = 0; return l
    }
    func sectionHeader(_ s: String) -> UILabel {
        let l = UILabel(); l.text = s.uppercased(); l.textColor = Theme.textSecond
        l.font = .systemFont(ofSize: 12, weight: .semibold); return l
    }
    func caption(_ s: String) -> UILabel {
        let l = UILabel(); l.text = s; l.textColor = Theme.textSecond
        l.font = .systemFont(ofSize: 12); l.numberOfLines = 0; return l
    }
    func primaryButton(_ t: String, _ sel: Selector) -> UIButton {
        let b = UIButton(type: .custom); b.setTitle(t, for: .normal)
        b.setTitleColor(.white, for: .normal); b.titleLabel?.font = .systemFont(ofSize: 17, weight: .bold)
        b.backgroundColor = Theme.brand; b.layer.cornerRadius = Theme.radius
        b.heightAnchor.constraint(equalToConstant: Theme.btnH).isActive = true
        b.addTarget(self, action: sel, for: .touchUpInside); return b
    }
    func outlineButton(_ t: String, _ sel: Selector, color: UIColor = Theme.brand) -> UIButton {
        let b = UIButton(type: .custom); b.setTitle(t, for: .normal)
        b.setTitleColor(color, for: .normal); b.titleLabel?.font = .systemFont(ofSize: 16, weight: .semibold)
        b.layer.cornerRadius = Theme.radius; b.layer.borderWidth = 1.5; b.layer.borderColor = color.cgColor
        b.heightAnchor.constraint(equalToConstant: Theme.btnH).isActive = true
        b.addTarget(self, action: sel, for: .touchUpInside); return b
    }
    func field(_ ph: String, secure: Bool = false) -> UITextField {
        let f = UITextField(); f.isSecureTextEntry = secure
        f.attributedPlaceholder = NSAttributedString(string: ph, attributes: [.foregroundColor: Theme.textSecond])
        f.textColor = Theme.textPrimary; f.backgroundColor = Theme.card; f.tintColor = Theme.brand
        f.layer.cornerRadius = 12; f.layer.borderWidth = 1; f.layer.borderColor = Theme.cardBorder.cgColor
        f.leftView = UIView(frame: CGRect(x: 0, y: 0, width: 12, height: 1)); f.leftViewMode = .always
        f.heightAnchor.constraint(equalToConstant: 48).isActive = true
        return f
    }
    /// 卡片容器：圆角+描边+内边距，包一个纵向 stack
    func card(_ views: [UIView], spacing: CGFloat = 12) -> UIView {
        let c = UIView(); c.backgroundColor = Theme.card; c.layer.cornerRadius = Theme.radius
        c.layer.borderWidth = 1; c.layer.borderColor = Theme.cardBorder.cgColor
        let s = UIStackView(arrangedSubviews: views); s.axis = .vertical; s.spacing = spacing
        s.translatesAutoresizingMaskIntoConstraints = false; c.addSubview(s)
        NSLayoutConstraint.activate([
            s.topAnchor.constraint(equalTo: c.topAnchor, constant: 16),
            s.bottomAnchor.constraint(equalTo: c.bottomAnchor, constant: -16),
            s.leadingAnchor.constraint(equalTo: c.leadingAnchor, constant: 16),
            s.trailingAnchor.constraint(equalTo: c.trailingAnchor, constant: -16),
        ])
        return c
    }
    func pill(_ text: String, color: UIColor) -> UILabel {
        let l = UILabel(); l.text = "  \(text)  "; l.font = .systemFont(ofSize: 12, weight: .bold)
        l.textColor = color; l.layer.borderColor = color.cgColor; l.layer.borderWidth = 1
        l.layer.cornerRadius = 11; l.layer.masksToBounds = true
        l.heightAnchor.constraint(equalToConstant: 22).isActive = true
        l.setContentHuggingPriority(.required, for: .horizontal); return l
    }
    func brandRow(subtitle: String) -> UIStackView {
        let mark = BrandMark()
        mark.widthAnchor.constraint(equalToConstant: 56).isActive = true
        mark.heightAnchor.constraint(equalToConstant: 62).isActive = true
        let name = UILabel(); name.text = "btc-wallate"; name.textColor = Theme.textPrimary
        name.font = .systemFont(ofSize: 24, weight: .heavy)
        let sub = UILabel(); sub.text = subtitle; sub.textColor = Theme.textSecond; sub.font = .systemFont(ofSize: 14)
        let txt = UIStackView(arrangedSubviews: [name, sub]); txt.axis = .vertical; txt.spacing = 2
        let row = UIStackView(arrangedSubviews: [mark, txt]); row.axis = .horizontal
        row.spacing = 14; row.alignment = .center
        return row
    }
    func toast(_ msg: String) {
        let l = PaddingLabel(); l.text = msg; l.textColor = .white; l.font = .systemFont(ofSize: 14, weight: .semibold)
        l.backgroundColor = UIColor(hex: 0x000000, alpha: 0.82); l.layer.cornerRadius = 10; l.layer.masksToBounds = true
        l.textAlignment = .center; l.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(l)
        NSLayoutConstraint.activate([
            l.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            l.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -40),
        ])
        l.alpha = 0
        UIView.animate(withDuration: 0.2, animations: { l.alpha = 1 }) { _ in
            UIView.animate(withDuration: 0.3, delay: 1.1, options: [], animations: { l.alpha = 0 }) { _ in l.removeFromSuperview() }
        }
    }
    func alert(_ msg: String) {
        let a = UIAlertController(title: nil, message: msg, preferredStyle: .alert)
        a.addAction(UIAlertAction(title: t("好", "OK"), style: .default)); present(a, animated: true)
    }
    func settingsButton() {
        let b = UIBarButtonItem(title: "•••", style: .plain, target: self, action: #selector(openSettings))
        let f: [NSAttributedString.Key: Any] = [.font: UIFont.systemFont(ofSize: 20, weight: .bold)]
        b.setTitleTextAttributes(f, for: .normal); b.setTitleTextAttributes(f, for: .highlighted)
        b.accessibilityLabel = t("设置", "Settings")   // 无障碍/过审：图标按钮需可读标签
        navigationItem.rightBarButtonItem = b
    }
    @objc func openSettings() { navigationController?.pushViewController(SettingsVC(), animated: true) }
}
/// 带内边距的 label（toast 用）
final class PaddingLabel: UILabel {
    override var intrinsicContentSize: CGSize {
        let s = super.intrinsicContentSize; return CGSize(width: s.width + 32, height: s.height + 18)
    }
}

// MARK: - App 入口
@UIApplicationMain
final class AppDelegate: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    #if DEBUG
    static var demoConsumed = false   // DEMO 截图入口只作用于冷启动首屏，不干扰后续 rebuildRoot
    #endif
    func application(_ application: UIApplication,
                     didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil) -> Bool {
        Session.shared.loadNet()
        applyAppearance()
        let w = UIWindow(frame: UIScreen.main.bounds)
        w.backgroundColor = Theme.bg
        w.rootViewController = nav(makeRoot())
        w.makeKeyAndVisible(); window = w
        return true
    }
    private func applyAppearance() {
        let nb = UINavigationBar.appearance()
        nb.barStyle = .black                 // 状态栏浅色字
        nb.isTranslucent = false
        nb.barTintColor = Theme.bg
        nb.tintColor = Theme.brand
        nb.titleTextAttributes = [.foregroundColor: Theme.textPrimary]
        nb.shadowImage = UIImage()           // 去掉底部分隔线
        nb.setBackgroundImage(UIImage(), for: .default)
        UISegmentedControl.appearance().tintColor = Theme.brand
    }
    private func nav(_ root: UIViewController) -> UINavigationController {
        let n = UINavigationController(rootViewController: root); n.view.backgroundColor = Theme.bg; return n
    }
    func makeRoot() -> UIViewController {
        #if DEBUG
        // 仅调试：DEMO=unlock/home/settings/verify/about 直达指定屏，用于逐屏截图（Release 不含）。
        // 只作用于冷启动首屏；之后 rebuildRoot（重置/切换语言）走正常逻辑，避免重置后被重新导入。
        if !AppDelegate.demoConsumed, let demo = ProcessInfo.processInfo.environment["DEMO"] {
            AppDelegate.demoConsumed = true
            if KC.load() == nil, let blob = Core.importMnemonic("test test test test test test test test test test test junk", "pw") { KC.save(blob) }
            if demo != "unlock" { Session.shared.ks = KC.load(); Session.shared.password = "pw" }
            switch demo {
            case "home": return HomeVC()
            case "settings": return SettingsVC()
            case "about": return AboutVC()
            case "verify":
                let data = Core.sampleUnsigned().data(using: .utf8) ?? Data()
                let (_, sum) = Core.summarize(Session.shared.net, data)
                return VerifyVC(unsigned: data, summary: sum)
            default: return UnlockVC()
            }
        }
        #endif
        if KC.load() == nil { return SetupVC() }
        if Session.shared.ks != nil { return HomeVC() }
        return UnlockVC() // 钱包存在 → 进入即解锁
    }
    func rebuildRoot() { window?.rootViewController = nav(makeRoot()) }
}
func rebuildRoot() { (UIApplication.shared.delegate as? AppDelegate)?.rebuildRoot() }

// MARK: - 导入（参考 imToken 布局：品牌 + 标题 + 大输入框 + 底部固定按钮）
final class SetupVC: BaseVC, UITextViewDelegate {
    private let mnemonic = UITextView()
    private let placeholder = UILabel()
    private var pwField: UITextField!
    private var importBtn: UIButton!
    private var btnBottom: NSLayoutConstraint!

    override func viewDidLoad() {
        super.viewDidLoad(); title = t("导入钱包", "Import Wallet"); navigationItem.title = ""; settingsButton()

        // 顶部：品牌横排（保留 logo，首屏不苍白）+ 标题 + 说明 + 了解链接
        let heading = UILabel()
        heading.text = t("导入助记词", "Import mnemonic")
        heading.font = .systemFont(ofSize: 18, weight: .semibold); heading.textColor = Theme.textPrimary
        let desc = body(t("输入助记词来添加或恢复钱包。助记词将被加密并安全存储在本设备。为了资产安全，本 App 不联网，也不会上传你的助记词。",
                          "Enter a mnemonic to add or recover your wallet. It is encrypted and stored on this device. For your safety the app is offline and never uploads your mnemonic."))
        let link = UIButton(type: .system)
        link.setTitle(t("了解助记词", "About mnemonics"), for: .normal)
        link.setTitleColor(Theme.brand, for: .normal)
        link.titleLabel?.font = .systemFont(ofSize: 14, weight: .semibold)
        link.contentHorizontalAlignment = .leading
        link.addTarget(self, action: #selector(explain), for: .touchUpInside)
        let head = UIStackView(arrangedSubviews: [brandRow(subtitle: t("离线签名机", "Air-gapped signer")), heading, desc, link])
        head.axis = .vertical; head.spacing = 12; head.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(head)

        // 助记词大输入框（弹性高度 + 叠加 placeholder）
        mnemonic.backgroundColor = Theme.card; mnemonic.textColor = Theme.textPrimary
        mnemonic.font = Theme.mono(15); mnemonic.layer.cornerRadius = Theme.radius
        mnemonic.layer.borderWidth = 1; mnemonic.layer.borderColor = Theme.cardBorder.cgColor
        mnemonic.textContainerInset = UIEdgeInsets(top: 12, left: 10, bottom: 12, right: 10)
        mnemonic.autocapitalizationType = .none; mnemonic.autocorrectionType = .no
        mnemonic.delegate = self; mnemonic.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(mnemonic)
        placeholder.text = t("输入助记词单词，用空格分隔", "Enter mnemonic words, separated by spaces")
        placeholder.textColor = Theme.textSecond; placeholder.font = Theme.mono(15); placeholder.numberOfLines = 0
        placeholder.translatesAutoresizingMaskIntoConstraints = false; mnemonic.addSubview(placeholder)

        pwField = field(t("keystore 口令", "keystore password"), secure: true)
        pwField.translatesAutoresizingMaskIntoConstraints = false
        pwField.addTarget(self, action: #selector(editingChanged), for: .editingChanged)
        view.addSubview(pwField)

        importBtn = primaryButton(t("导入并加密保存", "Import & encrypt"), #selector(doImport))
        importBtn.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(importBtn)

        #if DEBUG
        mnemonic.text = "test test test test test test test test test test test junk" // 仅调试预填
        pwField.text = "pw"
        #endif

        let g = view.safeAreaLayoutGuide
        let region = UILayoutGuide(); view.addLayoutGuide(region)   // 可用区：品牌区底 → 口令框顶
        btnBottom = importBtn.bottomAnchor.constraint(equalTo: g.bottomAnchor, constant: -20)
        NSLayoutConstraint.activate([
            head.topAnchor.constraint(equalTo: g.topAnchor, constant: 16),
            head.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
            head.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -20),
            region.topAnchor.constraint(equalTo: head.bottomAnchor, constant: 16),
            region.bottomAnchor.constraint(equalTo: importBtn.topAnchor, constant: -16),
            // 助记词框：顶对齐可用区、高度取一半；口令框紧跟其下方自然上移；按钮底部固定（留白落在口令与按钮之间）
            mnemonic.topAnchor.constraint(equalTo: region.topAnchor),
            mnemonic.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
            mnemonic.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -20),
            mnemonic.heightAnchor.constraint(equalTo: region.heightAnchor, multiplier: 0.5),
            placeholder.topAnchor.constraint(equalTo: mnemonic.topAnchor, constant: 14),
            placeholder.leadingAnchor.constraint(equalTo: mnemonic.leadingAnchor, constant: 14),
            placeholder.trailingAnchor.constraint(equalTo: mnemonic.trailingAnchor, constant: -14),
            pwField.topAnchor.constraint(equalTo: mnemonic.bottomAnchor, constant: 12),
            pwField.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
            pwField.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -20),
            importBtn.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 20),
            importBtn.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -20),
            btnBottom,
        ])
        NotificationCenter.default.addObserver(self, selector: #selector(kbFrame(_:)),
                                               name: UIResponder.keyboardWillChangeFrameNotification, object: nil)
        refresh()
    }

    /// placeholder 显隐 + 按钮启用态（助记词与口令均非空才可导入）
    private func refresh() {
        placeholder.isHidden = !(mnemonic.text ?? "").isEmpty
        let ok = !(mnemonic.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                 && !(pwField.text ?? "").isEmpty
        importBtn.isEnabled = ok
        importBtn.backgroundColor = ok ? Theme.brand : Theme.cardBorder
        importBtn.setTitleColor(ok ? .white : Theme.textSecond, for: .normal)
    }
    func textViewDidChange(_ tv: UITextView) { refresh() }
    @objc func editingChanged() { refresh() }

    @objc func explain() {
        alert(t("助记词（BIP-39）是你钱包的唯一凭证：谁拿到它就能动用你的资产。\n\n· 请离线抄写在纸/金属上，切勿截图、拍照或上传。\n· 本 App 全程离线，助记词仅加密存于本机。\n· 丢失或泄露将导致资产永久损失。",
                "Your BIP-39 mnemonic is the only key to your wallet — anyone who has it controls your funds.\n\n· Write it offline on paper/metal; never screenshot, photograph or upload it.\n· The app is fully offline; the mnemonic is stored encrypted on-device only.\n· Losing or leaking it means permanent loss."))
    }
    @objc func kbFrame(_ n: Notification) {
        guard let v = n.userInfo?[UIResponder.keyboardFrameEndUserInfoKey] as? NSValue else { return }
        let kb = view.convert(v.cgRectValue, from: nil)
        let overlap = max(0, view.bounds.maxY - kb.minY)
        btnBottom.constant = overlap > 0 ? -(overlap - view.safeAreaInsets.bottom + 12) : -20
        UIView.animate(withDuration: 0.25) { self.view.layoutIfNeeded() }
    }

    @objc func doImport() {
        let m = (mnemonic.text ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        guard let blob = Core.importMnemonic(m, pwField.text ?? "") else { alert(t("助记词无效", "Invalid mnemonic")); return }
        KC.save(blob)
        Session.shared.ks = blob; Session.shared.password = pwField.text ?? ""
        navigationController?.setViewControllers([HomeVC()], animated: true)
    }
}

// MARK: - 解锁（钱包存在时的入口）
final class UnlockVC: BaseVC {
    private var pwField: UITextField!
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("解锁", "Unlock"); navigationItem.title = ""; settingsButton()
        let f = field(t("keystore 口令", "keystore password"), secure: true)
        #if DEBUG
        f.text = "pw"
        #endif
        pwField = f
        let mark = BrandMark(); mark.translatesAutoresizingMaskIntoConstraints = false
        let holder = UIView(); holder.addSubview(mark)   // 容器居中，防被 stack 拉伸变形
        NSLayoutConstraint.activate([
            mark.centerXAnchor.constraint(equalTo: holder.centerXAnchor),
            mark.topAnchor.constraint(equalTo: holder.topAnchor),
            mark.bottomAnchor.constraint(equalTo: holder.bottomAnchor),
            mark.widthAnchor.constraint(equalToConstant: 72),
            mark.heightAnchor.constraint(equalToConstant: 80)])
        stack([holder, title(t("解锁钱包", "Unlock wallet")),
               body(t("输入口令解锁本机加密钱包。", "Enter your password to unlock this device's wallet.")),
               f, primaryButton(t("解锁", "Unlock"), #selector(doUnlock))], spacing: 18, top: 36)
    }
    override func viewDidAppear(_ a: Bool) { super.viewDidAppear(a); pwField.becomeFirstResponder() }
    @objc func doUnlock() {
        guard let ks = KC.load() else { rebuildRoot(); return }
        let (ok, s) = Core.walletInfo(ks, pwField.text ?? "", "", Session.shared.net)
        if ok {
            Session.shared.ks = ks; Session.shared.password = pwField.text ?? ""
            navigationController?.setViewControllers([HomeVC()], animated: true)
        } else { alert(t("解锁失败: ", "Unlock failed: ") + s) }
    }
}

// MARK: - 主页（概览 + 入口）
final class HomeVC: BaseVC {
    private let btcRow = AddressRow(chip: "BTC", color: Theme.brand)
    private let ethRow = AddressRow(chip: "ETH", color: Theme.eth)
    private lazy var netHint = caption("")
    private var pillHolder: UIView!
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("btc-wallate", "btc-wallate"); settingsButton()
        pillHolder = UIView()
        let hdr = UIStackView(arrangedSubviews: [brandRow(subtitle: t("离线签名机", "Air-gapped signer")), UIView(), pillHolder])
        hdr.axis = .horizontal; hdr.alignment = .center
        let overview = card([sectionHeader(t("钱包地址", "Wallet addresses")), btcRow, ethRow, netHint])
        stack([hdr, overview,
               primaryButton(t("扫码签名", "Scan to sign"), #selector(goScan)),
               outlineButton(t("手动输入交易", "Enter transaction"), #selector(goPaste))], spacing: 18)
    }
    override func viewWillAppear(_ a: Bool) {
        super.viewWillAppear(a)
        // 网络徽标
        pillHolder.subviews.forEach { $0.removeFromSuperview() }
        let isTest = Session.shared.net != 0
        netHint.text = netDesc(isTest)
        let p = pill(isTest ? t("测试网", "Testnet") : t("主网", "Mainnet"), color: isTest ? Theme.warn : Theme.success)
        p.translatesAutoresizingMaskIntoConstraints = false; pillHolder.addSubview(p)
        NSLayoutConstraint.activate([
            p.topAnchor.constraint(equalTo: pillHolder.topAnchor), p.bottomAnchor.constraint(equalTo: pillHolder.bottomAnchor),
            p.leadingAnchor.constraint(equalTo: pillHolder.leadingAnchor), p.trailingAnchor.constraint(equalTo: pillHolder.trailingAnchor)])
        // 地址
        guard let ks = Session.shared.ks else { rebuildRoot(); return }
        let (ok, s) = Core.walletInfo(ks, Session.shared.password, Session.shared.passphrase, Session.shared.net)
        if ok {
            for line in s.split(separator: "\n") {
                if line.hasPrefix("BTC:") { btcRow.set(line.replacingOccurrences(of: "BTC:", with: "").trimmingCharacters(in: .whitespaces)) }
                if line.hasPrefix("ETH:") { ethRow.set(line.replacingOccurrences(of: "ETH:", with: "").trimmingCharacters(in: .whitespaces)) }
            }
        } else { btcRow.set(t("读取失败", "read failed")); ethRow.set("—") }
    }
    @objc func goScan() { navigationController?.pushViewController(ScanVC(), animated: true) }
    @objc func goPaste() { navigationController?.pushViewController(PasteVC(), animated: true) }
}

// MARK: - 设置
final class SettingsVC: BaseVC {
    private lazy var netHint = caption(netDesc(Session.shared.net != 0))
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("设置", "Settings")
        let net = UISegmentedControl(items: [t("主网", "Mainnet"), t("测试网", "Testnet")])
        net.selectedSegmentIndex = Int(Session.shared.net)
        styleSeg(net); net.addTarget(self, action: #selector(netChanged(_:)), for: .valueChanged)
        let lang = UISegmentedControl(items: [t("跟随系统", "System"), "中文", "English"])
        lang.selectedSegmentIndex = L10n.langIndex
        styleSeg(lang); lang.addTarget(self, action: #selector(langChanged(_:)), for: .valueChanged)
        let about = body("btc-wallate · v1.0\n" + t("非托管本地签名器 · 不联网 · 不做币币兑换/投资建议",
                                                     "Non-custodial local signer · offline · no exchange/advice"))
        var items: [UIView] = [
            sectionHeader(t("网络", "Network")), card([net, netHint]),
            sectionHeader(t("语言", "Language")), card([lang]),
            sectionHeader(t("关于", "About")),
            card([about, outlineButton(t("关于与免责声明", "About & disclaimer"), #selector(openAbout))])]
        // 仅在已有钱包时显示「重置钱包」（从导入页进入时尚无钱包，无可重置）
        if KC.load() != nil {
            items.append(sectionHeader(t("危险区", "Danger zone")))
            items.append(card([body(t("删除本机加密钱包，需口令确认。请先备份助记词。",
                                      "Delete this device's wallet (password required). Back up your mnemonic first.")),
                               outlineButton(t("重置钱包", "Reset wallet"), #selector(reset), color: Theme.danger)]))
        }
        stack(items)
    }
    private func styleSeg(_ s: UISegmentedControl) {
        s.setTitleTextAttributes([.foregroundColor: Theme.textSecond], for: .normal)
        s.setTitleTextAttributes([.foregroundColor: UIColor.white], for: .selected)
        if #available(iOS 13.0, *) {
            s.selectedSegmentTintColor = Theme.brand   // 选中橙底白字
            s.backgroundColor = Theme.card
        } else {
            s.tintColor = Theme.brand                  // iOS 12：tintColor 决定选中填充
        }
    }
    @objc func openAbout() { navigationController?.pushViewController(AboutVC(), animated: true) }
    @objc func netChanged(_ s: UISegmentedControl) {
        Session.shared.setNet(UInt8(s.selectedSegmentIndex))
        netHint.text = netDesc(s.selectedSegmentIndex != 0)   // 说明随网络切换更新
    }
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

// MARK: - 粘贴 UR（无摄像头/测试）
final class PasteVC: BaseVC {
    let tv = UITextView()
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("粘贴待签数据", "Paste unsigned data")
        tv.backgroundColor = Theme.card; tv.textColor = Theme.textPrimary; tv.font = Theme.mono(12)
        tv.layer.cornerRadius = 12; tv.layer.borderWidth = 1; tv.layer.borderColor = Theme.cardBorder.cgColor
        tv.textContainerInset = UIEdgeInsets(top: 10, left: 8, bottom: 10, right: 8)
        tv.autocapitalizationType = .none; tv.autocorrectionType = .no
        #if DEBUG
        tv.text = Core.sampleUnsigned()   // 仅调试预填示例；Release 留空，审核员从审核备注粘贴
        #endif
        tv.heightAnchor.constraint(equalToConstant: 180).isActive = true
        stack([body(t("粘贴 ur:crypto-psbt / ur:eth-sign-request 或 base64 PSBT：",
                      "Paste ur:crypto-psbt / ur:eth-sign-request or base64 PSBT:")),
               tv, primaryButton(t("核对", "Review"), #selector(go))])
    }
    @objc func go() {
        let data = (tv.text ?? "").data(using: .utf8) ?? Data()
        let (ok, s) = Core.summarize(Session.shared.net, data)
        if ok { navigationController?.pushViewController(VerifyVC(unsigned: data, summary: s), animated: true) }
        else { alert(t("解析失败: ", "Parse failed: ") + s) }
    }
}

// MARK: - 摄像头扫码（真机）
final class ScanVC: BaseVC, AVCaptureMetadataOutputObjectsDelegate {
    let session = AVCaptureSession(); var frames = Set<String>(); let status = PaddingLabel()
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("扫码", "Scan"); view.backgroundColor = .black
        guard let dev = AVCaptureDevice.default(for: .video),
              let input = try? AVCaptureDeviceInput(device: dev), session.canAddInput(input) else {
            showStatus(t("无法访问摄像头", "Camera unavailable")); return
        }
        session.addInput(input)
        let out = AVCaptureMetadataOutput(); session.addOutput(out)
        out.setMetadataObjectsDelegate(self, queue: .main); out.metadataObjectTypes = [.qr]
        let preview = AVCaptureVideoPreviewLayer(session: session)
        preview.frame = view.bounds; preview.videoGravity = .resizeAspectFill; view.layer.addSublayer(preview)
        // 取景框
        let side = min(view.bounds.width, view.bounds.height) * 0.68
        let frame = UIView(frame: CGRect(x: (view.bounds.width - side)/2, y: (view.bounds.height - side)/2 - 20,
                                         width: side, height: side))
        frame.layer.borderColor = Theme.brand.cgColor; frame.layer.borderWidth = 3; frame.layer.cornerRadius = 16
        frame.autoresizingMask = [.flexibleTopMargin, .flexibleBottomMargin, .flexibleLeftMargin, .flexibleRightMargin]
        view.addSubview(frame)
        showStatus(t("对准另一台设备上的二维码", "Point at the QR on the other device"))
        DispatchQueue.global().async { self.session.startRunning() }
    }
    private func showStatus(_ s: String) {
        status.text = s; status.textColor = .white; status.font = .systemFont(ofSize: 14, weight: .semibold)
        status.backgroundColor = UIColor(hex: 0x000000, alpha: 0.7); status.layer.cornerRadius = 10; status.layer.masksToBounds = true
        status.textAlignment = .center; status.numberOfLines = 0
        status.translatesAutoresizingMaskIntoConstraints = false
        if status.superview == nil {
            view.addSubview(status)
            NSLayoutConstraint.activate([
                status.centerXAnchor.constraint(equalTo: view.centerXAnchor),
                status.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor, constant: -40),
                status.leadingAnchor.constraint(greaterThanOrEqualTo: view.leadingAnchor, constant: 20)])
        }
    }
    func metadataOutput(_ o: AVCaptureMetadataOutput, didOutput objs: [AVMetadataObject], from c: AVCaptureConnection) {
        for m in objs { if let r = m as? AVMetadataMachineReadableCodeObject, let s = r.stringValue { frames.insert(s) } }
        let data = frames.joined(separator: "\n").data(using: .utf8) ?? Data()
        let (ok, sum) = Core.summarize(Session.shared.net, data)
        showStatus(String(format: t("已收 %d 帧…", "Got %d frames…"), frames.count))
        if ok { session.stopRunning()
            navigationController?.pushViewController(VerifyVC(unsigned: data, summary: sum), animated: true) }
    }
    override func viewWillDisappear(_ a: Bool) { super.viewWillDisappear(a); if session.isRunning { session.stopRunning() } }
}

// MARK: - 核对 + Touch ID + 签名
final class VerifyVC: BaseVC {
    let unsigned: Data; let summary: String
    init(unsigned: Data, summary: String) { self.unsigned = unsigned; self.summary = summary
        super.init(nibName: nil, bundle: nil) }
    required init?(coder: NSCoder) { fatalError() }
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("核对交易", "Review")
        // 琥珀警示条
        let warn = PaddingLabel(); warn.numberOfLines = 0
        warn.text = t("⚠︎ 请逐项核对收款地址与金额，防止被入侵设备偷换。",
                      "⚠︎ Verify each recipient and amount — a compromised device may swap them.")
        warn.textColor = Theme.bg; warn.backgroundColor = Theme.warn; warn.font = .systemFont(ofSize: 13, weight: .semibold)
        warn.layer.cornerRadius = 10; warn.layer.masksToBounds = true
        // 收据卡片
        let l = UILabel(); l.numberOfLines = 0; l.font = Theme.mono(13); l.textColor = Theme.textPrimary; l.text = summary
        stack([warn, card([l]),
               primaryButton(t("Touch ID 确认并签名", "Confirm with Touch ID & sign"), #selector(doSign))])
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
final class ResultVC: BaseVC {
    let ur: String
    init(ur: String) { self.ur = ur; super.init(nibName: nil, bundle: nil) }
    required init?(coder: NSCoder) { fatalError() }
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("签名结果", "Signature")
        // 二维码放白色卡片（深色主题下必须白底保证可扫）
        let qrWrap = UIView(); qrWrap.backgroundColor = .white; qrWrap.layer.cornerRadius = Theme.radius
        let iv = UIImageView(image: makeQR(ur)); iv.contentMode = .scaleAspectFit
        iv.translatesAutoresizingMaskIntoConstraints = false; qrWrap.addSubview(iv)
        NSLayoutConstraint.activate([
            iv.topAnchor.constraint(equalTo: qrWrap.topAnchor, constant: 16),
            iv.bottomAnchor.constraint(equalTo: qrWrap.bottomAnchor, constant: -16),
            iv.leadingAnchor.constraint(equalTo: qrWrap.leadingAnchor, constant: 16),
            iv.trailingAnchor.constraint(equalTo: qrWrap.trailingAnchor, constant: -16),
            iv.heightAnchor.constraint(equalToConstant: 240)])
        let tip = body(t("用手机观察钱包扫描此二维码广播交易。", "Scan this QR with your watch-only wallet to broadcast."))
        tip.textAlignment = .center
        stack([tip, qrWrap,
               outlineButton(t("复制 UR 文本", "Copy UR text"), #selector(copyUR)),
               primaryButton(t("完成", "Done"), #selector(finish))])
    }
    @objc func copyUR() { UIPasteboard.general.string = ur; toast(t("已复制", "Copied")) }
    @objc func finish() { navigationController?.setViewControllers([HomeVC()], animated: true) }
}

// MARK: - 关于与免责声明（合规声明页）
final class AboutVC: BaseVC {
    override func viewDidLoad() {
        super.viewDidLoad(); title = t("关于", "About")
        let sv = UIScrollView(); sv.translatesAutoresizingMaskIntoConstraints = false; view.addSubview(sv)
        let c = UIStackView(); c.axis = .vertical; c.spacing = 16
        c.translatesAutoresizingMaskIntoConstraints = false; sv.addSubview(c)
        NSLayoutConstraint.activate([
            sv.topAnchor.constraint(equalTo: view.safeAreaLayoutGuide.topAnchor),
            sv.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            sv.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            sv.bottomAnchor.constraint(equalTo: view.safeAreaLayoutGuide.bottomAnchor),
            c.topAnchor.constraint(equalTo: sv.topAnchor, constant: 20),
            c.bottomAnchor.constraint(equalTo: sv.bottomAnchor, constant: -20),
            c.leadingAnchor.constraint(equalTo: sv.leadingAnchor, constant: 20),
            c.trailingAnchor.constraint(equalTo: sv.trailingAnchor, constant: -20),
            c.widthAnchor.constraint(equalTo: sv.widthAnchor, constant: -40)])
        c.addArrangedSubview(brandRow(subtitle: "v1.0"))
        c.addArrangedSubview(card([
            sectionHeader(t("这是什么", "What this is")),
            body(t("btc-wallate 是一个非托管、离线的比特币 / 以太坊交易签名器。它只在本机对交易进行签名，需配合联网的观察钱包广播。",
                   "btc-wallate is a non-custodial, offline transaction signer for Bitcoin & Ethereum. It only signs locally; a separate online watch-only wallet broadcasts."),
                 color: Theme.textPrimary)]))
        c.addArrangedSubview(card([
            sectionHeader(t("隐私与安全", "Privacy & security")),
            bullet(t("助记词 / 私钥仅以加密形式存储在本机（Keychain / Secure Enclave），永不离开设备、永不上传。",
                     "Mnemonic / private keys are stored encrypted on-device only (Keychain / Secure Enclave); they never leave the device and are never uploaded.")),
            bullet(t("App 全程离线运行，无任何网络请求，不收集、不传输任何数据。",
                     "Runs fully offline with zero network requests; collects and transmits no data.")),
            bullet(t("建议使用时开启飞行模式，并离线备份助记词。",
                     "Use in airplane mode and keep an offline backup of your mnemonic."))]))
        c.addArrangedSubview(card([
            sectionHeader(t("合规声明", "Compliance")),
            bullet(t("非托管：本 App 不保管你的资金，也无法动用你的资产。",
                     "Non-custodial: the app never holds or can move your funds.")),
            bullet(t("不做币币兑换、不做法币出入金、不提供任何投资建议。",
                     "No crypto exchange, no fiat on/off-ramp, no investment advice.")),
            bullet(t("加密资产波动与操作风险由使用者自行承担；助记词丢失或泄露将导致资产永久损失。",
                     "You bear all market and operational risk; a lost or leaked mnemonic means permanent loss."))]))
    }
    private func bullet(_ s: String) -> UILabel { body("•  " + s, color: Theme.textPrimary) }
}
