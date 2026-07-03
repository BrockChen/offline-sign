//! 终端二维码显示。
//!
//! 用半块字符（`▀▄█`）渲染二维码到终端，等价 `qrencode -t ANSIUTF8`——每个字符承载
//! 上下两个模块，长宽比接近正方形，手机可稳定扫描。动画二维码则清屏逐帧重绘。

use qrcode::render::unicode;
use qrcode::QrCode;

/// 把数据渲染为可打印的终端二维码字符串。
pub fn render(data: &str) -> anyhow::Result<String> {
    let code = QrCode::new(data.as_bytes())
        .map_err(|e| anyhow::anyhow!("二维码生成失败（数据可能过长，改用动画分帧）: {e}"))?;
    Ok(code
        .render::<unicode::Dense1x2>()
        .quiet_zone(true)
        .build())
}

/// 打印单个二维码，附带其 UR 文本（便于核对/复制）。
pub fn print(data: &str) -> anyhow::Result<()> {
    println!("{}", render(data)?);
    println!("{data}");
    Ok(())
}

/// 打印动画二维码的一帧（清屏 + 帧序号 + 二维码）。调用方按 fps 循环。
pub fn print_frame(part: &str, idx: usize, total: usize) -> anyhow::Result<()> {
    // 清屏 + 光标归位（ANSI）。
    print!("\x1b[2J\x1b[H");
    println!("动画二维码 帧 {}/{}（手机对准持续扫描即可，可循环多轮）\n", idx + 1, total);
    println!("{}", render(part)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_nonempty_qr() {
        let out = render("ur:eth-signature/abcdef").unwrap();
        assert!(!out.is_empty());
        // 半块渲染应包含块状字符。
        assert!(out.contains('█') || out.contains('▀') || out.contains('▄'));
    }
}
