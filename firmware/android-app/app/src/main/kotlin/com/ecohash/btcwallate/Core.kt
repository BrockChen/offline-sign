package com.ecohash.btcwallate

import android.content.Context
import android.content.SharedPreferences
import android.graphics.Bitmap
import android.graphics.Color
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.os.Bundle
import android.text.InputType
import android.util.Base64
import android.view.Gravity
import android.view.View
import android.view.ViewGroup.LayoutParams.MATCH_PARENT
import android.view.ViewGroup.LayoutParams.WRAP_CONTENT
import android.widget.*
import androidx.appcompat.app.AppCompatActivity
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.MultiFormatWriter
import java.util.Locale

// ===== 设计令牌（深色·硬件钱包风）=====
object Theme {
    val bg = 0xFF0E1116.toInt()
    val card = 0xFF1A1F2A.toInt()
    val cardBorder = 0xFF2A3140.toInt()
    val textPrimary = 0xFFF5F7FA.toInt()
    val textSecond = 0xFF9AA4B2.toInt()
    val brand = 0xFFF7931A.toInt()
    val eth = 0xFF627EEA.toInt()
    val success = 0xFF2ECC71.toInt()
    val danger = 0xFFFF5A5F.toInt()
    val warn = 0xFFFFB020.toInt()
}
fun Context.dp(v: Int): Int = (v * resources.displayMetrics.density).toInt()
fun Context.statusBarHeight(): Int {
    val id = resources.getIdentifier("status_bar_height", "dimen", "android")
    return if (id > 0) resources.getDimensionPixelSize(id) else dp(24)
}
fun rounded(color: Int, radiusDp: Float, ctx: Context, strokeColor: Int? = null, strokeDp: Int = 1): GradientDrawable {
    val d = GradientDrawable()
    d.cornerRadius = radiusDp * ctx.resources.displayMetrics.density
    d.setColor(color)
    if (strokeColor != null) d.setStroke(ctx.dp(strokeDp), strokeColor)
    return d
}

// ===== i18n =====
object L10n {
    private fun cfg(ctx: Context): SharedPreferences = ctx.getSharedPreferences("cfg", Context.MODE_PRIVATE)
    fun isEn(ctx: Context): Boolean {
        val v = cfg(ctx).getString("lang", null)
        if (v != null) return v == "en"
        return Locale.getDefault().language != "zh"
    }
    fun setLang(ctx: Context, v: String?) {
        val e = cfg(ctx).edit(); if (v == null) e.remove("lang") else e.putString("lang", v); e.apply()
    }
    fun langIndex(ctx: Context): Int = when (cfg(ctx).getString("lang", null)) { "zh" -> 1; "en" -> 2; else -> 0 }
}
fun Context.t(zh: String, en: String): String = if (L10n.isEn(this)) en else zh

// ===== 会话 =====
object Session {
    var ks: ByteArray? = null
    var password = ""
    var passphrase = ""
    var net = 0 // 0 主网 1 测试网
    fun loadNet(ctx: Context) { net = ctx.getSharedPreferences("cfg", 0).getInt("net", 0) }
    fun setNet(ctx: Context, n: Int) { net = n; ctx.getSharedPreferences("cfg", 0).edit().putInt("net", n).apply() }
    fun lock() { ks = null; password = ""; passphrase = "" }
}

// ===== 加密存储（对标 iOS Keychain）=====
object KC {
    private fun prefs(ctx: Context): SharedPreferences {
        val master = MasterKey.Builder(ctx).setKeyScheme(MasterKey.KeyScheme.AES256_GCM).build()
        return EncryptedSharedPreferences.create(
            ctx, "keystore", master,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM
        )
    }
    fun save(ctx: Context, blob: ByteArray) {
        prefs(ctx).edit().putString("blob", Base64.encodeToString(blob, Base64.NO_WRAP)).apply()
    }
    fun load(ctx: Context): ByteArray? {
        val s = prefs(ctx).getString("blob", null) ?: return null
        return Base64.decode(s, Base64.NO_WRAP)
    }
    fun clear(ctx: Context) { prefs(ctx).edit().remove("blob").apply() }
}

