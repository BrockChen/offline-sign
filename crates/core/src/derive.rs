//! 从主私钥派生 BTC 与 ETH 的密钥与地址。
//!
//! - BTC：BIP-84 原生 segwit，路径 `m/84'/coin'/account'/change/index`
//!   （coin' = 主网 0'、测试网/ signet 1'）。地址为 P2WPKH（bech32，`bc1...`）。
//! - ETH：BIP-44，路径 `m/44'/60'/account'/0/index`。地址为 keccak256(pubkey) 末 20 字节，
//!   并按 EIP-55 做大小写校验和。
//!
//! 账户级 xpub（`m/purpose'/coin'/account'`）用于导出到手机做「只读观察钱包」。

use bitcoin::bip32::{ChildNumber, DerivationPath, Xpub};
use bitcoin::{Address, CompressedPublicKey, Network};
use tiny_keccak::{Hasher, Keccak};

use crate::{Error, Result, Wallet};

/// 支持的币种。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coin {
    Btc,
    Eth,
}

impl Coin {
    /// BIP-44 `purpose'`：BTC 用 84'(BIP-84)，ETH 用 44'(BIP-44)。
    fn purpose(self) -> u32 {
        match self {
            Coin::Btc => 84,
            Coin::Eth => 44,
        }
    }

    /// SLIP-44 `coin_type'`。BTC 依网络区分主网(0)/测试网(1)，ETH 恒为 60。
    fn coin_type(self, network: Network) -> u32 {
        match self {
            Coin::Btc => match network {
                Network::Bitcoin => 0,
                _ => 1,
            },
            Coin::Eth => 60,
        }
    }
}

/// 一个账户的导出信息：账户级 xpub + 派生路径，供手机端建立观察钱包。
#[derive(Debug, Clone)]
pub struct Account {
    pub coin: Coin,
    pub account: u32,
    /// 账户级派生路径，如 `m/84'/0'/0'`。
    pub path: DerivationPath,
    /// 账户级扩展公钥。
    pub xpub: Xpub,
}

/// 构造账户级路径 `m/purpose'/coin'/account'`。
fn account_path(coin: Coin, network: Network, account: u32) -> Result<DerivationPath> {
    Ok(DerivationPath::from(vec![
        ChildNumber::from_hardened_idx(coin.purpose()).map_err(bip32_idx)?,
        ChildNumber::from_hardened_idx(coin.coin_type(network)).map_err(bip32_idx)?,
        ChildNumber::from_hardened_idx(account).map_err(bip32_idx)?,
    ]))
}

/// 构造完整地址路径 `m/purpose'/coin'/account'/change/index`。
fn address_path(
    coin: Coin,
    network: Network,
    account: u32,
    change: u32,
    index: u32,
) -> Result<DerivationPath> {
    Ok(DerivationPath::from(vec![
        ChildNumber::from_hardened_idx(coin.purpose()).map_err(bip32_idx)?,
        ChildNumber::from_hardened_idx(coin.coin_type(network)).map_err(bip32_idx)?,
        ChildNumber::from_hardened_idx(account).map_err(bip32_idx)?,
        ChildNumber::from_normal_idx(change).map_err(bip32_idx)?,
        ChildNumber::from_normal_idx(index).map_err(bip32_idx)?,
    ]))
}

fn bip32_idx(e: bitcoin::bip32::Error) -> Error {
    Error::Path(e.to_string())
}

/// 主私钥指纹（BIP-32 fingerprint），用于观察钱包描述符的 key origin。
pub fn master_fingerprint(wallet: &Wallet) -> bitcoin::bip32::Fingerprint {
    wallet.master_xpriv().fingerprint(wallet.secp())
}

/// 导出账户级扩展公钥（观察钱包用）。
pub fn account_xpub(wallet: &Wallet, coin: Coin, account: u32) -> Result<Account> {
    let path = account_path(coin, wallet.network(), account)?;
    let xpriv = wallet.master_xpriv().derive_priv(wallet.secp(), &path)?;
    let xpub = Xpub::from_priv(wallet.secp(), &xpriv);
    Ok(Account {
        coin,
        account,
        path,
        xpub,
    })
}

/// 派生 BTC BIP-84 (P2WPKH) 地址。
pub fn btc_address(
    wallet: &Wallet,
    account: u32,
    change: u32,
    index: u32,
) -> Result<Address> {
    let path = address_path(Coin::Btc, wallet.network(), account, change, index)?;
    let xpriv = wallet.master_xpriv().derive_priv(wallet.secp(), &path)?;
    let secp_pk = xpriv.private_key.public_key(wallet.secp());
    let cpk = CompressedPublicKey::from_slice(&secp_pk.serialize())?;
    Ok(Address::p2wpkh(&cpk, wallet.network()))
}

