package com.ecohash.btcwallate

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.graphics.Color
import android.graphics.Typeface
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.*
import androidx.biometric.BiometricManager
import androidx.biometric.BiometricPrompt
import androidx.core.content.ContextCompat

private fun BaseActivity.goRoot(cls: Class<*>) {
    startActivity(Intent(this, cls).apply { flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK })
    finish()
}
private fun BaseActivity.settingsLink(): TextView = TextView(this).apply {
    text = "•••"; setTextColor(Theme.brand); textSize = 20f; setTypeface(null, Typeface.BOLD)
    gravity = Gravity.END; isClickable = true; setOnClickListener { startAct(SettingsActivity::class.java) }
}

// ===== 导入 =====
class SetupActivity : BaseActivity() {
    override fun onCreate(s: Bundle?) {
        super.onCreate(s)
        val mnem = multiline(t("输入助记词单词，用空格分隔", "Enter mnemonic words, separated by spaces"), 150)
        val pw = field(t("keystore 口令", "keystore password"), true)
        val link = TextView(this).apply {
            text = t("了解助记词", "About mnemonics"); setTextColor(Theme.brand); textSize = 14f; setTypeface(null, Typeface.BOLD)
            isClickable = true; setOnClickListener {
                alert(t("助记词是钱包的唯一凭证：谁拿到它就能动用你的资产。\n\n· 请离线抄写在纸/金属上，切勿截图、拍照或上传。\n· 本 App 全程离线，助记词仅加密存于本机。\n· 丢失或泄露将导致资产永久损失。",
                        "Your mnemonic is the only key to your wallet.\n\n· Write it offline; never screenshot/upload.\n· This app is offline; the mnemonic is stored encrypted on-device only.\n· Losing or leaking it means permanent loss."))
            }
        }
        val create = outlineButton(t("创建新钱包", "Create new wallet")) { startAct(GenerateActivity::class.java) }
        val import = primaryButton(t("导入并加密保存", "Import & encrypt")) {
            val m = mnem.text.toString().trim()
            val blob = NativeCore.importMnemonic(m, pw.text.toString())
            if (blob == null) { alert(t("助记词无效", "Invalid mnemonic")); return@primaryButton }
            KC.save(this, blob); Session.ks = blob; Session.password = pw.text.toString()
            goRoot(HomeActivity::class.java)
        }
        setContentView(rootColumn(listOf(
            settingsLink(),
            brandRow(t("离线签名机", "Air-gapped signer")),
            heading(t("导入助记词", "Import mnemonic")),
            body(t("输入助记词来添加或恢复钱包。助记词将被加密并安全存储在本设备。本 App 不联网，也不会上传你的助记词。",
                   "Enter a mnemonic to add or recover your wallet. Encrypted on-device; the app is offline and never uploads it.")),
            link, mnem, pw, create, import
        )))
    }
}

// ===== 生成新钱包（12 词）=====
class GenerateActivity : BaseActivity() {
    override fun onCreate(s: Bundle?) {
        super.onCreate(s)
        val mnemonic = NativeCore.generateMnemonic(12)
        val words = mnemonic.split(" ")
        val warn = TextView(this).apply {
            text = t("⚠︎ 这是你钱包的唯一备份。请离线抄写在纸/金属上，切勿截图、拍照或上传；任何人拿到它即可动用你的资产。",
                     "⚠︎ The only backup of your wallet. Write it offline; never screenshot/upload. Anyone with it controls your funds.")
            setTextColor(Theme.bg); setBackgroundColor(Theme.warn); textSize = 13f; setTypeface(null, Typeface.BOLD)
            setPadding(dp(14), dp(12), dp(14), dp(12))
        }
        // 两列词网格
        val cols = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        val half = (words.size + 1) / 2
        fun colView(range: IntRange) = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            for (i in range) if (i < words.size) addView(mono(String.format("%2d. %s", i + 1, words[i]), 14f), lp(topDp = if (i == range.first) 0 else 6))
        }
        cols.addView(colView(0 until half), LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
        cols.addView(colView(half until words.size), LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
        val gridCard = card(listOf(sectionHeader(t("助记词", "Mnemonic")), cols))

        val sw = Switch(this)
        val agree = body(t("我已离线抄写备份", "I have written it down offline"), Theme.textPrimary)
        val checkRow = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL
            addView(agree, LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f)); addView(sw)
        }
        val next = primaryButton(t("下一步：设置口令", "Next: set password")) {
            if (!sw.isChecked) { toast(t("请先确认已备份", "Please confirm backup first")); return@primaryButton }
            startActivity(Intent(this, SetPassActivity::class.java).putExtra("mnemonic", mnemonic))
        }
        setEnabledStyle(next, false)
        sw.setOnCheckedChangeListener { _, b -> setEnabledStyle(next, b) }
        setContentView(rootColumn(listOf(backTitle(t("备份助记词", "Back up mnemonic")), warn, gridCard, checkRow, next)))
    }
}

