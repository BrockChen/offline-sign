//! 核心操作：把 CLI 命令翻译成对 `btc-wallate-core` 的调用。
//!
//! 这些函数不做任何交互提示（口令/确认由 `main.rs` 负责），因此可被单元测试直接覆盖。

use anyhow::{bail, Context};
use bitcoin::psbt::Psbt;
use bitcoin::Network;
use btc_wallate_core::airgap::{eth as eth_ur, psbt as psbt_ur};
use btc_wallate_core::{btc, derive, eth, keystore, seed::Wallet};

/// 一个待签任务：已从空气隙 payload 解析成结构化对象。
#[derive(Debug)]
pub enum SignJob {
    Psbt(Psbt),
    Eth(eth_ur::EthSignRequest),
}

/// 把 `(ur_type, payload)` 解析为待签任务。
pub fn parse_job(ur_type: &str, payload: &[u8]) -> anyhow::Result<SignJob> {
    match ur_type {
        psbt_ur::UR_TYPE => Ok(SignJob::Psbt(psbt_ur::from_cbor(payload)?)),
        eth_ur::SIGN_REQUEST_TYPE => Ok(SignJob::Eth(eth_ur::decode_sign_request(payload)?)),
        other => bail!("不支持的 UR 类型: {other}（本机仅签 crypto-psbt / eth-sign-request）"),
    }
}

/// 生成签名前**必须人工核对**的摘要文本（收款地址/金额/手续费）。
pub fn summarize(wallet: &Wallet, job: &SignJob) -> anyhow::Result<String> {
    let mut out = String::new();
    match job {
        SignJob::Psbt(p) => {
            let s = btc::summarize(wallet, p)?;
            out.push_str("== BTC 交易核对 (PSBT) ==\n");
            for (i, o) in s.outputs.iter().enumerate() {
                let addr = o.address.as_deref().unwrap_or("<无法解析脚本，警惕!>");
                let tag = if o.is_mine { " [找零/本钱包]" } else { "" };
                out.push_str(&format!(
                    "  输出#{i}: {} sat -> {addr}{tag}\n",
                    o.amount.to_sat()
                ));
            }
            out.push_str(&format!(
                "  发往外部合计: {} sat\n",
                s.spent_to_external.to_sat()
            ));
            match s.fee {
                Some(f) => out.push_str(&format!("  手续费: {} sat\n", f.to_sat())),
                None => out.push_str("  手续费: <无法计算，缺输入金额>\n"),
            }
        }
        SignJob::Eth(r) => {
            let s = eth::summarize_sign_request(r)?;
            out.push_str("== ETH 交易核对 (EIP-1559) ==\n");
            out.push_str(&format!("  chainId: {}\n", s.chain_id));
            out.push_str(&format!("  nonce: {}\n", s.nonce));
            out.push_str(&format!(
                "  收款: {}\n",
                s.to.as_deref().unwrap_or("<合约创建>")
            ));
            out.push_str(&format!("  金额: {} wei\n", s.value_wei));
            out.push_str(&format!(
                "  gasLimit: {}  maxFee: {} wei  maxPriority: {} wei\n",
                s.gas_limit, s.max_fee_per_gas, s.max_priority_fee_per_gas
            ));
            match &s.erc20_transfer {
                Some(t) => out.push_str(&format!(
                    "  [ERC-20 transfer] 代币合约: {}  收币: {}  数量: {}\n",
                    t.token_contract, t.recipient, t.amount
                )),
                None if s.data_len > 0 => out.push_str(&format!(
                    "  ⚠ 含 {} 字节未知 calldata（非标准 ERC-20 transfer），请谨慎，勿盲签\n",
                    s.data_len
                )),
                None => {}
            }
        }
    }
    Ok(out)
}

/// 对任务签名，返回 `(输出 ur_type, 输出 payload)`。调用方决定写文件还是显示二维码。
pub fn sign(wallet: &Wallet, job: &SignJob) -> anyhow::Result<(String, Vec<u8>)> {
    match job {
        SignJob::Psbt(p) => {
            let mut signed = p.clone();
            let n = btc::sign(wallet, &mut signed)?;
            if n == 0 {
                bail!("没有可签名的输入（该 PSBT 不含本钱包拥有的输入）");
            }
            Ok((psbt_ur::UR_TYPE.to_string(), psbt_ur::to_cbor(&signed)?))
        }
        SignJob::Eth(r) => {
            let sig = eth::sign_sign_request(wallet, r)?;
            Ok((
                eth_ur::SIGNATURE_TYPE.to_string(),
                eth_ur::encode_signature(&r.request_id, &sig)?,
            ))
        }
    }
}

