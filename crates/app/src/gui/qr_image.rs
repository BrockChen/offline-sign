//! 把二维码数据渲染成 egui 纹理用的 `ColorImage`（清晰、可缩放，优于终端半块字符）。

use eframe::egui::{Color32, ColorImage};
use qrcode::{Color, QrCode};

/// 生成放大后的二维码图像：每个模块 `scale` 像素，四周留 4 模块静默边。
pub fn qr_image(data: &str, scale: usize) -> anyhow::Result<ColorImage> {
    let code = QrCode::new(data.as_bytes())
        .map_err(|e| anyhow::anyhow!("二维码生成失败（数据过长需分帧）: {e}"))?;
    let w = code.width();
    let colors = code.to_colors();
    let quiet = 4usize;
    let dim = (w + 2 * quiet) * scale;
    let mut pixels = vec![Color32::WHITE; dim * dim];
    for my in 0..w {
        for mx in 0..w {
            if colors[my * w + mx] != Color::Dark {
                continue;
            }
            let px0 = (mx + quiet) * scale;
            let py0 = (my + quiet) * scale;
            for dy in 0..scale {
                let row = (py0 + dy) * dim + px0;
                for dx in 0..scale {
                    pixels[row + dx] = Color32::BLACK;
                }
            }
        }
    }
    Ok(ColorImage {
        size: [dim, dim],
        pixels,
    })
}

/// 数据能否装进单张二维码（QR 版本容量内）。
pub fn fits_single(data: &str) -> bool {
    QrCode::new(data.as_bytes()).is_ok()
}