// ===== 设置口令 =====
class SetPassActivity : BaseActivity() {
    override fun onCreate(s: Bundle?) {
        super.onCreate(s)
        val mnemonic = intent.getStringExtra("mnemonic") ?: ""
        val p1 = field(t("设置 keystore 口令", "Set keystore password"), true)
        val p2 = field(t("再次输入口令", "Confirm password"), true)
        val save = primaryButton(t("创建并加密保存", "Create & encrypt")) {
            val blob = NativeCore.importMnemonic(mnemonic, p1.text.toString())
            if (blob == null) { alert(t("创建失败", "Creation failed")); return@primaryButton }
            KC.save(this, blob); Session.ks = blob; Session.password = p1.text.toString()
            goRoot(HomeActivity::class.java)
        }
        fun refresh() { setEnabledStyle(save, p1.text.isNotEmpty() && p1.text.toString() == p2.text.toString()) }
        val w = object : android.text.TextWatcher {
            override fun afterTextChanged(x: android.text.Editable?) = refresh()
            override fun beforeTextChanged(a: CharSequence?, b: Int, c: Int, d: Int) {}
            override fun onTextChanged(a: CharSequence?, b: Int, c: Int, d: Int) {}
        }
        p1.addTextChangedListener(w); p2.addTextChangedListener(w)
        refresh()
        setContentView(rootColumn(listOf(
            backTitle(t("设置口令", "Set password")),
            body(t("口令用于本机加密你的钱包，每次解锁需输入。口令无法找回，请牢记。",
                   "The password encrypts your wallet on this device and is required to unlock. It cannot be recovered.")),
            p1, p2, save
        )))
    }
}

// ===== 解锁 =====
class UnlockActivity : BaseActivity() {
    override fun onCreate(s: Bundle?) {
        super.onCreate(s)
        val pw = field(t("keystore 口令", "keystore password"), true)
        val unlock = primaryButton(t("解锁", "Unlock")) {
            val ks = KC.load(this) ?: run { goRoot(SetupActivity::class.java); return@primaryButton }
            val info = NativeCore.walletInfo(ks, pw.text.toString(), "", Session.net)
            if (info.startsWith("BTC")) {
                Session.ks = ks; Session.password = pw.text.toString(); goRoot(HomeActivity::class.java)
            } else alert(t("解锁失败: ", "Unlock failed: ") + info)
        }
        val markWrap = LinearLayout(this).apply {
            gravity = Gravity.CENTER
            addView(brandMark(80), LinearLayout.LayoutParams(dp(80), dp(80)))
        }
        setContentView(rootColumn(listOf(
            settingsLink(), markWrap,
            title(t("解锁钱包", "Unlock wallet")),
            body(t("输入口令解锁本机加密钱包。", "Enter your password to unlock this device's wallet.")),
            pw, unlock
        )))
    }
}

