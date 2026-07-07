//! `crypto-psbt`（BCR-2020-006）：CBOR 顶层就是一个字节串，内容是 PSBT 的原始字节。
//!
//! 固件版不解析 PSBT 结构（那在 `btc` 模块，Phase C.2），这里只做原始字节的 CBOR 包装/解包，
//! 与 x86 版线格式一致、可与 Sparrow/Nunchuk 互操作。

use crate::{Error, Result};

/// 该 registry 类型的 UR 类型串。
pub const UR_TYPE: &str = "crypto-psbt";

/// 原始 PSBT 字节 → crypto-psbt 的 CBOR payload（一个 CBOR 字节串）。
pub fn to_cbor(raw_psbt: &[u8]) -> Result<Vec<u8>> {
    let mut e = minicbor::Encoder::new(Vec::new());
    e.bytes(raw_psbt).map_err(|e| Error::Cbor(e.to_string()))?;
    Ok(e.into_writer())
}

/// crypto-psbt 的 CBOR payload → 原始 PSBT 字节。
pub fn from_cbor(cbor: &[u8]) -> Result<Vec<u8>> {
    let mut d = minicbor::Decoder::new(cbor);
    Ok(d.bytes().map_err(|e| Error::Cbor(e.to_string()))?.to_vec())
}
