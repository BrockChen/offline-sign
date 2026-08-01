//! 目标设备端到端探针：走和 iOS 完全相同的 `escore_*` C-ABI，验证
//! 导入 → 概览 → 签名 → 导出观察钱包 在真机（Android arm64 等）跑通且结果正确。
//! 期望：btc 首地址匹配官方向量、签名产出 ur:eth-signature、导出产出 ur:crypto-hdkey。

use esp_signer_core::ffi::{
    escore_export_account, escore_generate_mnemonic, escore_import_mnemonic, escore_sample_unsigned,
    escore_sign, escore_wallet_info,
};
use std::os::raw::c_char;

const PW: &[u8] = b"pw\0";
const PASS: &[u8] = b"\0";
// BIP-84 官方测试向量助记词（mainnet [0] = bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu）
const MNEMONIC: &[u8] = b"abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about\0";

/// 调用一个「写 out 缓冲」的 FFI，返回 (ret, 文本)。
unsafe fn call(f: impl FnOnce(*mut u8, usize) -> i32) -> (i32, String) {
    let mut b = vec![0u8; 16384];
    let n = f(b.as_mut_ptr(), b.len());
    let len = n.unsigned_abs() as usize;
    (n, String::from_utf8_lossy(&b[..len.min(b.len())]).to_string())
}

fn head(s: &str, n: usize) -> &str {
    &s[..n.min(s.len())]
}

fn main() {
    unsafe {
        let pw = PW.as_ptr() as *const c_char;
        let pass = PASS.as_ptr() as *const c_char;
        let m = MNEMONIC.as_ptr() as *const c_char;

        // 1) 生成一枚新助记词（验证 CSPRNG + BIP-39 生成）
        let (_, mnem12) = call(|o, c| escore_generate_mnemonic(12, o, c));
        println!("generate_12_words={}", mnem12.split_whitespace().count());

        // 2) 导入 → keystore（Argon2 + XChaCha20-Poly1305 加密）
        let mut ksbuf = vec![0u8; 16384];
        let kn = escore_import_mnemonic(m, pw, ksbuf.as_mut_ptr(), ksbuf.len());
        if kn <= 0 {
            println!("FAIL import ret={kn}");
            return;
        }
        let ks = &ksbuf[..kn as usize];
        println!("import_keystore_bytes={}", kn);

        // 3) 解锁 + 概览（派生 BTC/ETH 地址）
        let (wn, info) = call(|o, c| {
            escore_wallet_info(ks.as_ptr(), ks.len(), pw, pass, 0, o, c)
        });
        println!("wallet_info_ok={} [{}]", wn > 0, info.replace('\n', " | "));
        let btc_ok = info.contains("bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu");

        // 4) 取内置示例待签 → 签名（ETH eth-sign-request → eth-signature）
        let (_, sample) = call(|o, c| escore_sample_unsigned(o, c));
        let (sn, sig) = call(|o, c| {
            escore_sign(sample.as_ptr(), sample.len(), ks.as_ptr(), ks.len(), pw, pass, o, c)
        });
        let sign_ok = sn > 0 && sig.starts_with("ur:eth-signature/");
        println!("sign_ok={} sig={}", sign_ok, head(&sig, 44));

        // 5) 导出观察钱包（ETH crypto-hdkey / BTC 描述符）
        let (_, eth_hd) = call(|o, c| {
            escore_export_account(1, 0, 0, ks.as_ptr(), ks.len(), pw, pass, o, c)
        });
        let (_, btc_desc) = call(|o, c| {
            escore_export_account(0, 0, 0, ks.as_ptr(), ks.len(), pw, pass, o, c)
        });
        println!("export_eth={}", head(&eth_hd, 40));
        println!("export_btc={}", head(&btc_desc, 40));

        let all = btc_ok && sign_ok && wn > 0;
        println!("PROBE_ALL_OK={all}");
    }
}