// ===== 主页 =====
class HomeActivity : BaseActivity() {
    override fun onCreate(s: Bundle?) {
        super.onCreate(s)
        render()
    }
    override fun onResume() { super.onResume(); render() }
    private fun render() {
        val ks = Session.ks ?: run { goRoot(UnlockActivity::class.java); return }
        val info = NativeCore.walletInfo(ks, Session.password, Session.passphrase, Session.net)
        var btc = "—"; var eth = "—"
        info.split("\n").forEach {
            if (it.startsWith("BTC:")) btc = it.removePrefix("BTC:").trim()
            if (it.startsWith("ETH:")) eth = it.removePrefix("ETH:").trim()
        }
        val isTest = Session.net != 0
        val hdr = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL
            addView(brandRow(t("离线签名机", "Air-gapped signer")), LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
            addView(pill(if (isTest) t("测试网", "Testnet") else t("主网", "Mainnet"), if (isTest) Theme.warn else Theme.success))
            addView(TextView(this@HomeActivity).apply {
                text = "  •••"; setTextColor(Theme.brand); textSize = 20f; setTypeface(null, Typeface.BOLD)
                isClickable = true; setOnClickListener { startAct(SettingsActivity::class.java) }
            })
        }
        fun addrRow(chip: String, color: Int, addr: String) = LinearLayout(this).apply {
            orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL
            addView(TextView(this@HomeActivity).apply {
                text = " $chip "; setTextColor(Color.WHITE); setBackgroundColor(color); textSize = 12f; setTypeface(null, Typeface.BOLD)
            })
            addView(mono("  " + ellipsize(addr), 13f), LinearLayout.LayoutParams(0, WRAP_CONTENT, 1f))
            addView(TextView(this@HomeActivity).apply {
                text = t("复制", "Copy"); setTextColor(Theme.brand); textSize = 13f; setTypeface(null, Typeface.BOLD)
                isClickable = true; setOnClickListener { copy(addr) }
            })
        }
        val overview = card(listOf(sectionHeader(t("钱包地址", "Wallet addresses")),
            addrRow("BTC", Theme.brand, btc), addrRow("ETH", Theme.eth, eth), caption(netDesc(isTest))))
        setContentView(rootColumn(listOf(
            hdr, overview,
            primaryButton(t("扫码签名", "Scan to sign")) { startAct(ScanActivity::class.java) },
            outlineButton(t("手动输入交易", "Enter transaction")) { manualInput() },
            outlineButton(t("导出观察钱包", "Export watch-only")) { startAct(ExportActivity::class.java) }
        ), 14))
    }
    private fun manualInput() {
        val et = multiline(t("粘贴 ur:crypto-psbt / ur:eth-sign-request 或 base64 PSBT", "Paste ur:… or base64 PSBT"), 160)
        et.setText(NativeCore.sampleUnsigned())
        androidx.appcompat.app.AlertDialog.Builder(this).setView(et)
            .setPositiveButton(t("核对", "Review")) { _, _ ->
                val u = et.text.toString().trim()
                val sum = NativeCore.summarize(Session.net, if (L10n.isEn(this)) 1 else 0, u)
                if (sum.contains("==")) startActivity(Intent(this, VerifyActivity::class.java).putExtra("unsigned", u).putExtra("summary", sum))
                else alert(t("解析失败: ", "Parse failed: ") + sum)
            }.setNegativeButton(t("取消", "Cancel"), null).show()
    }
    private fun copy(s: String) {
        (getSystemService(CLIPBOARD_SERVICE) as android.content.ClipboardManager)
            .setPrimaryClip(android.content.ClipData.newPlainText("addr", s)); toast(t("已复制", "Copied"))
    }
}

