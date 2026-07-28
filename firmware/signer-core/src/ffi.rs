//! C-ABI 导出，供 iOS(Swift) / 其它宿主调用。
//!
//! 约定：返回 `i32`——`>=0` 为成功、值是写入 `out` 的字节数；`<0` 为失败、`|ret|` 是写入 `out`
//! 的错误信息字节数。文本一律 UTF-8（不含 NUL）。调用方给足够大的 `out`（如 16 KiB）。
//! 私钥/种子只在函数内瞬时存在，不经 FFI 边界返回。

use std::ffi::CStr;
use std::os::raw::c_char;
use std::slice;

use crate::derive::Net;
use crate::{airgap, btc_address, derive, keystore, mnemonic_to_seed, ops, Result};

const ABANDON: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

// Android 4.4 缺 dl_iterate_phdr（std backtrace 引用），提供空桩以便 .so 在老设备加载。
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn dl_iterate_phdr(
    _callback: *const core::ffi::c_void,
    _data: *const core::ffi::c_void,
) -> core::ffi::c_int {
    0
}

unsafe fn cstr<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        ""
    } else {
        CStr::from_ptr(p).to_str().unwrap_or("")
    }
}

fn net_of(n: u8) -> Net {
    if n == 0 {
        Net::Mainnet
    } else {
        Net::Test
    }
}

fn write_raw(out: *mut u8, cap: usize, b: &[u8]) -> i32 {
    if out.is_null() {
        return 0;
    }
    let n = b.len().min(cap);
    unsafe { core::ptr::copy_nonoverlapping(b.as_ptr(), out, n) };
    b.len() as i32
}

fn finish(out: *mut u8, cap: usize, r: Result<Vec<u8>>) -> i32 {
    match r {
        Ok(b) => write_raw(out, cap, &b),
        Err(e) => -write_raw(out, cap, e.to_string().as_bytes()),
    }
}

/// 自检：派生 BIP-84 首地址（原探针，保留）。
#[no_mangle]
pub unsafe extern "C" fn escore_probe(out: *mut u8, cap: usize) -> i32 {
    let r = (|| -> Result<Vec<u8>> {
        let seed = mnemonic_to_seed(ABANDON, "")?;
        Ok(btc_address(&seed, Net::Mainnet, 0, 0, 0)?.into_bytes())
    })();
    finish(out, cap, r)
}

/// 返回一个示例 eth-sign-request 的 UR 文本（Sepolia，供模拟器/演示无摄像头时粘贴测试）。
#[no_mangle]
pub unsafe extern "C" fn escore_sample_unsigned(out: *mut u8, cap: usize) -> i32 {
    const SAMPLE: &str = "a701d8255824327271317a337a6964693177367a6c69666d616b7878786a783536617a74666b3561383802583202f083aa36a7808316735f849ae84ffc825208946e6ebd1f18c3e6c1c2e2ef45dc83ec3724aa912f872386f26fc1000080c00304041a00aa36a705d90130a2018a182cf5183cf500f500f400f4021a2ed198a40654c6a234467b725b65dc7f598d853e6be2d3e1ffa00767696d546f6b656e";
    let r = (|| -> Result<Vec<u8>> {
        let payload = hex::decode(SAMPLE)
            .map_err(|e| crate::Error::Protocol(format!("sample hex: {e}")))?;
        Ok(airgap::encode_single("eth-sign-request", &payload).into_bytes())
    })();
    finish(out, cap, r)
}

/// 校验助记词并加密成 keystore，**二进制** blob 写入 out。
#[no_mangle]
pub unsafe extern "C" fn escore_import_mnemonic(
    mnemonic: *const c_char,
    password: *const c_char,
    out: *mut u8,
    cap: usize,
) -> i32 {
    let r = (|| -> Result<Vec<u8>> {
        let m = cstr(mnemonic);
        mnemonic_to_seed(m, "")?; // 校验助记词
        keystore::encrypt_mnemonic(m, cstr(password))
    })();
    finish(out, cap, r)
}

