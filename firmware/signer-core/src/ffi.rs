//! 最小 C-ABI 导出，用于「设备运行时验证」与后续 JNI 封装的雏形。
//!
//! `escore_probe` 会真正走一遍 BIP-39→种子→BIP-84 派生，把地址写进调用方缓冲区，
//! 返回写入长度（负数为错误）。用它 dlopen 到 Android 4.4 上跑，确认纯 Rust 核心可用。

use crate::{btc_address, mnemonic_to_seed, Net};

/// Android 4.4(API 19) 的 libc 无 `dl_iterate_phdr`（API 21 才有），而 Rust std 的 backtrace 会引用它，
/// 导致 .so 在老设备上 dlopen 失败。我们只在 android 目标提供一个「空迭代」桩：backtrace 用不到、
/// 返回 0 无害，从而让 .so 自我满足该符号、可在 4.4 上加载。（API21+ 上会遮蔽系统实现，同样无害。）
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn dl_iterate_phdr(
    _callback: *const core::ffi::c_void,
    _data: *const core::ffi::c_void,
) -> core::ffi::c_int {
    0
}

const ABANDON: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

/// 派生 BIP-84 首地址写入 `out`（以 NUL 结尾）。返回地址字节数；<0 为错误。
///
/// # Safety
/// `out` 必须指向至少 `cap` 字节的可写缓冲区。
#[no_mangle]
pub unsafe extern "C" fn escore_probe(out: *mut u8, cap: usize) -> i32 {
    let seed = match mnemonic_to_seed(ABANDON, "") {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let addr = match btc_address(&seed, Net::Mainnet, 0, 0, 0) {
        Ok(a) => a,
        Err(_) => return -2,
    };
    let bytes = addr.as_bytes();
    if out.is_null() || cap < bytes.len() + 1 {
        return -3;
    }
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), out, bytes.len());
    *out.add(bytes.len()) = 0;
    bytes.len() as i32
}