// ===== 设置 =====
class SettingsActivity : BaseActivity() {
    override fun onCreate(s: Bundle?) {
        super.onCreate(s)
        val netHint = caption(netDesc(Session.net != 0))
        val netGroup = RadioGroup(this).apply {
            orientation = RadioGroup.HORIZONTAL
            addView(RadioButton(this@SettingsActivity).apply { text = t("主网", "Mainnet"); setTextColor(Theme.textPrimary); id = 1 })
            addView(RadioButton(this@SettingsActivity).apply { text = t("测试网", "Testnet"); setTextColor(Theme.textPrimary); id = 2 })
            check(if (Session.net == 0) 1 else 2)
            setOnCheckedChangeListener { _, id -> Session.setNet(this@SettingsActivity, if (id == 1) 0 else 1); netHint.text = netDesc(id != 1) }
        }
        val langGroup = RadioGroup(this).apply {
            orientation = RadioGroup.HORIZONTAL
            addView(RadioButton(this@SettingsActivity).apply { text = t("系统", "System"); setTextColor(Theme.textPrimary); id = 10 })
            addView(RadioButton(this@SettingsActivity).apply { text = "中"; setTextColor(Theme.textPrimary); id = 11 })
            addView(RadioButton(this@SettingsActivity).apply { text = "EN"; setTextColor(Theme.textPrimary); id = 12 })
            check(10 + L10n.langIndex(this@SettingsActivity))
            setOnCheckedChangeListener { _, id ->
                L10n.setLang(this@SettingsActivity, if (id == 11) "zh" else if (id == 12) "en" else null)
                goRoot(MainActivity::class.java)
            }
        }
        val items = mutableListOf<View>(
            backTitle(t("设置", "Settings")),
            sectionHeader(t("网络", "Network")), card(listOf(netGroup, netHint)),
            sectionHeader(t("语言", "Language")), card(listOf(langGroup)),
            sectionHeader(t("关于", "About")),
            card(listOf(body("btc-wallate · v1.0"), outlineButton(t("关于与免责声明", "About & disclaimer")) { startAct(AboutActivity::class.java) }))
        )
        if (KC.load(this) != null) {
            items.add(sectionHeader(t("危险区", "Danger zone")))
            items.add(card(listOf(
                body(t("删除本机加密钱包，需口令确认。请先备份助记词。", "Delete this device's wallet (password required). Back up first.")),
                outlineButton(t("重置钱包", "Reset wallet"), Theme.danger) { reset() }
            )))
        }
        setContentView(rootColumn(items))
    }
    private fun reset() {
        val pw = field(t("keystore 口令", "keystore password"), true)
        androidx.appcompat.app.AlertDialog.Builder(this)
            .setTitle(t("重置钱包", "Reset wallet")).setView(pw)
            .setPositiveButton(t("确定删除", "Delete")) { _, _ ->
                val ks = KC.load(this) ?: return@setPositiveButton
                val info = NativeCore.walletInfo(ks, pw.text.toString(), "", Session.net)
                if (info.startsWith("BTC")) { KC.clear(this); Session.lock(); goRoot(SetupActivity::class.java) }
                else alert(t("口令错误", "Wrong password"))
            }.setNegativeButton(t("取消", "Cancel"), null).show()
    }
}

// ===== 关于 =====
class AboutActivity : BaseActivity() {
    override fun onCreate(s: Bundle?) {
        super.onCreate(s)
        fun bullet(zh: String, en: String) = body("•  " + t(zh, en), Theme.textPrimary)
        setContentView(rootColumn(listOf(
            backTitle(t("关于", "About")),
            card(listOf(sectionHeader(t("这是什么", "What this is")),
                body(t("btc-wallate 是一个非托管、离线的比特币/以太坊交易签名器。只在本机签名，需配合联网观察钱包广播。",
                       "A non-custodial, offline BTC/ETH transaction signer. Signs locally; a watch-only wallet broadcasts."), Theme.textPrimary))),
            card(listOf(sectionHeader(t("隐私与安全", "Privacy & security")),
                bullet("助记词/私钥仅以加密形式存储在本机，永不离开设备、永不上传。", "Keys are stored encrypted on-device only; never leave the device."),
                bullet("App 全程离线，无任何网络请求，不收集、不传输任何数据。", "Runs fully offline; collects/transmits nothing."),
                bullet("建议使用时开启飞行模式，并离线备份助记词。", "Use in airplane mode; keep an offline backup."))),
            card(listOf(sectionHeader(t("合规声明", "Compliance")),
                bullet("非托管：本 App 不保管你的资金，也无法动用你的资产。", "Non-custodial: never holds or can move your funds."),
                bullet("不做币币兑换、不做法币出入金、不提供任何投资建议。", "No exchange, no fiat ramp, no investment advice."),
                bullet("风险由使用者自行承担；助记词丢失或泄露将导致资产永久损失。", "You bear all risk; a lost/leaked mnemonic means permanent loss."))
            )
        )))
    }
}

