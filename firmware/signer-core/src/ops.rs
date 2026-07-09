//! 设备操作层：把「adb push 进来的待签文件 → 屏幕核对 → 签名 → 二维码输出」串起来。
//!
//! 这是 Android App（JNI）要调用的高层 API，全部无 IO 副作用（文件读写由 App 负责），可 host 测试。
//! 输入支持：`ur:` 文本（crypto-psbt / eth-sign-request）、原始二进制 PSBT、base64 PSBT。
//! 输出：签名结果编成 UR 帧，供小屏渲染二维码（BTC=crypto-psbt，ETH=eth-signature）。

use base64::Engine;

use crate::airgap::{self, eth as eth_ur};
use crate::derive::Net;
use crate::{btc, eth, keystore, seed, Error, Result};

/// 一笔待签任务。
pub enum Job {
    /// 原始 PSBT 字节。
    Psbt(Vec<u8>),
    /// ETH 签名请求。
    Eth(eth_ur::EthSignRequest),
}

/// 解密 keystore 并派生种子（`password` = keystore 口令，`passphrase` = BIP-39 第 25 词）。
pub fn unlock(keystore_blob: &[u8], password: &str, passphrase: &str) -> Result<[u8; 64]> {
    let mnemonic = keystore::decrypt_mnemonic(keystore_blob, password)?;
    seed::mnemonic_to_seed(&mnemonic, passphrase)
}

/// 解析 adb push 进来的待签数据（自动识别 ur / 二进制 PSBT / base64 PSBT）。
pub fn parse_unsigned(bytes: &[u8]) -> Result<Job> {
    if bytes.get(..3).is_some_and(|p| p.eq_ignore_ascii_case(b"ur:")) {
        let text = core::str::from_utf8(bytes).map_err(|_| Error::Protocol("UR 非 UTF-8".into()))?;
        let lines: Vec<String> = text
            .lines()
            .map(|l| l.trim().to_ascii_lowercase())
            .filter(|l| l.starts_with("ur:"))
            .collect();
        if lines.is_empty() {
            return Err(Error::Protocol("未找到 ur: 数据".into()));
        }
        let (ur_type, payload) = if lines.len() == 1 {
            airgap::decode_single(&lines[0])?
        } else {
            let mut c = airgap::PartCollector::new();
            for l in &lines {
                if c.is_complete() {
                    break;
                }
                c.receive(l)?;
            }
            if !c.is_complete() {
                return Err(Error::Protocol("UR 分片不完整".into()));
            }
            (
                c.ur_type().unwrap_or_default().to_string(),
                c.payload()?.ok_or_else(|| Error::Protocol("UR 重组为空".into()))?,
            )
        };
        match ur_type.as_str() {
            t if t == airgap::psbt::UR_TYPE => Ok(Job::Psbt(airgap::psbt::from_cbor(&payload)?)),
            t if t == eth_ur::SIGN_REQUEST_TYPE => {
                Ok(Job::Eth(eth_ur::decode_sign_request(&payload)?))
            }
            other => Err(Error::Protocol(format!("不支持的 UR 类型: {other}"))),
        }
    } else if bytes.starts_with(b"psbt\xff") {
        Ok(Job::Psbt(bytes.to_vec()))
    } else if let Ok(text) = core::str::from_utf8(bytes) {
        let raw = base64::engine::general_purpose::STANDARD
            .decode(text.trim())
            .map_err(|_| Error::Protocol("无法识别输入（非 ur / 二进制 PSBT / base64）".into()))?;
        if raw.starts_with(b"psbt\xff") {
            Ok(Job::Psbt(raw))
        } else {
            Err(Error::Protocol("base64 解出的不是 PSBT".into()))
        }
    } else {
        Err(Error::Protocol("无法识别输入".into()))
    }
}

/// 生成屏幕核对文本（必须人工比对收款地址/金额/手续费）。
pub fn summarize(net: Net, job: &Job) -> Result<String> {
    let mut out = String::new();
    match job {
        Job::Psbt(raw) => {
            let s = btc::summarize_psbt(net, raw)?;
            out.push_str("== BTC 交易核对 ==\n");
            for (i, (addr, val)) in s.outputs.iter().enumerate() {
                let a = addr.as_deref().unwrap_or("<非标准脚本, 谨慎>");
                out.push_str(&format!("#{i}: {val} sat -> {a}\n"));
            }
            match s.fee {
                Some(f) => out.push_str(&format!("手续费: {f} sat\n")),
                None => out.push_str("手续费: <未知, 缺输入金额>\n"),
            }
        }
        Job::Eth(req) => {
            let s = eth::summarize(req)?;
            out.push_str("== ETH 交易核对 ==\n");
            out.push_str(&format!("chainId: {}\n", s.chain_id));
            out.push_str(&format!("nonce: {}\n", s.nonce));
            match s.to {
                Some(t) => out.push_str(&format!("to: 0x{}\n", hex::encode(t))),
                None => out.push_str("to: <合约创建>\n"),
            }
            out.push_str(&format!("value: {} wei\n", s.value_wei));
            out.push_str(&format!(
                "gas: {} maxFee: {} wei\n",
                s.gas_limit, s.max_fee_per_gas
            ));
            if s.data_len > 0 {
                out.push_str(&format!("⚠ 含 {} 字节 calldata, 谨慎\n", s.data_len));
            }
        }
    }
    Ok(out)
}

