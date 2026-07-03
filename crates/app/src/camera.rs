//! 摄像头扫码（`camera` 特性）。
//!
//! 打开默认摄像头，持续抓帧并用 `rqrr` 解码二维码；把解出的 `ur:...` 分片喂给
//! [`PartCollector`]，收齐后重组回 payload。用于「手机 → 签名机」方向的动画二维码传输，
//! 与 MetaMask+Keystone / BlueWallet 等现成钱包的二维码空气隙流程一致。
//!
//! 平台说明：
//! - macOS：首次调用会触发系统摄像头权限申请，需在「系统设置 → 隐私与安全性 → 摄像头」
//!   放行运行本程序的终端 App（Terminal/iTerm）。
//! - Linux（目标 x86）：使用 V4L2，确保用户在 `video` 组且 `/dev/video0` 可访问。

use anyhow::{Context, Result};
use btc_wallate_core::airgap::PartCollector;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;

/// 打开默认摄像头扫描动画二维码，收齐 UR 分片后返回 `(ur_type, payload)`。
/// 通过 Ctrl-C 取消。
pub fn scan_ur() -> Result<(String, Vec<u8>)> {
    let index = CameraIndex::Index(0);
    let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera = Camera::new(index, format)
        .context("打开摄像头失败（检查设备连接与系统摄像头权限）")?;
    camera
        .open_stream()
        .context("开启摄像头视频流失败")?;

    println!("正在用摄像头扫描二维码……对准手机屏幕，Ctrl-C 取消。");

    let mut collector = PartCollector::new();
    let mut reported = 0usize;

    loop {
        // 抓一帧；偶发坏帧直接跳过。
        let frame = match camera.frame() {
            Ok(f) => f,
            Err(_) => continue,
        };
        let img = match frame.decode_image::<RgbFormat>() {
            Ok(i) => i,
            Err(_) => continue,
        };

        let (w, h) = (img.width() as usize, img.height() as usize);
        // 就地把 RGB 转灰度喂给 rqrr（避免额外的 image 类型依赖）。
        let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| {
            let p = img.get_pixel(x as u32, y as u32).0;
            ((p[0] as u16 * 30 + p[1] as u16 * 59 + p[2] as u16 * 11) / 100) as u8
        });

        for grid in prepared.detect_grids() {
            if let Ok((_meta, content)) = grid.decode() {
                let line = content.trim();
                if line.starts_with("ur:") {
                    // 重复/损坏分片交由解码器自行忽略。
                    let _ = collector.receive(line);
                }
            }
        }

        // 进度反馈：已纳入重组的分片数增加时打印。
        if let Some(n) = collector.resolved_fragments() {
            if n != reported {
                reported = n;
                println!("  已收 {n} 个分片……");
            }
        }

        if collector.is_complete() {
            let ur_type = collector.ur_type().unwrap_or_default().to_string();
            let payload = collector.payload()?.context("二维码重组失败")?;
            println!("扫描完成：{ur_type}");
            return Ok((ur_type, payload));
        }
    }
}