// ===== 扫码（CameraX + zxing 连续收帧）=====
class ScanActivity : BaseActivity() {
    private val frames = LinkedHashSet<String>()
    private var done = false
    private lateinit var status: TextView
    private lateinit var preview: androidx.camera.view.PreviewView
    private val reader = com.google.zxing.MultiFormatReader().apply {
        setHints(mapOf(com.google.zxing.DecodeHintType.POSSIBLE_FORMATS to listOf(com.google.zxing.BarcodeFormat.QR_CODE)))
    }
    private val exec = java.util.concurrent.Executors.newSingleThreadExecutor()

    override fun onCreate(s: Bundle?) {
        super.onCreate(s)
        preview = androidx.camera.view.PreviewView(this)
        status = TextView(this).apply {
            text = t("对准另一台设备上的二维码", "Point at the QR on the other device")
            setTextColor(Color.WHITE); setBackgroundColor(Color.parseColor("#99000000")); textSize = 14f
            setPadding(dp(16), dp(10), dp(16), dp(10)); gravity = Gravity.CENTER
        }
        val back = TextView(this).apply {
            text = t("‹ 返回", "‹ Back"); setTextColor(Color.WHITE); setBackgroundColor(Color.parseColor("#99000000"))
            textSize = 15f; setTypeface(null, Typeface.BOLD); setPadding(dp(14), dp(8), dp(14), dp(8))
            isClickable = true; setOnClickListener { finish() }
        }
        val root = FrameLayout(this).apply {
            addView(preview, FrameLayout.LayoutParams(MATCH_PARENT, MATCH_PARENT))
            addView(status, FrameLayout.LayoutParams(WRAP_CONTENT, WRAP_CONTENT, Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL).apply { bottomMargin = dp(48) })
            addView(back, FrameLayout.LayoutParams(WRAP_CONTENT, WRAP_CONTENT, Gravity.TOP or Gravity.START).apply { topMargin = statusBarHeight() + dp(8); leftMargin = dp(12) })
        }
        setContentView(root)
        if (ContextCompat.checkSelfPermission(this, Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED)
            requestPermissions(arrayOf(Manifest.permission.CAMERA), 1)
        else startCamera()
    }
    override fun onRequestPermissionsResult(rc: Int, p: Array<out String>, g: IntArray) {
        super.onRequestPermissionsResult(rc, p, g)
        if (g.isNotEmpty() && g[0] == PackageManager.PERMISSION_GRANTED) startCamera()
        else { toast(t("无法访问摄像头", "Camera unavailable")); finish() }
    }
    private fun startCamera() {
        val future = androidx.camera.lifecycle.ProcessCameraProvider.getInstance(this)
        future.addListener({
            val provider = future.get()
            val prev = androidx.camera.core.Preview.Builder().build().also { it.setSurfaceProvider(preview.surfaceProvider) }
            val analysis = androidx.camera.core.ImageAnalysis.Builder()
                .setBackpressureStrategy(androidx.camera.core.ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST).build()
            analysis.setAnalyzer(exec) { proxy -> decode(proxy) }
            provider.unbindAll()
            provider.bindToLifecycle(this, androidx.camera.core.CameraSelector.DEFAULT_BACK_CAMERA, prev, analysis)
        }, ContextCompat.getMainExecutor(this))
    }
    private fun decode(proxy: androidx.camera.core.ImageProxy) {
        try {
            val buf = proxy.planes[0].buffer
            val data = ByteArray(buf.remaining()); buf.get(data)
            val src = com.google.zxing.PlanarYUVLuminanceSource(data, proxy.planes[0].rowStride, proxy.height, 0, 0, proxy.width, proxy.height, false)
            val bmp = com.google.zxing.BinaryBitmap(com.google.zxing.common.HybridBinarizer(src))
            val text = try { reader.decodeWithState(bmp).text } catch (e: Exception) { null }
            if (text != null) runOnUiThread { onText(text) }
        } catch (e: Exception) {
        } finally { proxy.close() }
    }
    private fun onText(text: String) {
        if (done) return
        frames.add(text)
        val joined = frames.joinToString("\n")
        val sum = NativeCore.summarize(Session.net, if (L10n.isEn(this)) 1 else 0, joined)
        status.text = String.format(t("已收 %d 帧…", "Got %d frames…"), frames.size)
        if (sum.contains("==")) {
            done = true
            startActivity(Intent(this, VerifyActivity::class.java).putExtra("unsigned", joined).putExtra("summary", sum))
            finish()
        }
    }
    override fun onDestroy() { super.onDestroy(); exec.shutdown() }
}

// ===== 核对 + 指纹 + 签名 =====
class VerifyActivity : BaseActivity() {
    override fun onCreate(s: Bundle?) {
        super.onCreate(s)
        val unsigned = intent.getStringExtra("unsigned") ?: ""
        val summary = intent.getStringExtra("summary") ?: ""
        val warn = TextView(this).apply {
            text = t("⚠︎ 请逐项核对收款地址与金额，防止被入侵设备偷换。", "⚠︎ Verify each recipient and amount — a compromised device may swap them.")
            setTextColor(Theme.bg); setBackgroundColor(Theme.warn); textSize = 13f; setTypeface(null, Typeface.BOLD)
            setPadding(dp(14), dp(12), dp(14), dp(12))
        }
        val receipt = card(listOf(mono(summary, 13f)))
        val signBtn = primaryButton(t("指纹确认并签名", "Confirm with fingerprint & sign")) { authThenSign(unsigned) }
        setContentView(rootColumn(listOf(backTitle(t("核对交易", "Review")), warn, receipt, signBtn)))
    }
    private fun authThenSign(unsigned: String) {
        val can = BiometricManager.from(this).canAuthenticate(BiometricManager.Authenticators.BIOMETRIC_WEAK or BiometricManager.Authenticators.DEVICE_CREDENTIAL)
        if (can != BiometricManager.BIOMETRIC_SUCCESS) { doSign(unsigned); return }  // 无生物识别则直接签（对标 iOS fallback）
        val prompt = BiometricPrompt(this, ContextCompat.getMainExecutor(this), object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) { doSign(unsigned) }
        })
        prompt.authenticate(BiometricPrompt.PromptInfo.Builder()
            .setTitle(t("授权签名", "Authorize signing"))
            .setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_WEAK or BiometricManager.Authenticators.DEVICE_CREDENTIAL)
            .build())
    }
    private fun doSign(unsigned: String) {
        val ks = Session.ks ?: run { alert(t("未解锁", "Not unlocked")); return }
        val sig = NativeCore.sign(unsigned, ks, Session.password, Session.passphrase)
        if (sig.startsWith("ur:")) startActivity(Intent(this, ResultActivity::class.java).putExtra("ur", sig))
        else alert(t("签名失败: ", "Sign failed: ") + sig)
    }
}

