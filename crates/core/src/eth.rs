//! ETH 交易解析与签名（EIP-1559 类型交易）。
//!
//! 离线签名机对一笔以太坊交易做两件事：
//! 1. [`summarize`]：解析出 chainId / nonce / 收款地址 / 金额 / gas，并尝试解码 ERC-20
//!    `transfer`，供签名前在签名机屏幕上强制人工核对。
//! 2. [`sign`]：用派生私钥签名，产出可广播的 EIP-2718 原始交易。
//!
//! 签名与序列化全部委托给维护活跃的 alloy（避免旧库对 EIP-1559/2930/7702 交易可锻性
//! 校验缺失的问题，参见 CVE-2025-53359）。本模块不自实现任何密码学原语。

use alloy::consensus::{SignableTransaction, TxEip1559, TxEnvelope};
use alloy::eips::eip2718::Encodable2718;
use alloy::primitives::{Address, TxKind, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;

use crate::{derive, Error, Result, Wallet};

/// 解码出的 ERC-20 `transfer(address,uint256)` 调用。
#[derive(Debug, Clone)]
pub struct Erc20Transfer {
    /// 代币合约地址（即交易的 `to`），EIP-55。
    pub token_contract: String,
    /// 收币地址，EIP-55。
    pub recipient: String,
    /// 转账数量（最小单位，未按 decimals 换算）。
    pub amount: U256,
}

/// 一笔待签 ETH 交易的人类可读摘要。
#[derive(Debug, Clone)]
pub struct EthSummary {
    pub chain_id: u64,
    pub nonce: u64,
    /// 收款地址（合约创建时为 None），EIP-55。
    pub to: Option<String>,
    /// 转账 ETH（wei）。ERC-20 转账时此值为 0。
    pub value_wei: U256,
    pub gas_limit: u64,
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
    /// calldata 长度。
    pub data_len: usize,
    /// 若 calldata 是标准 ERC-20 transfer 则解出，否则 None（未知 data 需高危警告，勿盲签）。
    pub erc20_transfer: Option<Erc20Transfer>,
}

/// 签名结果。
#[derive(Debug, Clone)]
pub struct SignedEthTx {
    /// 可直接广播的原始交易（EIP-2718），带 `0x` 前缀。
    pub raw_tx_hex: String,
    /// 从签名恢复出的发送方地址（应与本钱包地址一致，用于自检）。
    pub signer: String,
}

const ERC20_TRANSFER_SELECTOR: [u8; 4] = [0xa9, 0x05, 0x9c, 0xbb];

fn tx_kind_to_addr(kind: &TxKind) -> Option<Address> {
    match kind {
        TxKind::Call(a) => Some(*a),
        TxKind::Create => None,
    }
}

/// 尝试把 calldata 解码为 ERC-20 transfer。
fn decode_erc20_transfer(token_contract: Option<Address>, data: &[u8]) -> Option<Erc20Transfer> {
    // transfer(address,uint256): 4 字节选择子 + 32 字节地址 + 32 字节金额 = 68 字节。
    if data.len() != 68 || data[0..4] != ERC20_TRANSFER_SELECTOR {
        return None;
    }
    // 地址在 32 字节槽的末 20 字节。
    let recipient = Address::from_slice(&data[16..36]);
    let amount = U256::from_be_slice(&data[36..68]);
    Some(Erc20Transfer {
        token_contract: token_contract?.to_checksum(None),
        recipient: recipient.to_checksum(None),
        amount,
    })
}

/// 解析 EIP-1559 交易，产出用于屏幕核对的摘要。
pub fn summarize(tx: &TxEip1559) -> EthSummary {
    let to_addr = tx_kind_to_addr(&tx.to);
    let erc20 = decode_erc20_transfer(to_addr, tx.input.as_ref());
    EthSummary {
        chain_id: tx.chain_id,
        nonce: tx.nonce,
        to: to_addr.map(|a| a.to_checksum(None)),
        value_wei: tx.value,
        gas_limit: tx.gas_limit,
        max_fee_per_gas: tx.max_fee_per_gas,
        max_priority_fee_per_gas: tx.max_priority_fee_per_gas,
        data_len: tx.input.len(),
        erc20_transfer: erc20,
    }
}

/// 用派生私钥对 EIP-1559 交易签名，返回可广播的原始交易。
pub fn sign(wallet: &Wallet, account: u32, index: u32, tx: TxEip1559) -> Result<SignedEthTx> {
    let sk = derive::eth_secret_bytes(wallet, account, index)?;
    let signer = PrivateKeySigner::from_slice(&sk).map_err(|e| Error::Path(e.to_string()))?;

    let sighash = tx.signature_hash();
    let sig = signer
        .sign_hash_sync(&sighash)
        .map_err(|e| Error::Path(e.to_string()))?;

    // 自检：从签名 + 签名哈希恢复发送方，应与本钱包派生地址一致；
    // 这同时证明签名对该交易哈希有效（非循环校验）。
    let recovered = sig
        .recover_address_from_prehash(&sighash)
        .map_err(|e| Error::Path(e.to_string()))?;

    let signed = tx.into_signed(sig);
    let envelope: TxEnvelope = signed.into();
    let raw = envelope.encoded_2718();

    Ok(SignedEthTx {
        raw_tx_hex: format!("0x{}", hex::encode(raw)),
        signer: recovered.to_checksum(None),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::derive::eth_address;
    use alloy::primitives::{address, Bytes};
    use bitcoin::Network;

    const TEST_JUNK: &str = "test test test test test test test test test test test junk";

    fn base_tx() -> TxEip1559 {
        TxEip1559 {
            chain_id: 11_155_111, // Sepolia
            nonce: 0,
            gas_limit: 21_000,
            max_fee_per_gas: 30_000_000_000,
            max_priority_fee_per_gas: 1_000_000_000,
            to: TxKind::Call(address!("70997970C51812dc3A010C7d01b50e0d17dc79C8")),
            value: U256::from(1_000_000_000_000_000_000u128), // 1 ETH
            input: Bytes::new(),
            access_list: Default::default(),
        }
    }

    #[test]
    fn summarize_plain_eth_transfer() {
        let tx = base_tx();
        let s = summarize(&tx);
        assert_eq!(s.chain_id, 11_155_111);
        assert_eq!(s.to.as_deref(), Some("0x70997970C51812dc3A010C7d01b50e0d17dc79C8"));
        assert_eq!(s.value_wei, U256::from(1_000_000_000_000_000_000u128));
        assert!(s.erc20_transfer.is_none());
    }

    #[test]
    fn summarize_decodes_erc20_transfer() {
        // transfer(0x7099...79C8, 1_000_000) 的 calldata。
        let mut data = Vec::new();
        data.extend_from_slice(&ERC20_TRANSFER_SELECTOR);
        data.extend_from_slice(&[0u8; 12]); // 地址左侧补零
        data.extend_from_slice(address!("70997970C51812dc3A010C7d01b50e0d17dc79C8").as_slice());
        let mut amt = [0u8; 32];
        amt[24..].copy_from_slice(&1_000_000u64.to_be_bytes());
        data.extend_from_slice(&amt);

        let mut tx = base_tx();
        tx.to = TxKind::Call(address!("dAC17F958D2ee523a2206206994597C13D831ec7")); // USDT 合约
        tx.value = U256::ZERO;
        tx.input = Bytes::from(data);

        let s = summarize(&tx);
        let t = s.erc20_transfer.expect("应解出 ERC-20 transfer");
        assert_eq!(t.token_contract, "0xdAC17F958D2ee523a2206206994597C13D831ec7");
        assert_eq!(t.recipient, "0x70997970C51812dc3A010C7d01b50e0d17dc79C8");
        assert_eq!(t.amount, U256::from(1_000_000u64));
    }

    #[test]
    fn sign_recovers_to_derived_address() {
        // 关键正确性：签名恢复出的发送方 == 由助记词派生的 ETH 地址。
        let w = Wallet::from_mnemonic(TEST_JUNK, "", Network::Bitcoin).unwrap();
        let expected = eth_address(&w, 0, 0).unwrap();
        let signed = sign(&w, 0, 0, base_tx()).unwrap();
        assert_eq!(signed.signer, expected);
        assert!(signed.raw_tx_hex.starts_with("0x02")); // EIP-1559 类型前缀 0x02
    }
}