/// 生成或校验助记词并加密成 keystore blob，返回 `(助记词短语, blob)`。
///
/// `mnemonic` 为 `Some` 时走恢复（校验校验和），为 `None` 时新生成 `words` 词助记词。
pub fn create_keystore(
    mnemonic: Option<&str>,
    words: usize,
    network: Network,
    ks_password: &str,
) -> anyhow::Result<(String, Vec<u8>)> {
    let phrase = match mnemonic {
        Some(m) => {
            // 校验助记词合法（校验和）。
            Wallet::from_mnemonic(m, "", network).context("助记词非法")?;
            m.to_string()
        }
        None => Wallet::generate(words, "", network)?.mnemonic_phrase(),
    };
    let blob = keystore::encrypt_mnemonic(&phrase, ks_password)?;
    Ok((phrase, blob))
}

/// 从 keystore blob 解密并构造钱包。
pub fn load_wallet(
    blob: &[u8],
    ks_password: &str,
    bip39_passphrase: &str,
    network: Network,
) -> anyhow::Result<Wallet> {
    let m = keystore::decrypt_mnemonic(blob, ks_password)?;
    Ok(Wallet::from_mnemonic(&m, bip39_passphrase, network)?)
}

/// 派生并返回接收地址（用于 `address` 命令）。
pub fn address(
    wallet: &Wallet,
    coin_is_btc: bool,
    account: u32,
    change: bool,
    index: u32,
) -> anyhow::Result<String> {
    if coin_is_btc {
        Ok(btc_address(wallet, account, change, index)?)
    } else {
        Ok(derive::eth_address(wallet, account, index)?)
    }
}

fn btc_address(w: &Wallet, account: u32, change: bool, index: u32) -> anyhow::Result<String> {
    let c = if change { 1 } else { 0 };
    Ok(derive::btc_address(w, account, c, index)?.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::{
        absolute::LockTime, transaction::Version, Address, Amount, OutPoint, Sequence, Transaction,
        TxIn, TxOut, Txid, Witness,
    };
    use std::str::FromStr;

    const TEST_JUNK: &str = "test test test test test test test test test test test junk";

    #[test]
    fn keystore_create_then_load_roundtrip() {
        let (phrase, blob) =
            create_keystore(Some(TEST_JUNK), 12, Network::Bitcoin, "pw123").unwrap();
        assert_eq!(phrase, TEST_JUNK);
        let w = load_wallet(&blob, "pw123", "", Network::Bitcoin).unwrap();
        // 载入后能派生出已知 ETH 地址。
        assert_eq!(
            address(&w, false, 0, false, 0).unwrap(),
            "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266"
        );
    }

    #[test]
    fn generated_wallet_words_and_encryption() {
        let (phrase, blob) = create_keystore(None, 24, Network::Testnet, "pw").unwrap();
        assert_eq!(phrase.split_whitespace().count(), 24);
        // 生成的助记词能解回、能载入。
        let w = load_wallet(&blob, "pw", "", Network::Testnet).unwrap();
        assert!(address(&w, true, 0, false, 0).unwrap().starts_with("tb1"));
    }

    // 构造一个「不含本钱包输入」的最简 PSBT：验证 parse_job/summarize 管线，
    // 并确认 sign 对无可签输入正确报错（ETH 端到端已在 core 充分覆盖）。
    fn minimal_psbt() -> Psbt {
        let dest = Address::from_str("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx")
            .unwrap()
            .require_network(Network::Testnet)
            .unwrap();
        let tx = Transaction {
            version: Version::TWO,
            lock_time: LockTime::ZERO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: Txid::from_str(
                        "0000000000000000000000000000000000000000000000000000000000000001",
                    )
                    .unwrap(),
                    vout: 0,
                },
                script_sig: Default::default(),
                sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(50_000),
                script_pubkey: dest.script_pubkey(),
            }],
        };
        Psbt::from_unsigned_tx(tx).unwrap()
    }

    #[test]
    fn psbt_parse_and_summarize_via_ops() {
        let w = Wallet::from_mnemonic(TEST_JUNK, "", Network::Testnet).unwrap();
        // 把 PSBT 包成 crypto-psbt payload（模拟从文件/二维码读入）。
        let payload = psbt_ur::to_cbor(&minimal_psbt()).unwrap();
        let job = parse_job(psbt_ur::UR_TYPE, &payload).unwrap();

        let summary = summarize(&w, &job).unwrap();
        assert!(summary.contains("BTC 交易核对"));
        assert!(summary.contains("50000 sat"));

        // 无本钱包输入 ⇒ 拒签并报错。
        let err = sign(&w, &job).unwrap_err();
        assert!(err.to_string().contains("没有可签名的输入"));
    }

    #[test]
    fn unsupported_ur_type_is_rejected() {
        let err = parse_job("crypto-account", b"\x00").unwrap_err();
        assert!(err.to_string().contains("不支持的 UR 类型"));
    }
}
