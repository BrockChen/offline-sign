//! Android JNI 桥：Kotlin `NativeCore` 的 native 方法。
//! 内部全部转发到 esp-signer-core 的 `escore_*` C-ABI —— 与 iOS 完全同一套密码学。
//! 约定：文本 UTF-8；keystore 为二进制 ByteArray；unsigned/UR 以字符串传入。

use esp_signer_core::ffi::{
    escore_export_account, escore_generate_mnemonic, escore_import_mnemonic, escore_sample_unsigned,
    escore_sign, escore_summarize, escore_wallet_info,
};
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jbyteArray, jint, jstring};
use jni::JNIEnv;
use std::ffi::CString;
use std::os::raw::c_char;

/// 调用「写 out 缓冲」的 FFI，返回文本（成功/失败都取回文本，失败为错误信息）。
unsafe fn text_out(f: impl FnOnce(*mut u8, usize) -> i32) -> String {
    let mut b = vec![0u8; 16384];
    let n = f(b.as_mut_ptr(), b.len());
    let len = n.unsigned_abs() as usize;
    String::from_utf8_lossy(&b[..len.min(b.len())]).into_owned()
}
unsafe fn bytes_out(f: impl FnOnce(*mut u8, usize) -> i32) -> (bool, Vec<u8>) {
    let mut b = vec![0u8; 16384];
    let n = f(b.as_mut_ptr(), b.len());
    let len = n.unsigned_abs() as usize;
    b.truncate(len.min(16384));
    (n >= 0, b)
}
fn jstr(env: &mut JNIEnv, s: &JString) -> String {
    env.get_string(s).map(|x| x.into()).unwrap_or_default()
}
fn cstr(s: &str) -> CString {
    CString::new(s).unwrap_or_else(|_| CString::new("").unwrap())
}
fn out_string(env: &mut JNIEnv, s: String) -> jstring {
    env.new_string(s).map(|j| j.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_com_ecohash_btcwallate_NativeCore_generateMnemonic(
    mut env: JNIEnv, _c: JClass, words: jint,
) -> jstring {
    let s = unsafe { text_out(|o, cap| escore_generate_mnemonic(words as u8, o, cap)) };
    out_string(&mut env, s)
}

#[no_mangle]
pub extern "system" fn Java_com_ecohash_btcwallate_NativeCore_importMnemonic<'a>(
    mut env: JNIEnv<'a>, _c: JClass<'a>, mnemonic: JString<'a>, password: JString<'a>,
) -> jbyteArray {
    let m = cstr(&jstr(&mut env, &mnemonic));
    let p = cstr(&jstr(&mut env, &password));
    let (ok, blob) = unsafe {
        bytes_out(|o, cap| escore_import_mnemonic(m.as_ptr() as *const c_char, p.as_ptr() as *const c_char, o, cap))
    };
    if !ok {
        return std::ptr::null_mut();
    }
    env.byte_array_from_slice(&blob).map(|a| a.into_raw()).unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_com_ecohash_btcwallate_NativeCore_walletInfo<'a>(
    mut env: JNIEnv<'a>, _c: JClass<'a>, ks: JByteArray<'a>, password: JString<'a>, passphrase: JString<'a>, net: jint,
) -> jstring {
    let k = env.convert_byte_array(&ks).unwrap_or_default();
    let p = cstr(&jstr(&mut env, &password));
    let pp = cstr(&jstr(&mut env, &passphrase));
    let s = unsafe {
        text_out(|o, cap| escore_wallet_info(k.as_ptr(), k.len(), p.as_ptr() as *const c_char, pp.as_ptr() as *const c_char, net as u8, o, cap))
    };
    out_string(&mut env, s)
}

#[no_mangle]
pub extern "system" fn Java_com_ecohash_btcwallate_NativeCore_summarize<'a>(
    mut env: JNIEnv<'a>, _c: JClass<'a>, net: jint, lang: jint, unsigned: JString<'a>,
) -> jstring {
    let u = jstr(&mut env, &unsigned);
    let ub = u.as_bytes();
    let s = unsafe { text_out(|o, cap| escore_summarize(net as u8, lang as u8, ub.as_ptr(), ub.len(), o, cap)) };
    out_string(&mut env, s)
}

#[no_mangle]
pub extern "system" fn Java_com_ecohash_btcwallate_NativeCore_sign<'a>(
    mut env: JNIEnv<'a>, _c: JClass<'a>, unsigned: JString<'a>, ks: JByteArray<'a>, password: JString<'a>, passphrase: JString<'a>,
) -> jstring {
    let u = jstr(&mut env, &unsigned);
    let ub = u.as_bytes();
    let k = env.convert_byte_array(&ks).unwrap_or_default();
    let p = cstr(&jstr(&mut env, &password));
    let pp = cstr(&jstr(&mut env, &passphrase));
    let s = unsafe {
        text_out(|o, cap| escore_sign(ub.as_ptr(), ub.len(), k.as_ptr(), k.len(), p.as_ptr() as *const c_char, pp.as_ptr() as *const c_char, o, cap))
    };
    out_string(&mut env, s)
}

#[no_mangle]
pub extern "system" fn Java_com_ecohash_btcwallate_NativeCore_exportAccount<'a>(
    mut env: JNIEnv<'a>, _c: JClass<'a>, coin: jint, account: jint, net: jint, ks: JByteArray<'a>, password: JString<'a>, passphrase: JString<'a>,
) -> jstring {
    let k = env.convert_byte_array(&ks).unwrap_or_default();
    let p = cstr(&jstr(&mut env, &password));
    let pp = cstr(&jstr(&mut env, &passphrase));
    let s = unsafe {
        text_out(|o, cap| escore_export_account(coin as u8, account as u32, net as u8, k.as_ptr(), k.len(), p.as_ptr() as *const c_char, pp.as_ptr() as *const c_char, o, cap))
    };
    out_string(&mut env, s)
}

#[no_mangle]
pub extern "system" fn Java_com_ecohash_btcwallate_NativeCore_sampleUnsigned(
    mut env: JNIEnv, _c: JClass,
) -> jstring {
    let s = unsafe { text_out(|o, cap| escore_sample_unsigned(o, cap)) };
    out_string(&mut env, s)
}
