//! `crypto-psbt`（BCR-2020-006）：CBOR 顶层就是一个字节串，内容是 PSBT 的序列化字节。
//!
//! 与 Sparrow / Keystone / BlueWallet 等使用同一标准，故本签名机导入/导出的
//! crypto-psbt 二维码可与这些现成观察钱包互操作。

use bitcoin::psbt::Psbt;

use crate::{Error, Result};

/// 该 registry 类型的 UR 类型串。
pub const UR_TYPE: &str = "crypto-psbt";

/// 把 PSBT 编成 crypto-psbt 的 CBOR payload（一个 CBOR 字节串）。
pub fn to_cbor(psbt: &Psbt) -> Result<Vec<u8>> {
    let raw = psbt.serialize();
    let mut e = minicbor::Encoder::new(Vec::new());
    e.bytes(&raw).map_err(|e| Error::Cbor(e.to_string()))?;
    Ok(e.into_writer())
}

/// 从 crypto-psbt 的 CBOR payload 解出 PSBT。
pub fn from_cbor(cbor: &[u8]) -> Result<Psbt> {
    let mut d = minicbor::Decoder::new(cbor);
    let raw = d.bytes().map_err(|e| Error::Cbor(e.to_string()))?;
    Psbt::deserialize(raw).map_err(|e| Error::Protocol(format!("PSBT 反序列化失败: {e}")))
}

/// 便捷：PSBT → crypto-psbt 单帧 UR 字符串。
pub fn to_ur_single(psbt: &Psbt) -> Result<String> {
    Ok(super::encode_single(UR_TYPE, &to_cbor(psbt)?))
}

/// 便捷：PSBT → crypto-psbt 动画二维码分帧。
pub fn to_ur_parts(psbt: &Psbt, max_fragment_length: usize, parts: usize) -> Result<Vec<String>> {
    super::encode_parts(UR_TYPE, &to_cbor(psbt)?, max_fragment_length, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airgap::PartCollector;
    use bitcoin::{
        absolute::LockTime, transaction::Version, Amount, OutPoint, Sequence, Transaction, TxIn,
        TxOut, Txid, Witness,
    };
    use std::str::FromStr;

    fn sample_psbt() -> Psbt {
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
                script_pubkey: bitcoin::ScriptBuf::new(),
            }],
        };
        Psbt::from_unsigned_tx(tx).unwrap()
    }

    #[test]
    fn cbor_roundtrip_preserves_psbt() {
        let psbt = sample_psbt();
        let cbor = to_cbor(&psbt).unwrap();
        // CBOR 顶层应为字节串（major type 2，首字节高 3 位 = 0b010）。
        assert_eq!(cbor[0] >> 5, 2);
        let back = from_cbor(&cbor).unwrap();
        assert_eq!(back.serialize(), psbt.serialize());
    }

    #[test]
    fn full_ur_qr_roundtrip() {
        let psbt = sample_psbt();
        // 走完整的「多帧动画二维码 → 收帧重组 → 还原 PSBT」链路。
        let parts = to_ur_parts(&psbt, 60, 30).unwrap();
        let mut c = PartCollector::new();
        for p in &parts {
            if c.is_complete() {
                break;
            }
            c.receive(p).unwrap();
        }
        assert!(c.is_complete());
        assert_eq!(c.ur_type(), Some(UR_TYPE));
        let payload = c.payload().unwrap().unwrap();
        let back = from_cbor(&payload).unwrap();
        assert_eq!(back.serialize(), psbt.serialize());
    }
}