/// 派生 ETH 账户地址（EIP-55 校验和，带 `0x` 前缀）。
pub fn eth_address(wallet: &Wallet, account: u32, index: u32) -> Result<String> {
    let path = address_path(Coin::Eth, wallet.network(), account, /*change=*/ 0, index)?;
    let xpriv = wallet.master_xpriv().derive_priv(wallet.secp(), &path)?;
    let secp_pk = xpriv.private_key.public_key(wallet.secp());
    // 非压缩公钥为 65 字节，首字节 0x04 是前缀，去掉后对 64 字节做 keccak256。
    let uncompressed = secp_pk.serialize_uncompressed();
    let hash = keccak256(&uncompressed[1..]);
    let addr20 = &hash[12..]; // 末 20 字节
    Ok(to_eip55(addr20))
}

/// 派生 ETH 账户私钥的 32 字节原始值（交给 alloy 签名）。
///
/// 安全：调用方拿到后应尽快用于签名并让其离开作用域；不要打印/落盘。
pub fn eth_secret_bytes(wallet: &Wallet, account: u32, index: u32) -> Result<[u8; 32]> {
    let path = address_path(Coin::Eth, wallet.network(), account, /*change=*/ 0, index)?;
    let xpriv = wallet.master_xpriv().derive_priv(wallet.secp(), &path)?;
    Ok(xpriv.private_key.secret_bytes())
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut k = Keccak::v256();
    let mut out = [0u8; 32];
    k.update(data);
    k.finalize(&mut out);
    out
}

/// 按 EIP-55 生成带校验和的 `0x` 地址。
fn to_eip55(addr20: &[u8]) -> String {
    let lower = hex::encode(addr20); // 40 个小写十六进制字符
    let hash = keccak256(lower.as_bytes());
    let mut out = String::with_capacity(42);
    out.push_str("0x");
    for (i, c) in lower.chars().enumerate() {
        if c.is_ascii_digit() {
            out.push(c);
        } else {
            // 取对应 nibble：第 i 个字符对应 hash 的第 i 个 nibble。
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

    // BIP-39 全零熵对应的标准 24... 实为 12 词助记词。
    const ABANDON: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    // Hardhat/Anvil 默认助记词，ETH 测试向量广为人知。
    const TEST_JUNK: &str =
        "test test test test test test test test test test test junk";

    #[test]
    fn btc_bip84_first_address_matches_bip84_spec() {
        // BIP-84 测试向量：m/84'/0'/0'/0/0
        let w = Wallet::from_mnemonic(ABANDON, "", Network::Bitcoin).unwrap();
        let addr = btc_address(&w, 0, 0, 0).unwrap();
        assert_eq!(
            addr.to_string(),
            "bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu"
        );
        // 第二个接收地址 m/84'/0'/0'/0/1
        let addr1 = btc_address(&w, 0, 0, 1).unwrap();
        assert_eq!(
            addr1.to_string(),
            "bc1qnjg0jd8228aq7egyzacy8cys3knf9xvrerkf9g"
        );
        // 第一个找零地址 m/84'/0'/0'/1/0
        let chg = btc_address(&w, 0, 1, 0).unwrap();
        assert_eq!(
            chg.to_string(),
            "bc1q8c6fshw2dlwun7ekn9qwf37cu2rn755upcp6el"
        );
    }

    #[test]
    fn eth_bip44_first_address_matches_hardhat_vector() {
        // Hardhat 默认助记词 account 0：0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266
        let w = Wallet::from_mnemonic(TEST_JUNK, "", Network::Bitcoin).unwrap();
        let a0 = eth_address(&w, 0, 0).unwrap();
        assert_eq!(a0, "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266");
        // account index 1：0x70997970C51812dc3A010C7d01b50e0d17dc79C8
        let a1 = eth_address(&w, 0, 1).unwrap();
        assert_eq!(a1, "0x70997970C51812dc3A010C7d01b50e0d17dc79C8");
    }

    #[test]
    fn account_xpub_path_is_correct() {
        let w = Wallet::from_mnemonic(ABANDON, "", Network::Bitcoin).unwrap();
        let acc = account_xpub(&w, Coin::Btc, 0).unwrap();
        assert_eq!(acc.path.to_string(), "84'/0'/0'");
        let acc_eth = account_xpub(&w, Coin::Eth, 0).unwrap();
        assert_eq!(acc_eth.path.to_string(), "44'/60'/0'");
    }

    #[test]
    fn passphrase_changes_derivation() {
        let w0 = Wallet::from_mnemonic(ABANDON, "", Network::Bitcoin).unwrap();
        let w1 = Wallet::from_mnemonic(ABANDON, "TREZOR", Network::Bitcoin).unwrap();
        assert_ne!(
            btc_address(&w0, 0, 0, 0).unwrap().to_string(),
            btc_address(&w1, 0, 0, 0).unwrap().to_string()
        );
    }

    #[test]
    fn restore_is_deterministic() {
        let w = Wallet::generate(24, "", Network::Bitcoin).unwrap();
        let phrase = w.mnemonic_phrase();
        let w2 = Wallet::from_mnemonic(&phrase, "", Network::Bitcoin).unwrap();
        assert_eq!(
            eth_address(&w, 0, 0).unwrap(),
            eth_address(&w2, 0, 0).unwrap()
        );
    }
}
