//! BIP-39 助记词 → 512 位种子（用 `bip39` crate，与 x86 版一致）。

use bip39::Mnemonic;

use crate::{Error, Result};

/// 校验助记词并派生 64 字节种子（可选 BIP-39 passphrase）。
pub fn mnemonic_to_seed(phrase: &str, passphrase: &str) -> Result<[u8; 64]> {
    let mnemonic = Mnemonic::parse(phrase).map_err(|e| Error::Mnemonic(format!("{e}")))?;
    Ok(mnemonic.to_seed(passphrase))
}