/// 生成新的 BIP-39 助记词（words = 12/15/18/21/24），返回**明文**助记词字符串供用户离线备份。
/// 熵取自系统 CSPRNG（getrandom）。本函数不落盘——落盘由 `escore_import_mnemonic` 加密（与导入同构）。
#[no_mangle]
pub unsafe extern "C" fn escore_generate_mnemonic(words: u8, out: *mut u8, cap: usize) -> i32 {
    let r = (|| -> Result<Vec<u8>> {
        let entropy_len = match words {
            12 => 16,
            15 => 20,
            18 => 24,
            21 => 28,
            24 => 32,
            _ => return Err(crate::Error::Mnemonic("词数必须为 12/15/18/21/24".into())),
        };
        let mut entropy = vec![0u8; entropy_len];
        getrandom::getrandom(&mut entropy)
            .map_err(|e| crate::Error::Crypto(format!("随机数生成失败: {e}")))?;
        let m = bip39::Mnemonic::from_entropy(&entropy).map_err(|e| crate::Error::Mnemonic(format!("{e}")))?;
        Ok(m.to_string().into_bytes())
    })();
    finish(out, cap, r)
}

/// 解锁并返回钱包概览文本（BTC/ETH 地址），供确认加载的是否预期钱包。
#[no_mangle]
pub unsafe extern "C" fn escore_wallet_info(
    ks: *const u8,
    ks_len: usize,
    password: *const c_char,
    passphrase: *const c_char,
    net: u8,
    out: *mut u8,
    cap: usize,
) -> i32 {
    let r = (|| -> Result<Vec<u8>> {
        let seed = ops::unlock(slice::from_raw_parts(ks, ks_len), cstr(password), cstr(passphrase))?;
        let n = net_of(net);
        let btc = derive::btc_address(&seed, n, 0, 0, 0)?;
        let eth = derive::eth_address(&seed, 0, 0)?;
        Ok(format!("BTC: {btc}\nETH: {eth}").into_bytes())
    })();
    finish(out, cap, r)
}

/// 解析待签数据并产出屏幕核对文本（无需密钥）。
#[no_mangle]
pub unsafe extern "C" fn escore_summarize(
    net: u8,
    lang: u8,
    unsigned: *const u8,
    unsigned_len: usize,
    out: *mut u8,
    cap: usize,
) -> i32 {
    let r = (|| -> Result<Vec<u8>> {
        let job = ops::parse_unsigned(slice::from_raw_parts(unsigned, unsigned_len))?;
        Ok(ops::summarize(net_of(net), &job, lang == 1)?.into_bytes())
    })();
    finish(out, cap, r)
}

/// 解锁 + 解析 + 签名，返回签名结果的单帧 UR 文本（BTC=crypto-psbt / ETH=eth-signature）。
/// 种子只在本函数内存在。
#[no_mangle]
pub unsafe extern "C" fn escore_sign(
    unsigned: *const u8,
    unsigned_len: usize,
    ks: *const u8,
    ks_len: usize,
    password: *const c_char,
    passphrase: *const c_char,
    out: *mut u8,
    cap: usize,
) -> i32 {
    let r = (|| -> Result<Vec<u8>> {
        let seed = ops::unlock(slice::from_raw_parts(ks, ks_len), cstr(password), cstr(passphrase))?;
        let job = ops::parse_unsigned(slice::from_raw_parts(unsigned, unsigned_len))?;
        let (ty, payload) = ops::sign(&seed, &job)?;
        Ok(airgap::encode_single(&ty, &payload).into_bytes())
    })();
    finish(out, cap, r)
}