// ===== 二维码（深色主题下白底）=====
fun makeQR(s: String, sizePx: Int): Bitmap? = try {
    val bits = MultiFormatWriter().encode(s, BarcodeFormat.QR_CODE, sizePx, sizePx, mapOf(EncodeHintType.MARGIN to 1))
    Bitmap.createBitmap(sizePx, sizePx, Bitmap.Config.RGB_565).also { bmp ->
        for (x in 0 until sizePx) for (y in 0 until sizePx)
            bmp.setPixel(x, y, if (bits.get(x, y)) Color.BLACK else Color.WHITE)
    }
} catch (e: Exception) { null }

// ===== 组件基类（对标 iOS BaseVC）=====
open class BaseActivity : AppCompatActivity() {

    fun lp(w: Int = MATCH_PARENT, h: Int = WRAP_CONTENT, topDp: Int = 0): LinearLayout.LayoutParams =
        LinearLayout.LayoutParams(w, h).apply { topMargin = dp(topDp) }

    fun title(s: String) = TextView(this).apply {
        text = s; setTextColor(Theme.textPrimary); textSize = 22f; typeface = Typeface.DEFAULT_BOLD
    }
    /** 顶部「返回」行 + 标题（非首页用；本机可能无可用系统返回键）。 */
    fun backTitle(s: String): LinearLayout = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        addView(TextView(this@BaseActivity).apply {
            text = t("‹ 返回", "‹ Back"); setTextColor(Theme.brand); textSize = 16f; setTypeface(null, Typeface.BOLD)
            isClickable = true; setPadding(0, dp(2), dp(8), dp(2)); setOnClickListener { finish() }
        })
        addView(title(s), lp(topDp = 8))
    }
    fun heading(s: String) = TextView(this).apply {
        text = s; setTextColor(Theme.textPrimary); textSize = 18f; setTypeface(null, Typeface.BOLD)
    }
    fun body(s: String, color: Int = Theme.textSecond) = TextView(this).apply {
        text = s; setTextColor(color); textSize = 15f
    }
    fun caption(s: String) = TextView(this).apply { text = s; setTextColor(Theme.textSecond); textSize = 12f }
    fun sectionHeader(s: String) = TextView(this).apply {
        text = s.uppercase(); setTextColor(Theme.textSecond); textSize = 12f; setTypeface(null, Typeface.BOLD)
    }
    fun mono(s: String, sizeSp: Float = 13f, color: Int = Theme.textPrimary) = TextView(this).apply {
        text = s; setTextColor(color); textSize = sizeSp; typeface = Typeface.MONOSPACE
    }

    fun primaryButton(s: String, onClick: () -> Unit) = TextView(this).apply {
        text = s; setTextColor(Color.WHITE); textSize = 17f; setTypeface(null, Typeface.BOLD)
        gravity = Gravity.CENTER; minHeight = dp(52); background = rounded(Theme.brand, 14f, this@BaseActivity)
        isClickable = true; setOnClickListener { onClick() }
    }
    fun outlineButton(s: String, color: Int = Theme.brand, onClick: () -> Unit) = TextView(this).apply {
        text = s; setTextColor(color); textSize = 16f; setTypeface(null, Typeface.BOLD)
        gravity = Gravity.CENTER; minHeight = dp(52)
        background = rounded(Color.TRANSPARENT, 14f, this@BaseActivity, color, 2)
        isClickable = true; setOnClickListener { onClick() }
    }
    fun setEnabledStyle(btn: TextView, ok: Boolean) {
        btn.isEnabled = ok
        btn.background = rounded(if (ok) Theme.brand else Theme.cardBorder, 14f, this)
        btn.setTextColor(if (ok) Color.WHITE else Theme.textSecond)
    }

    fun field(hint: String, secure: Boolean = false): EditText = EditText(this).apply {
        setHint(hint); setHintTextColor(Theme.textSecond); setTextColor(Theme.textPrimary)
        textSize = 15f; minHeight = dp(48)
        setPadding(dp(12), dp(10), dp(12), dp(10))
        background = rounded(Theme.card, 12f, this@BaseActivity, Theme.cardBorder, 1)
        if (secure) inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD
    }
    fun multiline(hint: String, heightDp: Int): EditText = EditText(this).apply {
        setHint(hint); setHintTextColor(Theme.textSecond); setTextColor(Theme.textPrimary)
        typeface = Typeface.MONOSPACE; textSize = 14f; gravity = Gravity.TOP or Gravity.START
        setPadding(dp(12), dp(12), dp(12), dp(12)); minimumHeight = dp(heightDp)
        background = rounded(Theme.card, 14f, this@BaseActivity, Theme.cardBorder, 1)
        inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_FLAG_MULTI_LINE
        setSingleLine(false)
    }
    fun pill(s: String, color: Int) = TextView(this).apply {
        text = "  $s  "; setTextColor(color); textSize = 12f; setTypeface(null, Typeface.BOLD)
        gravity = Gravity.CENTER; minHeight = dp(24)
        background = rounded(Color.TRANSPARENT, 11f, this@BaseActivity, color, 1)
    }
    /** 圆角卡片，纵向包一组子 view。 */
    fun card(children: List<View>, spacingDp: Int = 12): LinearLayout = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        background = rounded(Theme.card, 14f, this@BaseActivity, Theme.cardBorder, 1)
        setPadding(dp(16), dp(16), dp(16), dp(16))
        children.forEachIndexed { i, v -> addView(v, lp(topDp = if (i == 0) 0 else spacingDp)) }
    }
    /** 根：ScrollView 包一个纵向 LinearLayout（顶部对齐，左右 20dp 边距，元素间 spacingDp）。 */
    fun rootColumn(children: List<View>, spacingDp: Int = 16): ScrollView {
        val col = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            isFocusableInTouchMode = true   // 拿初始焦点，避免 EditText 自动弹键盘挡视图
            setPadding(dp(20), dp(20) + statusBarHeight(), dp(20), dp(40))
            children.forEachIndexed { i, v -> addView(v, lp(topDp = if (i == 0) 0 else spacingDp)) }
        }
        return ScrollView(this).apply { setBackgroundColor(Theme.bg); addView(col) }
    }
    fun toast(s: String) = Toast.makeText(this, s, Toast.LENGTH_SHORT).show()
    fun alert(msg: String) = androidx.appcompat.app.AlertDialog.Builder(this)
        .setMessage(msg).setPositiveButton(t("好", "OK"), null).show()

    /** 品牌横排：盾牌+₿ 由文字近似（阶段B先文字标，图标走 mipmap）。 */
    /** 品牌标记：橙色圆 + 白 B（避免 ₿ 字符在老设备字体缺失显示成方块）。 */
    fun brandMark(sizeDp: Int): TextView = TextView(this).apply {
        text = "B"; setTextColor(Color.WHITE); textSize = (sizeDp * 0.5f); setTypeface(null, Typeface.BOLD)
        gravity = Gravity.CENTER
        background = GradientDrawable().apply { shape = GradientDrawable.OVAL; setColor(Theme.brand) }
    }
    fun brandRow(subtitle: String): LinearLayout = LinearLayout(this).apply {
        orientation = LinearLayout.HORIZONTAL; gravity = Gravity.CENTER_VERTICAL
        val mark = brandMark(44)
        addView(mark, LinearLayout.LayoutParams(dp(44), dp(44)).apply { rightMargin = dp(12) })
        val texts = LinearLayout(this@BaseActivity).apply {
            orientation = LinearLayout.VERTICAL
            addView(TextView(this@BaseActivity).apply { text = "btc-wallate"; setTextColor(Theme.textPrimary); textSize = 22f; setTypeface(null, Typeface.BOLD) })
            addView(TextView(this@BaseActivity).apply { text = subtitle; setTextColor(Theme.textSecond); textSize = 13f })
        }
        addView(texts)
    }

    fun startAct(cls: Class<*>) = startActivity(android.content.Intent(this, cls))
}

/** 网络说明（对标 iOS netDesc；测试网仅标 Signet）。 */
fun Context.netDesc(isTest: Boolean): String = if (isTest)
    t("测试网 · BTC Signet（tb1）· ETH 地址同主网，按交易 chainId（如 Sepolia）",
      "Testnet · BTC Signet (tb1) · ETH same address as mainnet, by tx chainId (e.g. Sepolia)")
else t("主网 · BTC bc1 · ETH 以太坊主网", "Mainnet · BTC bc1 · ETH Ethereum mainnet")

/** 地址中段省略。 */
fun ellipsize(s: String, head: Int = 12, tail: Int = 8): String =
    if (s.length > head + tail + 1) s.take(head) + "…" + s.takeLast(tail) else s