// ===== 结果动画二维码 =====
class ResultActivity : BaseActivity() {
    private val handler = Handler(Looper.getMainLooper())
    private var runnable: Runnable? = null
    override fun onCreate(s: Bundle?) {
        super.onCreate(s)
        val ur = intent.getStringExtra("ur") ?: ""
        val frames = ur.split("\n")
        val imgs = frames.map { makeQR(it, dp(260)) }
        val iv = ImageView(this).apply { layoutParams = LinearLayout.LayoutParams(dp(260), dp(260)); }
        val wrap = LinearLayout(this).apply {
            gravity = Gravity.CENTER; setBackgroundColor(Color.WHITE); setPadding(dp(16), dp(16), dp(16), dp(16)); addView(iv)
        }
        iv.setImageBitmap(imgs.firstOrNull())
        if (imgs.size > 1) {
            var idx = 0
            runnable = object : Runnable {
                override fun run() { idx = (idx + 1) % imgs.size; iv.setImageBitmap(imgs[idx]); handler.postDelayed(this, 200) }
            }
            handler.postDelayed(runnable!!, 200)
        }
        val tip = body(if (frames.size > 1)
            String.format(t("动画二维码 · 共 %d 帧，用观察钱包对准保持直到收齐后广播。", "Animated QR · %d frames. Keep aimed until finished."), frames.size)
            else t("用观察钱包扫描此二维码广播交易。", "Scan with your watch-only wallet to broadcast.")).apply { gravity = Gravity.CENTER }
        setContentView(rootColumn(listOf(
            backTitle(t("签名结果", "Signature")), tip, wrap,
            outlineButton(t("复制文本", "Copy text")) {
                (getSystemService(CLIPBOARD_SERVICE) as android.content.ClipboardManager)
                    .setPrimaryClip(android.content.ClipData.newPlainText("ur", ur)); toast(t("已复制", "Copied"))
            },
            primaryButton(t("完成", "Done")) { goRoot(HomeActivity::class.java) }
        )))
    }
    override fun onDestroy() { super.onDestroy(); runnable?.let { handler.removeCallbacks(it) } }
}