/// 导出**观察钱包**（只含账户级公钥、不含私钥）供热钱包扫码：
/// coin=0 BTC → 输出描述符字符串 `wpkh([fp/84h/coinh/accounth]xpub/<0;1>/*)`（Sparrow/BlueWallet/Nunchuk）；
/// coin=1 ETH → `ur:crypto-hdkey/...`（MetaMask「连接硬件钱包 → 扫码」）。
/// 需解锁（派生公钥需种子），种子仅在函数内瞬时存在。
#[no_mangle]
pub unsafe extern "C" fn escore_export_account(
    coin: u8,
    account: u32,
    net: u8,
    ks: *const u8,
    ks_len: usize,
    password: *const c_char,
    passphrase: *const c_char,
    out: *mut u8,
    cap: usize,
) -> i32 {
    let r = (|| -> Result<Vec<u8>> {
        let seed = ops::unlock(slice::from_raw_parts(ks, ks_len), cstr(password), cstr(passphrase))?;
        let n = net_of(net);
        if coin == 0 {
            Ok(derive::btc_descriptor(&seed, account, n)?.into_bytes())
        } else {
            let exp = derive::account_export(&seed, derive::Coin::Eth, account, n)?;
            let key = airgap::eth::AccountKey {
                key_data: exp.key_data,
                chain_code: exp.chain_code,
                components: vec![(44, true), (60, true), (account, true)],
                source_fingerprint: exp.master_fp,
                parent_fingerprint: exp.parent_fp,
                name: "btc-wallate".into(),
            };
            Ok(airgap::eth::hdkey_to_ur_single(&key)?.into_bytes())
        }
    })();
    finish(out, cap, r)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_JUNK: &str = "test test test test test test test test test test test junk\0";

    fn call(f: impl FnOnce(*mut u8, usize) -> i32) -> (i32, String) {
        let mut buf = vec![0u8; 16384];
        let n = f(buf.as_mut_ptr(), buf.len());
        let len = n.unsigned_abs() as usize;
        (n, String::from_utf8_lossy(&buf[..len.min(buf.len())]).to_string())
    }

    #[test]
    fn ffi_generate_mnemonic() {
        let (n, s) = call(|o, c| unsafe { escore_generate_mnemonic(24, o, c) });
        assert!(n > 0, "生成失败: {s}");
        assert_eq!(s.split_whitespace().count(), 24, "应为 24 词: {s}");
        crate::mnemonic_to_seed(&s, "").expect("生成的助记词应可校验派生");
        let (_, s2) = call(|o, c| unsafe { escore_generate_mnemonic(24, o, c) });
        assert_ne!(s, s2, "两次生成不应相同");
        // 非法词数
        let (bad, _) = call(|o, c| unsafe { escore_generate_mnemonic(13, o, c) });
        assert!(bad < 0, "词数 13 应报错");
    }

    #[test]
    fn ffi_export_account() {
        let pw = c"pw".as_ptr();
        let mut ksbuf = vec![0u8; 16384];
        let kn = unsafe {
            escore_import_mnemonic(TEST_JUNK.as_ptr() as *const c_char, pw, ksbuf.as_mut_ptr(), ksbuf.len())
        };
        let ks = &ksbuf[..kn as usize];
        // ETH → crypto-hdkey UR
        let (en, eth) = call(|o, c| unsafe {
            escore_export_account(1, 0, 0, ks.as_ptr(), ks.len(), pw, c"".as_ptr(), o, c)
        });
        assert!(en > 0 && eth.starts_with("ur:crypto-hdkey/"), "eth 导出: {eth}");
        // BTC → 输出描述符
        let (bn, btc) = call(|o, c| unsafe {
            escore_export_account(0, 0, 0, ks.as_ptr(), ks.len(), pw, c"".as_ptr(), o, c)
        });
        assert!(bn > 0 && btc.starts_with("wpkh([") && btc.contains("]xpub"), "btc 导出: {btc}");
    }

    #[test]
    fn ffi_import_then_sign_eth() {
        // import → keystore blob
        let pw = c"pw".as_ptr();
        let mut ksbuf = vec![0u8; 16384];
        let kn = unsafe {
            escore_import_mnemonic(TEST_JUNK.as_ptr() as *const c_char, pw, ksbuf.as_mut_ptr(), ksbuf.len())
        };
        assert!(kn > 0, "import failed: {kn}");
        let ks = &ksbuf[..kn as usize];

        // wallet_info 解锁概览
        let (wn, info) = call(|o, c| unsafe {
            escore_wallet_info(ks.as_ptr(), ks.len(), pw, c"".as_ptr(), 0, o, c)
        });
        assert!(wn > 0 && info.contains("ETH: 0x"), "info={info}");

        // 构造一个 eth-sign-request UR 文本作为待签输入
        let payload = hex::decode("a701d8255824327271317a337a6964693177367a6c69666d616b7878786a783536617a74666b3561383802583202f083aa36a7808316735f849ae84ffc825208946e6ebd1f18c3e6c1c2e2ef45dc83ec3724aa912f872386f26fc1000080c00304041a00aa36a705d90130a2018a182cf5183cf500f500f400f4021a2ed198a40654c6a234467b725b65dc7f598d853e6be2d3e1ffa00767696d546f6b656e").unwrap();
        let ur = crate::airgap::encode_single("eth-sign-request", &payload);

        // summarize（无需密钥）
        let (sn, sum) = call(|o, c| unsafe {
            escore_summarize(0, 0, ur.as_ptr(), ur.len(), o, c)
        });
        assert!(sn > 0 && sum.contains("chainId: 11155111"), "sum={sum}");

        // sign → eth-signature UR
        let (gn, sig) = call(|o, c| unsafe {
            escore_sign(ur.as_ptr(), ur.len(), ks.as_ptr(), ks.len(), pw, c"".as_ptr(), o, c)
        });
        assert!(gn > 0 && sig.starts_with("ur:eth-signature/"), "sig={sig}");
    }
}
