package com.ecohash.btcwallate

import android.app.Activity
import android.graphics.Color
import android.os.Bundle
import android.widget.ScrollView
import android.widget.TextView

/**
 * 阶段 A 最小验证：调 NativeCore 全套 JNI（生成/导入/概览/签名/导出），
 * 屏幕打印结果，证明 Kotlin ↔ Rust 密钥核心在真机跑通、结果与 iOS 一致。
 * 阶段 B 再实现对标 iOS 的完整深色主题 UI。
 */
class MainActivity : Activity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val tv = TextView(this).apply {
            setPadding(48, 96, 48, 48)
            setTextColor(Color.parseColor("#F5F7FA"))
            typeface = android.graphics.Typeface.MONOSPACE
            textSize = 13f
        }
        val sb = StringBuilder("btc-wallate · Android native 自检\n\n")
        try {
            val mnem = NativeCore.generateMnemonic(12)
            sb.append("generate_12=").append(mnem.split(" ").size).append(" 词\n")

            val ks = NativeCore.importMnemonic(
                "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
                "pw"
            ) ?: error("import 失败")
            sb.append("keystore=").append(ks.size).append("B\n")

            sb.append(NativeCore.walletInfo(ks, "pw", "", 0)).append("\n")

            val sample = NativeCore.sampleUnsigned()
            val sig = NativeCore.sign(sample, ks, "pw", "")
            sb.append("sign=").append(sig.take(44)).append("…\n")

            sb.append("export_eth=").append(NativeCore.exportAccount(1, 0, 0, ks, "pw", "").take(36)).append("…\n")
            sb.append("export_btc=").append(NativeCore.exportAccount(0, 0, 0, ks, "pw", "").take(36)).append("…\n")

            val btcOk = NativeCore.walletInfo(ks, "pw", "", 0)
                .contains("bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu")
            sb.append("\n官方向量一致=").append(btcOk)
            sb.append("\n\nNATIVE_OK ✅（Rust 核心在本机跑通）")
        } catch (e: Throwable) {
            sb.append("\nERROR: ").append(e.message)
        }
        tv.text = sb.toString()
        setContentView(ScrollView(this).apply {
            setBackgroundColor(Color.parseColor("#0E1116"))
            addView(tv)
        })
    }
}
