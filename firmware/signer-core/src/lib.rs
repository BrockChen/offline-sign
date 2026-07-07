//! ESP32 固件的纯 Rust 密钥核心（Phase B）。
//!
//! 与 x86 版 `btc-wallate-core` 不同，这里**不用** C 版 secp256k1 / rust-bitcoin / alloy，
//! 改用可在嵌入式（esp-idf std）上编译的纯 Rust crate：`bip32`（k256 后端）、`k256`、
//! `sha2`、`ripemd`、`sha3`(keccak)、`bech32`。目标：与 x86 版产出**逐字节一致**的地址/密钥，
//! 用现有官方测试向量兜底。
//!
//! 用途限定学习/测试网/小额；非防篡改。

pub mod airgap;
pub mod derive;
pub mod eth;
pub mod seed;

pub use derive::{btc_address, eth_address, Coin, Net};
pub use seed::mnemonic_to_seed;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("助记词错误: {0}")]
    Mnemonic(String),
    #[error("派生错误: {0}")]
    Derive(String),
    #[error("编码错误: {0}")]
    Encode(String),
    #[error("签名错误: {0}")]
    Sign(String),
    #[error("UR 编解码错误: {0}")]
    Ur(String),
    #[error("CBOR 错误: {0}")]
    Cbor(String),
    #[error("空气隙协议格式错误: {0}")]
    Protocol(String),
}

pub type Result<T> = core::result::Result<T, Error>;