/// 签名，返回 `(ur_type, payload)`。
pub fn sign(seed: &[u8], job: &Job) -> Result<(String, Vec<u8>)> {
    match job {
        Job::Psbt(raw) => {
            let signed = btc::sign_psbt(seed, raw)?;
            Ok((airgap::psbt::UR_TYPE.to_string(), airgap::psbt::to_cbor(&signed)?))
        }
        Job::Eth(req) => {
            let (rid, sig) = eth::sign_request(seed, req)?;
            Ok((
                eth_ur::SIGNATURE_TYPE.to_string(),
                eth_ur::encode_signature(&rid, &sig)?,
            ))
        }
    }
}

/// 便捷：签名并切成适配小屏的动画二维码帧（`frag` 单帧字节，`parts` 帧数）。
pub fn sign_to_ur_frames(
    seed: &[u8],
    job: &Job,
    frag: usize,
    parts: usize,
) -> Result<Vec<String>> {
    let (ur_type, payload) = sign(seed, job)?;
    airgap::encode_parts(&ur_type, &payload, frag, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mnemonic_to_seed;
    use bitcoin::bip32::{ChildNumber, DerivationPath};
    use bitcoin::psbt::{Input, Psbt};
    use bitcoin::secp256k1::Secp256k1;
    use bitcoin::{
        absolute::LockTime, transaction::Version, Address, Amount, Network, OutPoint, Sequence,
        Transaction, TxIn, TxOut, Txid, Witness,
    };
    use btc_wallate_core::seed::Wallet;
    use std::str::FromStr;

    const ABANDON: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    const TEST_JUNK: &str = "test test test test test test test test test test test junk";
    const REAL_REQUEST: &str = "a701d8255824327271317a337a6964693177367a6c69666d616b7878786a783536617a74666b3561383802583202f083aa36a7808316735f849ae84ffc825208946e6ebd1f18c3e6c1c2e2ef45dc83ec3724aa912f872386f26fc1000080c00304041a00aa36a705d90130a2018a182cf5183cf500f500f400f4021a2ed198a40654c6a234467b725b65dc7f598d853e6be2d3e1ffa00767696d546f6b656e";

    fn signable_psbt() -> Psbt {
        let w = Wallet::from_mnemonic(ABANDON, "", Network::Testnet).unwrap();
        let secp = Secp256k1::new();
        let path = DerivationPath::from(vec![
            ChildNumber::from_hardened_idx(84).unwrap(),
            ChildNumber::from_hardened_idx(1).unwrap(),
            ChildNumber::from_hardened_idx(0).unwrap(),
            ChildNumber::from_normal_idx(0).unwrap(),
            ChildNumber::from_normal_idx(0).unwrap(),
        ]);
        let xpriv = w.master_xpriv().derive_priv(&secp, &path).unwrap();
        let pk = bitcoin::PublicKey::new(xpriv.private_key.public_key(&secp));
        let fp = w.master_xpriv().fingerprint(&secp);
        let cpk = bitcoin::CompressedPublicKey::from_slice(&pk.inner.serialize()).unwrap();
        let my_spk = Address::p2wpkh(&cpk, Network::Testnet).script_pubkey();
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
                value: Amount::from_sat(90_000),
                script_pubkey: dest.script_pubkey(),
            }],
        };
        let mut psbt = Psbt::from_unsigned_tx(tx).unwrap();
        let mut input = Input {
            witness_utxo: Some(TxOut {
                value: Amount::from_sat(100_000),
                script_pubkey: my_spk,
            }),
            ..Default::default()
        };
        input.bip32_derivation.insert(pk.inner, (fp, path));
        psbt.inputs[0] = input;
        psbt
    }

    #[test]
    fn unlock_roundtrip() {
        let blob = keystore::encrypt_mnemonic(TEST_JUNK, "pw").unwrap();
        let seed = unlock(&blob, "pw", "").unwrap();
        assert_eq!(seed, mnemonic_to_seed(TEST_JUNK, "").unwrap());
    }

    #[test]
    fn btc_flow_from_binary_psbt() {
        // 模拟 adb push 进来的原始 .psbt 二进制。
        let seed = mnemonic_to_seed(ABANDON, "").unwrap();
        let bytes = signable_psbt().serialize();
        let job = parse_unsigned(&bytes).unwrap();

        let summary = summarize(Net::Test, &job).unwrap();
        assert!(summary.contains("90000 sat"));
        assert!(summary.contains("手续费: 10000 sat"));

        // 签名 → crypto-psbt UR 帧 → 解回应含 partial_sig。
        let frames = sign_to_ur_frames(&seed, &job, 90, 8).unwrap();
        assert!(!frames.is_empty());
        let mut c = airgap::PartCollector::new();
        for f in &frames {
            if c.is_complete() {
                break;
            }
            c.receive(f).unwrap();
        }
        let payload = c.payload().unwrap().unwrap();
        let signed = airgap::psbt::from_cbor(&payload).unwrap();
        let p = Psbt::deserialize(&signed).unwrap();
        assert!(!p.inputs[0].partial_sigs.is_empty());
    }

    #[test]
    fn eth_flow_from_ur_text() {
        // 模拟 adb push 进来的 ur:eth-sign-request 文本文件。
        let seed = mnemonic_to_seed(TEST_JUNK, "").unwrap();
        let payload = hex::decode(REAL_REQUEST).unwrap();
        let ur = airgap::encode_single(eth_ur::SIGN_REQUEST_TYPE, &payload);
        let job = parse_unsigned(ur.as_bytes()).unwrap();

        let summary = summarize(Net::Test, &job).unwrap();
        assert!(summary.contains("chainId: 11155111"));

        let (ty, pl) = sign(&seed, &job).unwrap();
        assert_eq!(ty, "eth-signature");
        let (_rid, sig) = eth_ur::decode_signature(&pl).unwrap();
        assert_eq!(sig.len(), 65);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_unsigned(b"\x00\x01\x02not a psbt").is_err());
    }
}
