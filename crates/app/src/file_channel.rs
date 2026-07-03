//! U盘/SD 文件通道：与二维码同源的备用数据通路。
//!
//! 读入：支持 `.ur` 文本文件（一行或多行 `ur:...` 分片）与原始 `.psbt` 二进制文件。
//! 写出：把结果 payload 编成单条 `ur:...` 文本写盘（单条 UR 可承载任意长度，
//! 适合文件；二维码显示则另用动画分帧）。

use std::fs;
use std::path::Path;

use anyhow::{bail, Context};
use bitcoin::psbt::Psbt;
use btc_wallate_core::airgap::{self, psbt as psbt_ur, PartCollector};

/// 从文件读入待签数据，返回 `(ur_type, payload_cbor)`。
///
/// - 以 `ur:` 开头 ⇒ 按 UR 文本解析（多行分片自动收帧重组）。
/// - 否则 ⇒ 视为原始 PSBT 二进制，包装成 crypto-psbt 的 CBOR。
pub fn read_signing_input(path: &Path) -> anyhow::Result<(String, Vec<u8>)> {
    let bytes = fs::read(path).with_context(|| format!("读取 {} 失败", path.display()))?;

    if bytes.starts_with(b"ur:") {
        let text = String::from_utf8(bytes).context("UR 文件不是合法 UTF-8")?;
        let lines: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| l.starts_with("ur:"))
            .collect();
        if lines.is_empty() {
            bail!("UR 文件中未找到 ur: 数据");
        }
        // 单行 ⇒ 单帧；多行 ⇒ 多帧分片，需收齐重组。
        if lines.len() == 1 {
            Ok(airgap::decode_single(lines[0])?)
        } else {
            let mut c = PartCollector::new();
            for line in &lines {
                if c.is_complete() {
                    break;
                }
                c.receive(line)?;
            }
            if !c.is_complete() {
                bail!("UR 分片不完整，无法重组");
            }
            let ur_type = c.ur_type().context("无法识别 UR 类型")?.to_string();
            let payload = c.payload()?.context("UR 重组后为空")?;
            Ok((ur_type, payload))
        }
    } else {
        let psbt = Psbt::deserialize(&bytes).context("既非 UR 文本也非合法 PSBT 文件")?;
        Ok((psbt_ur::UR_TYPE.to_string(), psbt_ur::to_cbor(&psbt)?))
    }
}

/// 把结果 payload 以单条 UR 文本写入文件。
pub fn write_ur(path: &Path, ur_type: &str, payload: &[u8]) -> anyhow::Result<()> {
    let s = airgap::encode_single(ur_type, payload);
    fs::write(path, s).with_context(|| format!("写入 {} 失败", path.display()))?;
    Ok(())
}
