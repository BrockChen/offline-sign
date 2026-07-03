//! 助记词落盘加密（keystore）。
//!
//! 把 BIP-39 助记词用「口令派生密钥 + 认证加密」保护后存成一个自描述 blob：
//! - **Argon2id** 从口令 + 随机盐派生 32 字节密钥（抗暴力/抗 GPU）。
//! - **XChaCha20-Poly1305** 认证加密助记词，24 字节随机 nonce（无重用担忧），
//!   magic+version 作为 AAD 防降级/篡改。
//!
//! blob 布局（全部定长在前，密文在后）：
//! ```text
//! magic "BWKS"(4) | version(1) | salt(16) | nonce(24) | ciphertext(..)
//! ```
//!
//! 注意：这与 BIP-39 的 passphrase 是两个独立概念——keystore 口令保护磁盘上的助记词；
//! BIP-39 passphrase 在用助记词构造 [`crate::Wallet`] 时另行输入。二者可相同也可不同。
//! 助记词仍应有离线（金属板）物理备份，keystore 不是唯一备份。

use argon2::Argon2;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};

use crate::{Error, Result};

const MAGIC: &[u8; 4] = b"BWKS";
const VERSION: u8 = 1;
const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 24;
const KEY_LEN: usize = 32;
const HEADER_LEN: usize = 4 + 1 + SALT_LEN + NONCE_LEN;

fn os_random(buf: &mut [u8]) -> Result<()> {
    getrandom::getrandom(buf).map_err(|e| Error::Crypto(format!("获取随机数失败: {e}")))
}

/// 用 Argon2id 从口令 + 盐派生 32 字节密钥。
fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; KEY_LEN]> {
    let mut key = [0u8; KEY_LEN];
    Argon2::default()
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| Error::Crypto(format!("Argon2 派生失败: {e}")))?;
    Ok(key)
}

/// 加密助记词，返回可落盘的 blob。
// XNonce::from_slice 的弃用来自传递依赖 generic-array 未升级，功能正确，本地抑制。
#[allow(deprecated)]
pub fn encrypt_mnemonic(mnemonic: &str, password: &str) -> Result<Vec<u8>> {
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    os_random(&mut salt)?;
    os_random(&mut nonce)?;

    let key = derive_key(password, &salt)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| Error::Crypto(format!("初始化加密器失败: {e}")))?;

    // AAD = magic || version，绑定头部防止被改写/降级。
    let mut aad = Vec::with_capacity(5);
    aad.extend_from_slice(MAGIC);
    aad.push(VERSION);

    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: mnemonic.as_bytes(),
                aad: &aad,
            },
        )
        .map_err(|e| Error::Crypto(format!("加密失败: {e}")))?;

    let mut blob = Vec::with_capacity(HEADER_LEN + ciphertext.len());
    blob.extend_from_slice(MAGIC);
    blob.push(VERSION);
    blob.extend_from_slice(&salt);
    blob.extend_from_slice(&nonce);
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

/// 用口令解密 blob，还原助记词。口令错误 / blob 被篡改会返回错误。
#[allow(deprecated)]
pub fn decrypt_mnemonic(blob: &[u8], password: &str) -> Result<String> {
    if blob.len() < HEADER_LEN {
        return Err(Error::Crypto("keystore 数据过短".into()));
    }
    if &blob[0..4] != MAGIC {
        return Err(Error::Crypto("keystore magic 不匹配".into()));
    }
    let version = blob[4];
    if version != VERSION {
        return Err(Error::Crypto(format!("不支持的 keystore 版本: {version}")));
    }
    let salt = &blob[5..5 + SALT_LEN];
    let nonce = &blob[5 + SALT_LEN..HEADER_LEN];
    let ciphertext = &blob[HEADER_LEN..];

    let key = derive_key(password, salt)?;
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| Error::Crypto(format!("初始化加密器失败: {e}")))?;

    let mut aad = Vec::with_capacity(5);
    aad.extend_from_slice(MAGIC);
    aad.push(version);

    let plaintext = cipher
        .decrypt(
            XNonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| Error::Crypto("解密失败（口令错误或数据被篡改）".into()))?;

    String::from_utf8(plaintext).map_err(|_| Error::Crypto("助记词非合法 UTF-8".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn roundtrip_ok() {
        let blob = encrypt_mnemonic(M, "correct horse battery staple").unwrap();
        assert_eq!(&blob[0..4], MAGIC);
        assert_eq!(blob[4], VERSION);
        let back = decrypt_mnemonic(&blob, "correct horse battery staple").unwrap();
        assert_eq!(back, M);
    }

    #[test]
    fn wrong_password_fails() {
        let blob = encrypt_mnemonic(M, "right-pass").unwrap();
        let err = decrypt_mnemonic(&blob, "wrong-pass").unwrap_err();
        assert!(matches!(err, Error::Crypto(_)));
    }

    #[test]
    fn tamper_is_detected() {
        let mut blob = encrypt_mnemonic(M, "pw").unwrap();
        // 翻转密文最后一字节，认证应失败。
        let last = blob.len() - 1;
        blob[last] ^= 0x01;
        assert!(decrypt_mnemonic(&blob, "pw").is_err());
    }

    #[test]
    fn each_encryption_uses_fresh_salt_nonce() {
        let a = encrypt_mnemonic(M, "pw").unwrap();
        let b = encrypt_mnemonic(M, "pw").unwrap();
        assert_ne!(a, b, "相同明文+口令两次加密应因随机盐/nonce 而不同");
    }
}
