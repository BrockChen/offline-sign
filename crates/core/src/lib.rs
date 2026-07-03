//! btc-wallate 离线签名机核心逻辑（无 IO、可单元测试）。
//!
//! 该 crate 只做纯计算：助记词管理、BTC/ETH 密钥派生、（后续）交易解码与签名、
//! 以及空气隙协议编解码。所有涉及资金安全的密码学一律使用经过审计的上游库
//! （rust-bitcoin / bip39 / secp256k1），本 crate 绝不自行实现密码学原语。

pub mod airgap;
pub mod btc;
pub mod derive;
pub mod eth;
pub mod keystore;
pub mod seed;

pub use derive::{Account, Coin};
pub use seed::Wallet;

/// core 层统一错误类型。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("助记词错误: {0}")]
    Mnemonic(#[from] bip39::Error),
    #[error("BIP32 派生错误: {0}")]
    Bip32(#[from] bitcoin::bip32::Error),
    #[error("secp256k1 错误: {0}")]
    Secp(#[from] bitcoin::secp256k1::Error),
    #[error("派生路径非法: {0}")]
    Path(String),
    #[error("UR 编解码错误: {0}")]
    Ur(String),
    #[error("CBOR 错误: {0}")]
    Cbor(String),
    #[error("空气隙协议格式错误: {0}")]
    Protocol(String),
    #[error("keystore 加解密错误: {0}")]
    Crypto(String),
}

pub type Result<T> = std::result::Result<T, Error>;
