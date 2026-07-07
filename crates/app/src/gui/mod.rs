//! egui 图形界面（`gui` 特性）。聚焦签名流程；逻辑复用 `crate::ops`，摄像头复用
//! `crate::camera::scan_ur_cb`（需同时启用 `camera` 特性）。

mod app;
mod qr_image;

use std::path::PathBuf;

use anyhow::anyhow;
use bitcoin::Network;
use eframe::egui;

pub use app::GuiApp;

/// 启动 egui 图形界面。`default_keystore` 预填 keystore 路径（可空）。
pub fn run(network: Network, default_keystore: Option<PathBuf>) -> anyhow::Result<()> {
    let ks = default_keystore.map(|p| p.to_string_lossy().into_owned());
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "btc-wallate 离线签名机",
        options,
        Box::new(move |cc| {
            install_cjk_font(&cc.egui_ctx);
            Ok(Box::new(GuiApp::new(network, ks)))
        }),
    )
    .map_err(|e| anyhow!("egui 运行失败: {e}"))?;
    Ok(())
}

/// 尽力从系统加载一个 CJK 字体，使中文标签/摘要可渲染；找不到则退回默认字体（中文显示为方框）。
fn install_cjk_font(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        // macOS
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
        // Linux（Noto / 文泉驿）
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJKsc-Regular.otf",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/wenquanyi/wqy-microhei/wqy-microhei.ttc",
    ];
    for path in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("cjk".to_owned(), egui::FontData::from_owned(bytes).into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts
                .families
                .entry(family)
                .or_default()
                .insert(0, "cjk".to_owned());
        }
        ctx.set_fonts(fonts);
        return;
    }
}
