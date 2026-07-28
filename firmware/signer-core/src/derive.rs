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

/// 通用：按 `m/...` 路径派生 32 字节私钥。
pub fn privkey(seed: &[u8], path: &str) -> Result<[u8; 32]> {
    let xprv = derive(seed, path)?;
    let bytes = xprv.private_key().to_bytes();
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

/// 通用：按 `m/...` 路径派生 33 字节压缩公钥。
pub fn pubkey_compressed(seed: &[u8], path: &str) -> Result<[u8; 33]> {
    Ok(compressed_pubkey(&derive(seed, path)?))
}

/// 派生 ETH 账户私钥的 32 字节原始值（`m/44'/60'/account'/0/index`），供签名用。
pub fn eth_privkey(seed: &[u8], account: u32, index: u32) -> Result<[u8; 32]> {
    privkey(seed, &format!("m/44'/60'/{}'/0/{}", account, index))
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

/// 账户级扩展公钥导出（供构造 crypto-hdkey / 输出描述符，均只含公钥，不含私钥）。
pub struct AccountExport {
    pub key_data: [u8; 33],   // 账户节点压缩公钥
    pub chain_code: [u8; 32], // 账户节点链码
    pub depth: u8,            // 账户节点深度（BIP-84/44 账户级为 3）
    pub parent_fp: u32,       // 账户节点父指纹
    pub child_number: u32,    // 账户节点 child number（含硬化位）
    pub master_fp: u32,       // 主密钥指纹（描述符 origin / keypath source）
}

/// 主密钥指纹（hash160(压缩主公钥) 前 4 字节 → u32）。
fn master_fingerprint(seed: &[u8]) -> Result<u32> {
    let m = XPrv::new(seed).map_err(|e| Error::Derive(format!("{e}")))?;
    let fp = m.public_key().fingerprint(); // [u8;4]
    Ok(u32::from_be_bytes(fp))
}

/// 账户节点路径：BTC `m/84'/coin'/account'`，ETH `m/44'/60'/account'`。
fn account_path(coin: Coin, account: u32, net: Net) -> String {
    match coin {
        Coin::Btc => format!("m/84'/{}'/{}'", net.btc_coin_type(), account),
        Coin::Eth => format!("m/44'/60'/{}'", account),
    }
}

/// 导出账户级公钥数据（不解私钥外泄，仅公钥/链码/指纹）。
pub fn account_export(seed: &[u8], coin: Coin, account: u32, net: Net) -> Result<AccountExport> {
    let xprv = derive(seed, &account_path(coin, account, net))?;
    let a = xprv.attrs();
    Ok(AccountExport {
        key_data: compressed_pubkey(&xprv),
        chain_code: a.chain_code,
        depth: a.depth,
        parent_fp: u32::from_be_bytes(a.parent_fingerprint),
        child_number: a.child_number.0,
        master_fp: master_fingerprint(seed)?,
    })
}

/// Base58（比特币字母表）。
fn base58(data: &[u8]) -> String {
    const A: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let zeros = data.iter().take_while(|&&b| b == 0).count();
    let mut digits: Vec<u8> = Vec::new();
    for &byte in data {
        let mut carry = byte as u32;
        for d in digits.iter_mut() {
            carry += (*d as u32) << 8;
            *d = (carry % 58) as u8;
            carry /= 58;
        }
        while carry > 0 {
            digits.push((carry % 58) as u8);
            carry /= 58;
        }
    }
    let mut s = String::with_capacity(zeros + digits.len());
    for _ in 0..zeros {
        s.push('1');
    }
    for &d in digits.iter().rev() {
        s.push(A[d as usize] as char);
    }
    s
}

/// Base58Check（附加 double-SHA256 校验和后 base58）。
fn base58check(payload: &[u8]) -> String {
    let h1 = Sha256::digest(payload);
    let h2 = Sha256::digest(h1);
    let mut v = payload.to_vec();
    v.extend_from_slice(&h2[..4]);
    base58(&v)
}

/// 序列化账户级扩展公钥为 `xpub`/`tpub`（BIP-32 78 字节 + base58check）。
fn account_xpub_base58(exp: &AccountExport, net: Net) -> String {
    let version: u32 = match net {
        Net::Mainnet => 0x0488_B21E, // xpub
        Net::Test => 0x0435_87CF,    // tpub
    };
    let mut p = Vec::with_capacity(78);
    p.extend_from_slice(&version.to_be_bytes());
    p.push(exp.depth);
    p.extend_from_slice(&exp.parent_fp.to_be_bytes());
    p.extend_from_slice(&exp.child_number.to_be_bytes());
    p.extend_from_slice(&exp.chain_code);
    p.extend_from_slice(&exp.key_data);
    base58check(&p)
}

/// BTC 观察钱包输出描述符（BIP-84 P2WPKH）：`wpkh([fp/84h/coinh/accounth]xpub/<0;1>/*)`。
/// 供 Sparrow / BlueWallet / Nunchuk / Bitcoin Core 建立 watch-only。
pub fn btc_descriptor(seed: &[u8], account: u32, net: Net) -> Result<String> {
    let exp = account_export(seed, Coin::Btc, account, net)?;
    let xpub = account_xpub_base58(&exp, net);
    Ok(format!(
        "wpkh([{:08x}/84h/{}h/{}h]{}/<0;1>/*)",
        exp.master_fp,
        net.btc_coin_type(),
        account,
        xpub
    ))
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

    // 账户级 xpub 手写 base58check 与 rust-bitcoin 逐字节一致（BIP-84 m/84'/0'/0'）。
    #[test]
    fn btc_account_xpub_matches_rust_bitcoin() {
        use bitcoin::bip32::{DerivationPath as BDP, Xpriv, Xpub};
        let seed = mnemonic_to_seed(ABANDON, "").unwrap();
        let exp = account_export(&seed, Coin::Btc, 0, Net::Mainnet).unwrap();
        let ours = account_xpub_base58(&exp, Net::Mainnet);
        let secp = bitcoin::secp256k1::Secp256k1::new();
        let root = Xpriv::new_master(bitcoin::Network::Bitcoin, &seed).unwrap();
        let path: BDP = "m/84'/0'/0'".parse().unwrap();
        let acct = root.derive_priv(&secp, &path).unwrap();
        let xpub = Xpub::from_priv(&secp, &acct);
        assert_eq!(ours, xpub.to_string(), "账户 xpub 应与 rust-bitcoin 一致");
    }

    #[test]
    fn btc_descriptor_shape() {
        let seed = mnemonic_to_seed(ABANDON, "").unwrap();
        let d = btc_descriptor(&seed, 0, Net::Mainnet).unwrap();
        assert!(d.starts_with("wpkh([") && d.contains("]xpub") && d.ends_with("/<0;1>/*)"), "描述符格式: {d}");
        let t = btc_descriptor(&seed, 0, Net::Test).unwrap();
        assert!(t.contains("]tpub"), "测试网应为 tpub: {t}");
    }
}
