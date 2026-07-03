//! 助记词与主私钥。
//!
//! 一个 [`Wallet`] 由「BIP-39 助记词 + 可选 passphrase」确定，派生出一个 BIP-32
//! 主私钥；BTC 与 ETH 共用这同一个种子。
//!
//! 安全说明：此结构持有种子/主私钥等敏感材料，`Debug` 已被刻意屏蔽，避免误打日志。
//! 落盘加密（Argon2 + ChaCha20-Poly1305）在后续 `keystore` 模块实现，本模块只负责内存态。

use bip39::Mnemonic;
use bitcoin::bip32::Xpriv;
use bitcoin::secp256k1::{All, Secp256k1};
use bitcoin::Network;

use crate::Result;

/// 内存中的钱包：助记词 + 由其派生的主私钥。
pub struct Wallet {
    mnemonic: Mnemonic,
    seed: [u8; 64],
    master: Xpriv,
    network: Network,
    secp: Secp256k1<All>,
}

// 手动实现，绝不打印任何密钥材料。
impl std::fmt::Debug for Wallet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Wallet")
            .field("network", &self.network)
            .field("mnemonic", &"<redacted>")
            .field("seed", &"<redacted>")
            .finish()
    }
}

impl Wallet {
    /// 生成新的助记词（word_count 通常取 12 或 24）。使用系统 CSPRNG。
    pub fn generate(word_count: usize, passphrase: &str, network: Network) -> Result<Self> {
        let mnemonic = Mnemonic::generate(word_count)?;
        Self::from_mnemonic_inner(mnemonic, passphrase, network)
    }

    /// 从已有助记词恢复钱包（会校验 BIP-39 校验和）。
    pub fn from_mnemonic(phrase: &str, passphrase: &str, network: Network) -> Result<Self> {
        let mnemonic = Mnemonic::parse(phrase)?;
        Self::from_mnemonic_inner(mnemonic, passphrase, network)
    }

    fn from_mnemonic_inner(mnemonic: Mnemonic, passphrase: &str, network: Network) -> Result<Self> {
        let seed = mnemonic.to_seed(passphrase);
        let master = Xpriv::new_master(network, &seed)?;
        Ok(Self {
            mnemonic,
            seed,
            master,
            network,
            secp: Secp256k1::new(),
        })
    }

    /// 助记词短语（空格分隔）。仅在离线设备上、用户明确要求备份时展示。
    pub fn mnemonic_phrase(&self) -> String {
        self.mnemonic.to_string()
    }

    /// 原始 64 字节种子。
    pub fn seed(&self) -> &[u8; 64] {
        &self.seed
    }

    /// BIP-32 主私钥。
    pub fn master_xpriv(&self) -> &Xpriv {
        &self.master
    }

    /// 该钱包使用的比特币网络（决定地址 HRP，与 ETH 派生无关）。
    pub fn network(&self) -> Network {
        self.network
    }

    pub(crate) fn secp(&self) -> &Secp256k1<All> {
        &self.secp
    }
}
