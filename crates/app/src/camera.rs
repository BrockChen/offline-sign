//! 摄像头扫码（`camera` 特性）。
//!
//! 打开默认摄像头，持续抓帧并用 `rqrr` 解码二维码。识别的内容：
//! - `ur:...`（单帧或多帧动画二维码）——BlueWallet/Nunchuk/MetaMask+Keystone 的标准空气隙格式；
//! - base64 PSBT 二维码——部分钱包的静态 PSBT 二维码。
//!
//! 会实时打印「检测到二维码 / 收帧进度」，便于排查（例如误扫了地址/描述符这类非 `ur:` 二维码）。
//!
//! 平台说明：
//! - macOS：首次调用会触发系统摄像头权限申请，需在「系统设置 → 隐私与安全性 → 摄像头」
//!   放行运行本程序的终端 App（Terminal/iTerm）；未授权时画面全黑、扫不出任何二维码。
//! - Linux（目标 x86）：使用 V4L2，确保用户在 `video` 组且 `/dev/video0` 可访问。

use std::str::FromStr;

use anyhow::{Context, Result};
use bitcoin::psbt::Psbt;
use btc_wallate_core::airgap::{self, psbt as psbt_ur, PartCollector};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;

/// 打开默认摄像头扫描二维码，识别到完整数据后返回 `(ur_type, payload)`。Ctrl-C 取消。
pub fn scan_ur() -> Result<(String, Vec<u8>)> {
    let index = CameraIndex::Index(0);
    let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera =
        Camera::new(index, format).context("打开摄像头失败（检查设备连接与系统摄像头权限）")?;
    camera.open_stream().context("开启摄像头视频流失败")?;

    println!("摄像头已开启，对准要扫描的二维码，Ctrl-C 取消。");
    println!("（提示：这里要扫的是手机钱包给出的 ur:crypto-psbt / ur:eth-sign-request 动画二维码，");
    println!(" 不是本机导出的地址/描述符二维码。）");

    let mut collector = PartCollector::new();
    let mut frames = 0u64;
    let mut resolved = 0usize;
    let mut last_seen = String::new();
    let mut first_frame = true;

    loop {
        let frame = match camera.frame() {
            Ok(f) => f,
            Err(e) => {
                eprintln!("取帧失败: {e}");
                std::thread::sleep(std::time::Duration::from_millis(50));
                continue;
            }
        };
        let img = match frame.decode_image::<RgbFormat>() {
            Ok(i) => i,
            Err(_) => continue,
        };
        let (w, h) = (img.width() as usize, img.height() as usize);
        if first_frame {
            first_frame = false;
            println!("正在扫描……（帧 {w}x{h}）");
        }
        frames += 1;
        if frames % 60 == 0 {
            println!("扫描中……已处理 {frames} 帧，仍未识别到有效二维码。");
        }

        let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| {
            let p = img.get_pixel(x as u32, y as u32).0;
            ((p[0] as u16 * 30 + p[1] as u16 * 59 + p[2] as u16 * 11) / 100) as u8
        });

        for grid in prepared.detect_grids() {
            let content = match grid.decode() {
                Ok((_meta, c)) => c,
                Err(_) => continue, // 二维码模糊/半遮挡，跳过
            };
            let line = content.trim();

            // 新内容才打印，避免刷屏。
            if line != last_seen {
                last_seen = line.to_string();
                let preview: String = line.chars().take(56).collect();
                let ell = if line.chars().count() > 56 { "…" } else { "" };
                println!("检测到二维码: {preview}{ell}");
            }

            if let Some(rest) = line.strip_prefix("ur:") {
                // `ur:type/data` 为单帧；`ur:type/seq-total/data` 为多帧。
                let segments = rest.split('/').count();
                if segments <= 2 {
                    match airgap::decode_single(line) {
                        Ok((t, p)) => {
                            println!("扫描完成（单帧 UR）: {t}");
                            return Ok((t, p));
                        }
                        Err(e) => eprintln!("单帧 UR 解析失败: {e}"),
                    }
                } else {
                    let _ = collector.receive(line); // 重复/坏帧交解码器忽略
                }
            } else if let Ok(psbt) = Psbt::from_str(line) {
                // 非 UR，但是 base64 PSBT 静态二维码。
                println!("扫描完成（base64 PSBT 二维码）");
                return Ok((psbt_ur::UR_TYPE.to_string(), psbt_ur::to_cbor(&psbt)?));
            } else {
                println!("  ↑ 非 ur:/PSBT 二维码，已忽略（可能是地址或描述符二维码）");
            }
        }

        if let Some(n) = collector.resolved_fragments() {
            if n != resolved {
                resolved = n;
                println!("  已收 {n} 个分片……");
            }
        }
        if collector.is_complete() {
            let ur_type = collector.ur_type().unwrap_or_default().to_string();
            let payload = collector.payload()?.context("二维码重组失败")?;
            println!("扫描完成（多帧 UR）: {ur_type}");
            return Ok((ur_type, payload));
        }
    }
}
