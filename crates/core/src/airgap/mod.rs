//! 空气隙传输层：BC-UR 动画二维码分帧 + 各链的 CBOR registry 类型。
//!
//! 分两层：
//! - **UR 分帧**（本模块）：把一段 CBOR payload 用 fountain code 编成一串 `ur:<type>/...`
//!   字符串（可渲染为动画二维码），接收侧增量收帧后重组回 payload。基于 `ur` crate。
//! - **CBOR registry 类型**（子模块）：
//!   - [`psbt`]：`crypto-psbt`（BCR-2020-006），BTC。
//!   - [`eth`]：`eth-sign-request` / `eth-signature`（ERC-4527）+ `crypto-keypath`，ETH。
//!
//! 传输通道无关：同样的 payload 也可写入 U盘/SD 文件（`.ur`），本模块只管编解码。

pub mod eth;
pub mod psbt;

use crate::{Error, Result};

fn ur_err(e: ur::ur::Error) -> Error {
    Error::Ur(e.to_string())
}

/// 单帧编码：payload 足够小、一帧能装下时，返回一个 `ur:<type>/...` 字符串。
pub fn encode_single(ur_type: &str, payload: &[u8]) -> String {
    ur::encode(payload, &ur::Type::Custom(ur_type))
}

/// 多帧（动画二维码）编码：把 payload 用 fountain code 切成 `parts` 帧。
///
/// - `max_fragment_length`：单帧最大字节数（决定二维码密度，常用 100~200）。
/// - `parts`：生成的帧数；fountain 为无限流，`parts` 应 ≥ 纯分片数以保证接收侧可重组，
///   通常取「分片数的 1.5~2 倍」以增加抗丢帧能力。
pub fn encode_parts(
    ur_type: &str,
    payload: &[u8],
    max_fragment_length: usize,
    parts: usize,
) -> Result<Vec<String>> {
    let mut enc = ur::Encoder::new(payload, max_fragment_length, ur_type).map_err(ur_err)?;
    let n = parts.max(1);
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(enc.next_part().map_err(ur_err)?);
    }
    Ok(out)
}

/// 解码单帧 UR（`ur:<type>/<bytewords>`，无 fountain 索引），返回 `(ur_type, payload)`。
///
/// 多帧动画二维码请改用 [`PartCollector`]。若传入的其实是某个多帧分片，返回错误。
pub fn decode_single(part: &str) -> Result<(String, Vec<u8>)> {
    let (kind, payload) = ur::decode(part).map_err(ur_err)?;
    match kind {
        ur::ur::Kind::SinglePart => Ok((parse_type(part).unwrap_or_default(), payload)),
        ur::ur::Kind::MultiPart => Err(Error::Protocol(
            "这是多帧分片，单帧无法重组，请收齐后用 PartCollector".into(),
        )),
    }
}

/// 解析一帧的 UR 类型（`ur:<type>/...` → `<type>`，小写）。
pub fn parse_type(part: &str) -> Option<String> {
    part.strip_prefix("ur:")?
        .split('/')
        .next()
        .map(|s| s.to_ascii_lowercase())
}

/// 增量收帧器：喂入分片直到 [`is_complete`](Self::is_complete)，再取回原始 payload。
///
/// 同时记录首帧解析出的 UR 类型，供上层判断这是 crypto-psbt 还是 eth-sign-request。
pub struct PartCollector {
    decoder: ur::Decoder,
    ur_type: Option<String>,
}

impl Default for PartCollector {
    fn default() -> Self {
        Self {
            decoder: ur::Decoder::default(),
            ur_type: None,
        }
    }
}

impl PartCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// 收一帧。首帧会记录 UR 类型。
    pub fn receive(&mut self, part: &str) -> Result<()> {
        if self.ur_type.is_none() {
            self.ur_type = parse_type(part);
        }
        self.decoder.receive(part).map_err(ur_err)
    }

    /// 是否已收齐可重组。
    pub fn is_complete(&self) -> bool {
        self.decoder.complete()
    }

    /// 首帧解析出的 UR 类型。
    pub fn ur_type(&self) -> Option<&str> {
        self.ur_type.as_deref()
    }

    /// 取回重组后的原始 payload（未 complete 时返回 `Ok(None)`）。
    pub fn payload(&self) -> Result<Option<Vec<u8>>> {
        self.decoder.message().map_err(ur_err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_part_roundtrip_and_type() {
        let payload = b"hello air-gap";
        let s = encode_single("crypto-psbt", payload);
        assert!(s.starts_with("ur:crypto-psbt/"));
        assert_eq!(parse_type(&s).as_deref(), Some("crypto-psbt"));

        let (_, decoded) = ur::decode(&s).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn multi_part_roundtrip_via_collector() {
        // 构造一段较长 payload，强制多帧。
        let payload: Vec<u8> = (0..500u32).map(|i| (i % 251) as u8).collect();
        let parts = encode_parts("eth-sign-request", &payload, 50, 40).unwrap();
        assert!(parts.len() >= 10);

        let mut c = PartCollector::new();
        for p in &parts {
            if c.is_complete() {
                break;
            }
            c.receive(p).unwrap();
        }
        assert!(c.is_complete());
        assert_eq!(c.ur_type(), Some("eth-sign-request"));
        assert_eq!(c.payload().unwrap().as_deref(), Some(payload.as_slice()));
    }
}
