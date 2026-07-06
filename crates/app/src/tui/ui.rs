//! TUI 渲染（只读 `App` 状态作画，无副作用）。

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;

use super::app::{App, Screen, SetupField, NETWORKS};

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // 标题
            Constraint::Min(3),    // 主体
            Constraint::Length(3), // 页脚（错误 + 提示）
        ])
        .split(f.area());

    draw_title(f, chunks[0], app);
    draw_body(f, chunks[1], app);
    draw_footer(f, chunks[2], app);
}

fn draw_title(f: &mut Frame, area: Rect, app: &App) {
    let title = format!(
        " btc-wallate 离线签名机 · {:?} · {} ",
        app.network(),
        screen_name(app.screen)
    );
    let p = Paragraph::new(title).block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}

fn screen_name(s: Screen) -> &'static str {
    match s {
        Screen::Setup => "解锁",
        Screen::ChooseInput => "选择输入",
        Screen::FilePath => "待签文件",
        Screen::Scanning => "摄像头扫码",
        Screen::Verify => "核对交易",
        Screen::OutputChoose => "选择输出",
        Screen::OutFile => "输出文件",
        Screen::ShowQr => "二维码回显",
        Screen::Done => "完成",
    }
}

fn draw_body(f: &mut Frame, area: Rect, app: &App) {
    match app.screen {
        Screen::Setup => draw_setup(f, area, app),
        Screen::ChooseInput => draw_choose(
            f,
            area,
            "选择待签数据来源（↑↓ 选择，Enter 确认，Esc 返回）",
            &["从文件读入 (.psbt / base64 / ur)", "摄像头扫码"],
            app.choose_input_idx,
        ),
        Screen::FilePath => draw_input(f, area, "待签文件路径", &app.in_file.display()),
        Screen::Scanning => draw_scanning(f, area, app),
        Screen::Verify => draw_verify(f, area, app),
        Screen::OutputChoose => draw_choose(
            f,
            area,
            "选择签名结果输出方式（↑↓ 选择，Enter 确认，Esc 返回）",
            &["写入文件", "显示动画二维码（手机扫回广播）"],
            app.output_idx,
        ),
        Screen::OutFile => draw_input(f, area, "输出文件路径", &app.out_file.display()),
        Screen::ShowQr => draw_qr(f, area, app),
        Screen::Done => {
            let p = Paragraph::new(app.done_msg.clone())
                .block(Block::default().borders(Borders::ALL).title("完成"))
                .wrap(Wrap { trim: false });
            f.render_widget(p, area);
        }
    }
}

fn draw_setup(f: &mut Frame, area: Rect, app: &App) {
    let net = NETWORKS[app.network_idx];
    let field = |label: &str, val: String, focused: bool| {
        let marker = if focused { "▶ " } else { "  " };
        let style = if focused {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        Line::from(vec![Span::styled(format!("{marker}{label}: {val}"), style)])
    };
    let lines = vec![
        field(
            "keystore",
            app.keystore.display(),
            app.setup_field == SetupField::Keystore,
        ),
        field(
            "口令",
            app.password.display(),
            app.setup_field == SetupField::Password,
        ),
        field(
            "BIP39 passphrase(可空)",
            app.passphrase.display(),
            app.setup_field == SetupField::Passphrase,
        ),
        field(
            "网络(←→切换)",
            format!("{net:?}"),
            app.setup_field == SetupField::Network,
        ),
        Line::from(""),
        Line::from("Tab/↑↓ 切换字段，Enter 解锁，Esc 退出"),
    ];
    let p = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("解锁钱包"));
    f.render_widget(p, area);
}

fn draw_choose(f: &mut Frame, area: Rect, title: &str, items: &[&str], selected: usize) {
    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let prefix = if i == selected { "▶ " } else { "  " };
            let style = if i == selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(Span::styled(format!("{prefix}{s}"), style)))
        })
        .collect();
    let list = List::new(list_items).block(Block::default().borders(Borders::ALL).title(title.to_string()));
    f.render_widget(list, area);
}

fn draw_input(f: &mut Frame, area: Rect, title: &str, value: &str) {
    let text = vec![
        Line::from(format!("{value}▏")),
        Line::from(""),
        Line::from("输入路径后按 Enter，Esc 返回"),
    ];
    let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(title.to_string()));
    f.render_widget(p, area);
}

fn draw_scanning(f: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = vec![
        Line::from("对准手机屏幕上的二维码；Esc 取消。"),
        Line::from(""),
    ];
    for l in &app.scan_lines {
        lines.push(Line::from(l.clone()));
    }
    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("摄像头扫码"))
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn draw_verify(f: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "请逐项核对以下交易内容（防止被入侵的联网设备偷换收款地址）:",
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    for l in app.summary.lines() {
        lines.push(Line::from(l.to_string()));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "核对无误按 y 签名，n/Esc 取消。",
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    )));
    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("核对交易"))
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn draw_qr(f: &mut Frame, area: Rect, app: &App) {
    let frame_txt = app.current_qr_frame().unwrap_or("");
    let total = app.qr_frames.len();
    let header = if total <= 1 {
        "签名结果二维码（手机扫描后广播；q/Esc 结束）".to_string()
    } else {
        format!("动画二维码 帧 {}/{}（手机持续扫描；q/Esc 结束）", app.qr_idx + 1, total)
    };
    let mut lines = vec![Line::from(header)];
    for l in frame_txt.lines() {
        lines.push(Line::from(l.to_string()));
    }
    let p = Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("扫我"));
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let line = match &app.error {
        Some(e) => Line::from(Span::styled(
            format!("⚠ {e}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        None => Line::from(Span::styled(
            "私钥永不触网 · 签名前务必核对 · Ctrl-C 强制退出",
            Style::default().fg(Color::DarkGray),
        )),
    };
    let p = Paragraph::new(line).block(Block::default().borders(Borders::ALL));
    f.render_widget(p, area);
}
