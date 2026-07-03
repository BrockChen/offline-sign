//! ETH 交易解析与签名（EIP-1559 类型交易）。
//!
//! 离线签名机对一笔以太坊交易做两件事：
//! 1. [`summarize`]：解析出 chainId / nonce / 收款地址 / 金额 / gas，并尝试解码 ERC-20
//!    `transfer`，供签名前在签名机屏幕上强制人工核对。
//! 2. [`sign`]：用派生私钥签名，产出可广播的 EIP-2718 原始交易。
//!
//! 签名与序列化全部委托给维护活跃的 alloy（避免旧库对 EIP-1559/2930/7702 交易可锻性
//! 校验缺失的问题，参见 CVE-2025-53359）。本模块不自实现任何密码学原语。

use alloy::consensus::transaction::RlpEcdsaDecodableTx;
use alloy::consensus::{SignableTransaction, TxEip1559, TxEnvelope};
use alloy::eips::eip2718::Encodable2718;
use alloy::primitives::{keccak256, Address, TxKind, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy::signers::SignerSync;

use crate::airgap::eth as eth_ur;
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

// ---------- ERC-4527 端到端胶水 ----------
//
// 把空气隙层的 `eth-sign-request`（含待签交易字节 + 派生路径）接到解码/签名，
// 再产出 `eth-signature`，串起完整的离线签名闭环。

/// 从 EIP-1559 的 sign-data（`0x02 || rlp(未签名字段)`）解码出交易。
fn decode_eip1559_sign_data(sign_data: &[u8]) -> Result<TxEip1559> {
    let first = *sign_data
        .first()
        .ok_or_else(|| Error::Protocol("sign-data 为空".into()))?;
    if first != 0x02 {
        return Err(Error::Protocol(format!(
            "期望 EIP-1559 类型前缀 0x02，实际 0x{first:02x}"
        )));
    }
    let mut buf = &sign_data[1..];
    TxEip1559::rlp_decode(&mut buf).map_err(|e| Error::Protocol(format!("EIP-1559 解码失败: {e}")))
}

/// 解码签名请求里的待签交易，产出屏幕核对用的摘要。
///
/// 目前支持 EIP-1559 typed transaction（data-type=2）。其余类型（legacy/消息/typed-data）
/// 暂不解码——**不做盲签**，交由上层提示用户拒绝或谨慎处理。
pub fn summarize_sign_request(req: &eth_ur::EthSignRequest) -> Result<EthSummary> {
    match req.data_type {
        eth_ur::DataType::TypedTransaction => {
            let tx = decode_eip1559_sign_data(&req.sign_data)?;
            Ok(summarize(&tx))
        }
        other => Err(Error::Protocol(format!(
            "暂不支持解码 data-type {other:?} 做核对（拒绝盲签）"
        ))),
    }
}

/// 对签名请求签名：按请求内的派生路径取私钥，对 `keccak256(sign_data)` 签名，
/// 返回 65 字节 `r‖s‖v`（v 为 EIP-1559 的 y-parity，取值 0/1）。
///
/// 若请求带 `address`，会核对派生地址与之一致，防止路径/地址不匹配导致签错账户。
pub fn sign_sign_request(wallet: &Wallet, req: &eth_ur::EthSignRequest) -> Result<[u8; 65]> {
    let (account, index) = req.derivation.eth_account_index().ok_or_else(|| {
        Error::Protocol("派生路径不是标准 ETH 路径 m/44'/60'/a'/0/i".into())
    })?;
    let sk = derive::eth_secret_bytes(wallet, account, index)?;
    let signer = PrivateKeySigner::from_slice(&sk).map_err(|e| Error::Path(e.to_string()))?;

    if let Some(expected) = &req.address {
        if signer.address().as_slice() != expected.as_slice() {
            return Err(Error::Protocol(
                "请求指定的 address 与派生地址不一致，拒绝签名".into(),
            ));
        }
    }

    let hash = keccak256(&req.sign_data);
    let sig = signer
        .sign_hash_sync(&hash)
        .map_err(|e| Error::Path(e.to_string()))?;
    Ok(sig.as_rsy())
}

/// 端到端处理一个签名请求：产出（屏幕核对摘要, 回传给手机的 eth-signature 单帧 UR）。
///
/// 典型 GUI 用法：先用 [`summarize_sign_request`] 展示核对，用户确认后再调本函数签名。
/// 这里为方便一次性返回；GUI 可拆成两步以插入人工确认。
pub fn handle_sign_request(
    wallet: &Wallet,
    req: &eth_ur::EthSignRequest,
) -> Result<(EthSummary, String)> {
    let summary = summarize_sign_request(req)?;
    let sig = sign_sign_request(wallet, req)?;
    let ur = eth_ur::signature_to_ur_single(&req.request_id, &sig)?;
    Ok((summary, ur))
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

    // ---------- ERC-4527 端到端胶水测试 ----------

    use crate::airgap::eth as eth_ur;
    use crate::airgap::PartCollector;
    use alloy::primitives::{keccak256, Signature};

    // 构造一个「观察钱包」侧的 eth-sign-request：把未签名 EIP-1559 交易编成 sign-data。
    fn make_sign_request(w: &Wallet) -> eth_ur::EthSignRequest {
        let tx = base_tx();
        let sign_data = tx.encoded_for_signing(); // 0x02 || rlp(未签名字段)
        // sanity：sign-data 的 keccak 应等于交易签名哈希。
        assert_eq!(keccak256(&sign_data), tx.signature_hash());

        // 取本钱包 account 0 / index 0 的地址填入请求，测试地址核对路径。
        let sk = derive::eth_secret_bytes(w, 0, 0).unwrap();
        let addr = PrivateKeySigner::from_slice(&sk).unwrap().address();

        eth_ur::EthSignRequest {
            request_id: vec![0x42; 16],
            sign_data,
            data_type: eth_ur::DataType::TypedTransaction,
            chain_id: Some(11_155_111),
            derivation: eth_ur::KeyPath::eth_default(0, 0, None),
            address: Some(addr.as_slice().to_vec()),
        }
    }

    #[test]
    fn end_to_end_sign_request_to_signature() {
        let w = Wallet::from_mnemonic(TEST_JUNK, "", Network::Bitcoin).unwrap();
        let req = make_sign_request(&w);

        // 1. 屏幕核对摘要：应还原出交易字段。
        let summary = summarize_sign_request(&req).unwrap();
        assert_eq!(summary.chain_id, 11_155_111);
        assert_eq!(
            summary.to.as_deref(),
            Some("0x70997970C51812dc3A010C7d01b50e0d17dc79C8")
        );
        assert_eq!(summary.value_wei, U256::from(1_000_000_000_000_000_000u128));

        // 2. 签名：65 字节 r‖s‖v。
        let sig_bytes = sign_sign_request(&w, &req).unwrap();
        assert_eq!(sig_bytes.len(), 65);
        assert!(sig_bytes[64] <= 1, "v 应为 y-parity 0/1");

        // 3. 验签：从 (r,s,v) + 签名哈希恢复出的地址 == 本钱包派生地址。
        let expected = eth_address(&w, 0, 0).unwrap();
        let r = U256::from_be_slice(&sig_bytes[0..32]);
        let s = U256::from_be_slice(&sig_bytes[32..64]);
        let sig = Signature::new(r, s, sig_bytes[64] != 0);
        let hash = keccak256(&req.sign_data);
        let recovered = sig.recover_address_from_prehash(&hash).unwrap();
        assert_eq!(recovered.to_checksum(None), expected);
    }

    #[test]
    fn end_to_end_full_ur_loop() {
        let w = Wallet::from_mnemonic(TEST_JUNK, "", Network::Bitcoin).unwrap();
        let req = make_sign_request(&w);

        // 观察钱包 → 签名机：请求编成动画二维码并被收帧重组。
        let parts = eth_ur::sign_request_to_ur_parts(&req, 60, 40).unwrap();
        let mut c = PartCollector::new();
        for p in &parts {
            if c.is_complete() {
                break;
            }
            c.receive(p).unwrap();
        }
        assert_eq!(c.ur_type(), Some(eth_ur::SIGN_REQUEST_TYPE));
        let decoded = eth_ur::decode_sign_request(&c.payload().unwrap().unwrap()).unwrap();

        // 签名机处理：核对 + 签名 + 产出 eth-signature UR。
        let (_summary, sig_ur) = handle_sign_request(&w, &decoded).unwrap();
        assert!(sig_ur.starts_with("ur:eth-signature/"));

        // 签名机 → 观察钱包：解 eth-signature，request-id 必须原样带回。
        let (_, payload) = ur::decode(&sig_ur).unwrap();
        let (rid, sig65) = eth_ur::decode_signature(&payload).unwrap();
        assert_eq!(rid, req.request_id);

        // 该签名对原交易哈希有效，且恢复出本钱包地址。
        let sig = Signature::new(
            U256::from_be_slice(&sig65[0..32]),
            U256::from_be_slice(&sig65[32..64]),
            sig65[64] != 0,
        );
        let recovered = sig
            .recover_address_from_prehash(&keccak256(&req.sign_data))
            .unwrap();
        assert_eq!(recovered.to_checksum(None), eth_address(&w, 0, 0).unwrap());
    }

    #[test]
    fn sign_request_rejects_address_mismatch() {
        let w = Wallet::from_mnemonic(TEST_JUNK, "", Network::Bitcoin).unwrap();
        let mut req = make_sign_request(&w);
        req.address = Some(vec![0xff; 20]); // 故意填错地址
        let err = sign_sign_request(&w, &req).unwrap_err();
        assert!(matches!(err, Error::Protocol(_)));
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
