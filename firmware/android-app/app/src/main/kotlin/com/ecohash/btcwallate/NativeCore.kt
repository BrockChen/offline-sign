package com.ecohash.btcwallate

/** Rust 密钥核心的 JNI 绑定（libbtcwallate_jni.so，转发到与 iOS 相同的 escore_* C-ABI）。 */
object NativeCore {
    init { System.loadLibrary("btcwallate_jni") }

    /** 生成新助记词（words = 12/15/18/21/24）。 */
    external fun generateMnemonic(words: Int): String

    /** 校验助记词并加密成 keystore blob（Argon2 + XChaCha20-Poly1305）。 */
    external fun importMnemonic(mnemonic: String, password: String): ByteArray?

    /** 解锁并返回概览文本（BTC/ETH 地址）。net: 0=主网 1=测试网。 */
    external fun walletInfo(ks: ByteArray, password: String, passphrase: String, net: Int): String

    /** 解析待签数据产出屏幕核对文本。lang: 0=中 1=英。 */
    external fun summarize(net: Int, lang: Int, unsigned: String): String

    /** 解锁+签名，返回结果 UR（长交易为换行分隔多帧）。 */
    external fun sign(unsigned: String, ks: ByteArray, password: String, passphrase: String): String

    /** 导出观察钱包。coin: 0=BTC 描述符 / 1=ETH crypto-hdkey。 */
    external fun exportAccount(coin: Int, account: Int, net: Int, ks: ByteArray, password: String, passphrase: String): String

    /** 内置示例待签数据（Sepolia eth-sign-request），演示/自检用。 */
    external fun sampleUnsigned(): String
}