// ===== 导出观察钱包 =====
class ExportActivity : BaseActivity() {
    private var coin = 0
    private lateinit var iv: ImageView
    private lateinit var hint: TextView
    private var payload = ""
    override fun onCreate(s: Bundle?) {
        super.onCreate(s)
        iv = ImageView(this)
        hint = caption("")
        val seg = RadioGroup(this).apply {
            orientation = RadioGroup.HORIZONTAL
            addView(RadioButton(this@ExportActivity).apply { text = "BTC"; setTextColor(Theme.textPrimary); id = 100 })
            addView(RadioButton(this@ExportActivity).apply { text = "ETH"; setTextColor(Theme.textPrimary); id = 101 })
            check(100)
            setOnCheckedChangeListener { _, id -> coin = if (id == 100) 0 else 1; refresh() }
        }
        val qrWrap = LinearLayout(this).apply {
            gravity = Gravity.CENTER; setBackgroundColor(Color.WHITE); setPadding(dp(16), dp(16), dp(16), dp(16))
            addView(iv, LinearLayout.LayoutParams(dp(240), dp(240)))
        }
        val copy = outlineButton(t("复制文本", "Copy text")) {
            (getSystemService(CLIPBOARD_SERVICE) as android.content.ClipboardManager)
                .setPrimaryClip(android.content.ClipData.newPlainText("wo", payload)); toast(t("已复制", "Copied"))
        }
        setContentView(rootColumn(listOf(
            backTitle(t("导出观察钱包", "Export watch-only")),
            body(t("用热钱包扫此二维码建立观察钱包。仅含账户公钥，不含私钥。", "Scan with your hot wallet to create a watch-only wallet. Public key only.")),
            card(listOf(seg)), qrWrap, hint, copy
        )))
        refresh()
    }
    private fun refresh() {
        val ks = Session.ks ?: run { goRoot(UnlockActivity::class.java); return }
        payload = NativeCore.exportAccount(coin, 0, Session.net, ks, Session.password, Session.passphrase)
        iv.setImageBitmap(makeQR(payload, dp(240)))
        hint.text = if (coin == 0)
            t("BTC 输出描述符 · 用 Sparrow / BlueWallet / Nunchuk 扫码或粘贴导入", "BTC descriptor · import in Sparrow / BlueWallet / Nunchuk")
        else t("ETH crypto-hdkey · 在 MetaMask「连接硬件钱包」中扫码", "ETH crypto-hdkey · scan in MetaMask “Connect Hardware Wallet”")
    }
}
