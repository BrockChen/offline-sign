//! 摄像头扫码（`camera` 特性）。
//!
//! 打开默认摄像头，持续抓帧并用 `rqrr` 解码二维码。识别的内容：
//! - `ur:...`（单帧或多帧动画二维码，大小写不敏感）——BlueWallet/Nunchuk/MetaMask+Keystone
//!   的标准空气隙格式；
//! - base64 PSBT 二维码——部分钱包的静态 PSBT 二维码。
//!
//! 扫码进度通过 [`ScanEvent`] 回调上报，**不直接打印**，以便 CLI（println）与 TUI（消息通道）
//! 各自消费。支持通过 `cancel` 原子标志协作取消。
//!
//! 平台说明：
//! - macOS：首次调用会触发系统摄像头权限申请，需在「系统设置 → 隐私与安全性 → 摄像头」
//!   放行运行本程序的终端 App；未授权时画面全黑、扫不出任何二维码。
//! - Linux（目标 x86）：使用 V4L2，确保用户在 `video` 组且 `/dev/video0` 可访问。

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use bitcoin::psbt::Psbt;
use btc_wallate_core::airgap::{self, psbt as psbt_ur, PartCollector};
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};
use nokhwa::Camera;

/// 扫码过程中的进度事件（供调用方渲染，替代直接打印）。
#[derive(Debug, Clone)]
pub enum ScanEvent {
    /// 摄像头已开启，报告首帧分辨率。
    Started { width: usize, height: usize },
    /// 已处理帧计数（周期性）。
    Frame(u64),
    /// 检测到一个二维码，内容预览（截断）。
    Detected(String),
    /// 多帧 UR 已纳入重组的分片数。
    Progress(usize),
    /// 检测到非 UR/PSBT 的二维码（已忽略）。
    NonUr,
}

/// 用回调 + 可取消标志扫描二维码。返回 `Ok(None)` 表示被取消。
///
/// `on_event` 用于上报进度（CLI 打印 / TUI 发消息）；`cancel` 每帧检查一次。
pub fn scan_ur_cb(
    cancel: &AtomicBool,
    on_event: &mut dyn FnMut(ScanEvent),
) -> Result<Option<(String, Vec<u8>)>> {
    let index = CameraIndex::Index(0);
    let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
    let mut camera =
        Camera::new(index, format).context("打开摄像头失败（检查设备连接与系统摄像头权限）")?;
    camera.open_stream().context("开启摄像头视频流失败")?;

    let mut collector = PartCollector::new();
    let mut frames = 0u64;
    let mut resolved = 0usize;
    let mut last_seen = String::new();
    let mut first_frame = true;

    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }

        let frame = match camera.frame() {
            Ok(f) => f,
            Err(_) => {
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
            on_event(ScanEvent::Started {
                width: w,
                height: h,
            });
        }
        frames += 1;
        if frames % 30 == 0 {
            on_event(ScanEvent::Frame(frames));
        }

        let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(w, h, |x, y| {
            let p = img.get_pixel(x as u32, y as u32).0;
            ((p[0] as u16 * 30 + p[1] as u16 * 59 + p[2] as u16 * 11) / 100) as u8
        });

        for grid in prepared.detect_grids() {
            let content = match grid.decode() {
                Ok((_meta, c)) => c,
                Err(_) => continue,
            };
            let line = content.trim();
            if line != last_seen {
                last_seen = line.to_string();
                let preview: String = line.chars().take(56).collect();
                on_event(ScanEvent::Detected(preview));
            }

            // 二维码里的 UR 常按 QR 字母数字模式用大写编码；bytewords 为小写字母，整体转小写即标准串。
            let is_ur = line.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("ur:"));
            if is_ur {
                let lower = line.to_ascii_lowercase();
                let rest = lower.strip_prefix("ur:").unwrap_or("");
                let segments = rest.split('/').count();
                if segments <= 2 {
                    if let Ok((t, p)) = airgap::decode_single(&lower) {
                        return Ok(Some((t, p)));
                    }
                } else {
                    let _ = collector.receive(&lower);
                }
            } else if let Ok(psbt) = Psbt::from_str(line) {
                return Ok(Some((psbt_ur::UR_TYPE.to_string(), psbt_ur::to_cbor(&psbt)?)));
            } else {
                on_event(ScanEvent::NonUr);
            }
        }

        if let Some(n) = collector.resolved_fragments() {
            if n != resolved {
                resolved = n;
                on_event(ScanEvent::Progress(n));
            }
        }
        if collector.is_complete() {
            let ur_type = collector.ur_type().unwrap_or_default().to_string();
            let payload = collector.payload()?.context("二维码重组失败")?;
            return Ok(Some((ur_type, payload)));
        }
    }
}

/// CLI 用：直接打印进度、不可取消，收齐后返回结果（保持 `sign --scan` 原有体验）。
pub fn scan_ur() -> Result<(String, Vec<u8>)> {
    println!("摄像头已开启，对准要扫描的二维码，Ctrl-C 取消。");
    println!("（要扫的是手机钱包给出的 ur:crypto-psbt / ur:eth-sign-request 二维码，");
    println!(" 不是本机导出的地址/描述符二维码。）");
    let cancel = AtomicBool::new(false);
    let mut on_event = |ev: ScanEvent| match ev {
        ScanEvent::Started { width, height } => println!("正在扫描……（帧 {width}x{height}）"),
        ScanEvent::Frame(n) => println!("扫描中……已处理 {n} 帧，仍未识别到有效二维码。"),
        ScanEvent::Detected(preview) => println!("检测到二维码: {preview}…"),
        ScanEvent::Progress(n) => println!("  已收 {n} 个分片……"),
        ScanEvent::NonUr => println!("  ↑ 非 ur:/PSBT 二维码，已忽略（可能是地址或描述符二维码）"),
    };
    match scan_ur_cb(&cancel, &mut on_event)? {
        Some(r) => Ok(r),
        None => anyhow::bail!("扫描已取消"),
    }
}
