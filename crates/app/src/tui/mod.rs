//! 聚焦签名流程的 TUI（ratatui）。
//!
//! `run()` 负责终端进入/退出（`ratatui::init/restore` 已装 panic 钩子还原终端）与事件循环；
//! 状态机在 [`app`]、渲染在 [`ui`]。摄像头扫码在后台线程运行，进度经通道回主循环，
//! 与 ratatui 独占的屏幕互不干扰。

mod app;
mod ui;

pub use app::App;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use bitcoin::Network;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use app::{Action, Screen};

#[cfg(feature = "camera")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "camera")]
use std::sync::mpsc::{self, Receiver};
#[cfg(feature = "camera")]
use std::sync::Arc;
#[cfg(feature = "camera")]
use std::thread::JoinHandle;

/// 启动 TUI 签名流程。`default_keystore` 预填 keystore 路径（可空）。
pub fn run(network: Network, default_keystore: Option<PathBuf>) -> Result<()> {
    let camera_available = cfg!(feature = "camera");
    let ks = default_keystore.map(|p| p.to_string_lossy().into_owned());
    let mut app = App::new(network, ks, camera_available);

    // try_init 失败时返回错误而非 panic（如非真实终端 / 管道场景）。
    let mut terminal = ratatui::try_init()
        .context("初始化终端失败：TUI 需在真实终端中运行；脚本化请改用子命令（见 --help）")?;
    let res = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    res
}

fn event_loop(terminal: &mut DefaultTerminal, app: &mut App) -> Result<()> {
    #[cfg(feature = "camera")]
    let mut scan: Option<(Arc<AtomicBool>, Receiver<ScanMsg>, JoinHandle<()>)> = None;

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // 摄像头扫码线程：与 app.screen 对齐启动/停止，并排空进度消息。
        #[cfg(feature = "camera")]
        {
            if app.screen == Screen::Scanning && scan.is_none() {
                scan = Some(start_scan_thread());
            }
            if app.screen != Screen::Scanning {
                if let Some((cancel, _rx, handle)) = scan.take() {
                    cancel.store(true, Ordering::Relaxed);
                    let _ = handle.join();
                }
            }
            if let Some((_, rx, _)) = scan.as_ref() {
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        ScanMsg::Line(l) => app.push_scan_line(l),
                        ScanMsg::Done(r) => app.on_scan_done(r),
                    }
                }
            }
        }

        // 事件轮询带超时，以便驱动二维码动画与扫码刷新。
        if event::poll(Duration::from_millis(120))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    // Ctrl-C 全局强制退出（raw 模式下不触发 SIGINT）。
                    if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
                        break;
                    }
                    if app.on_key(k) == Action::Quit {
                        break;
                    }
                }
            }
        }

        if app.screen == Screen::ShowQr {
            app.qr_tick();
        }
        if app.should_quit {
            break;
        }
    }

    // 收尾：确保扫码线程已停止。
    #[cfg(feature = "camera")]
    if let Some((cancel, _rx, handle)) = scan.take() {
        cancel.store(true, Ordering::Relaxed);
        let _ = handle.join();
    }
    Ok(())
}

#[cfg(feature = "camera")]
enum ScanMsg {
    Line(String),
    Done(Result<Option<(String, Vec<u8>)>>),
}

#[cfg(feature = "camera")]
fn start_scan_thread() -> (Arc<AtomicBool>, Receiver<ScanMsg>, JoinHandle<()>) {
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let cancel_thread = cancel.clone();
    let handle = std::thread::spawn(move || {
        let tx_evt = tx.clone();
        let mut on_event = move |ev: crate::camera::ScanEvent| {
            let _ = tx_evt.send(ScanMsg::Line(format_scan_event(ev)));
        };
        let result = crate::camera::scan_ur_cb(&cancel_thread, &mut on_event);
        let _ = tx.send(ScanMsg::Done(result));
    });
    (cancel, rx, handle)
}

#[cfg(feature = "camera")]
fn format_scan_event(ev: crate::camera::ScanEvent) -> String {
    use crate::camera::ScanEvent;
    match ev {
        ScanEvent::Started { width, height } => {
            format!("摄像头已开启（{width}x{height}），扫描中……")
        }
        ScanEvent::Frame(n) => format!("已处理 {n} 帧，仍未识别到有效二维码"),
        ScanEvent::Detected(p) => format!("检测到二维码: {p}…"),
        ScanEvent::Progress(n) => format!("已收 {n} 个分片……"),
        ScanEvent::NonUr => "↑ 非 ur:/PSBT 二维码，已忽略".to_string(),
    }
}
