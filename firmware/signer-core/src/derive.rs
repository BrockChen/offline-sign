//! BTC BIP-84 / ETH BIP-44 派生与地址（纯 Rust，k256 后端）。

use bip32::{DerivationPath, XPrv};
use ripemd::Ripemd160;
use sha2::{Digest, Sha256};
use sha3::Keccak256;

use crate::{Error, Result};

/// 网络（决定 BTC 的 coin_type 与 bech32 HRP；ETH 不受影响）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Net {
    Mainnet,
    Test, // testnet / signet / regtest（同为 tb / coin_type 1）
}

impl Net {
    fn btc_coin_type(self) -> u32 {
        match self {
            Net::Mainnet => 0,
            Net::Test => 1,
        }
    }
    fn btc_hrp(self) -> &'static str {
        match self {
            Net::Mainnet => "bc",
            Net::Test => "tb",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coin {
    Btc,
    Eth,
}

fn derive(seed: &[u8], path: &str) -> Result<XPrv> {
    let path: DerivationPath = path.parse().map_err(|e| Error::Derive(format!("{e:?}")))?;
    XPrv::derive_from_path(seed, &path).map_err(|e| Error::Derive(format!("{e}")))
}

/// 压缩公钥（33 字节）。
fn compressed_pubkey(xprv: &XPrv) -> [u8; 33] {
    let vk = xprv.private_key().verifying_key();
    let pt = vk.to_encoded_point(true);
    let mut out = [0u8; 33];
    out.copy_from_slice(pt.as_bytes());
    out
}

/// 非压缩公钥去掉 0x04 前缀后的 64 字节（供 keccak）。
fn pubkey_xy(xprv: &XPrv) -> [u8; 64] {
    let vk = xprv.private_key().verifying_key();
    let pt = vk.to_encoded_point(false);
    let mut out = [0u8; 64];
    out.copy_from_slice(&pt.as_bytes()[1..65]);
    out
}

fn hash160(data: &[u8]) -> [u8; 20] {
    let sha = Sha256::digest(data);
    let rip = Ripemd160::digest(sha);
    let mut out = [0u8; 20];
    out.copy_from_slice(&rip);
    out
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(&Keccak256::digest(data));
    out
}

/// 派生 BTC BIP-84 (P2WPKH, bech32) 地址：`m/84'/coin'/account'/change/index`。
pub fn btc_address(
    seed: &[u8],
    net: Net,
    account: u32,
    change: u32,
    index: u32,
) -> Result<String> {
    let path = format!(
        "m/84'/{}'/{}'/{}/{}",
        net.btc_coin_type(),
        account,
        change,
        index
    );
    let xprv = derive(seed, &path)?;
    let program = hash160(&compressed_pubkey(&xprv));
    // bech32 segwit v0（witness version 0 + 20 字节 program）。
    bech32::segwit::encode_v0(bech32::Hrp::parse(net.btc_hrp()).unwrap(), &program)
        .map_err(|e| Error::Encode(format!("{e}")))
}

/// 派生 ETH BIP-44 地址（EIP-55 校验和，带 0x）：`m/44'/60'/account'/0/index`。
pub fn eth_address(seed: &[u8], account: u32, index: u32) -> Result<String> {
    let path = format!("m/44'/60'/{}'/0/{}", account, index);
    let xprv = derive(seed, &path)?;
    let hash = keccak256(&pubkey_xy(&xprv));
    Ok(to_eip55(&hash[12..]))
}

/// 按 EIP-55 生成带校验和的 `0x` 地址。
fn to_eip55(addr20: &[u8]) -> String {
    let lower = hex::encode(addr20);
    let hash = keccak256(lower.as_bytes());
    let mut out = String::with_capacity(42);
    out.push_str("0x");
    for (i, c) in lower.chars().enumerate() {
        if c.is_ascii_digit() {
            out.push(c);
        } else {
            let byte = hash[i / 2];
            let nibble = if i % 2 == 0 { byte >> 4 } else { byte & 0x0f };
            if nibble >= 8 {
                out.push(c.to_ascii_uppercase());
            } else {
                out.push(c);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::mnemonic_to_seed;

    const ABANDON: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const TEST_JUNK: &str = "test test test test test test test test test test test junk";

    // 与 x86 版 core 相同的 BIP-84 官方向量，验证纯 Rust 派生逐字节一致。
    #[test]
    fn btc_bip84_matches_spec_vector() {
        let seed = mnemonic_to_seed(ABANDON, "").unwrap();
        assert_eq!(
            btc_address(&seed, Net::Mainnet, 0, 0, 0).unwrap(),
            "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu"
        );
        assert_eq!(
            btc_address(&seed, Net::Mainnet, 0, 0, 1).unwrap(),
            "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g"
        );
        assert_eq!(
            btc_address(&seed, Net::Mainnet, 0, 1, 0).unwrap(),
            "bc1q8c6fshw2dlwun7ekn9qwf37cu2rn755upcp6el"
        );
    }

    // Hardhat 默认助记词 ETH 向量。
    #[test]
    fn eth_matches_hardhat_vector() {
        let seed = mnemonic_to_seed(TEST_JUNK, "").unwrap();
        assert_eq!(
            eth_address(&seed, 0, 0).unwrap(),
            "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
        );
        assert_eq!(
            eth_address(&seed, 0, 1).unwrap(),
            "0x70997970C51812dc3A010C7d01b50e0d17dc79C8"
        );
    }

    #[test]
    fn testnet_btc_is_tb() {
        let seed = mnemonic_to_seed(ABANDON, "").unwrap();
        assert!(btc_address(&seed, Net::Test, 0, 0, 0).unwrap().starts_with("tb1"));
    }

    #[test]
    fn passphrase_changes_result() {
        let a = mnemonic_to_seed(ABANDON, "").unwrap();
        let b = mnemonic_to_seed(ABANDON, "TREZOR").unwrap();
        assert_ne!(
            btc_address(&a, Net::Mainnet, 0, 0, 0).unwrap(),
            btc_address(&b, Net::Mainnet, 0, 0, 0).unwrap()
        );
    }
}
