//! egui 图形界面：聚焦签名流程（解锁 → 选输入 → 核对 → 签名 → 输出）。
//!
//! 逻辑全部复用 `crate::ops`；摄像头扫码在后台线程跑 `crate::camera::scan_ur_cb`，
//! 通过通道把「预览帧 / 进度 / 结果」送回 UI 线程。CJK 字体在启动时尽力从系统加载。

use std::path::Path;

use bitcoin::Network;
use eframe::egui;
use btc_wallate_core::seed::Wallet;

use crate::gui::qr_image;
use crate::ops::{self, SignJob};

#[cfg(feature = "camera")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "camera")]
use std::sync::mpsc::{self, Receiver};
#[cfg(feature = "camera")]
use std::sync::Arc;
#[cfg(feature = "camera")]
use std::thread::JoinHandle;

pub const NETWORKS: [Network; 4] = [
    Network::Bitcoin,
    Network::Testnet,
    Network::Signet,
    Network::Regtest,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Welcome,
    Overview,
    ChooseInput,
    FilePath,
    #[cfg(feature = "camera")]
    Scanning,
    Verify,
    Output,
    Done,
}

#[cfg(feature = "camera")]
enum ScanMsg {
    Line(String),
    Preview { width: usize, height: usize, rgb: Vec<u8> },
    Done(anyhow::Result<Option<(String, Vec<u8>)>>),
}

#[cfg(feature = "camera")]
struct ScanState {
    cancel: Arc<AtomicBool>,
    rx: Receiver<ScanMsg>,
    handle: Option<JoinHandle<()>>,
    lines: Vec<String>,
    preview: Option<egui::TextureHandle>,
}

pub struct GuiApp {
    screen: Screen,
    error: Option<String>,
    // Setup
    keystore: String,
    password: String,
    passphrase: String,
    net_idx: usize,
    // 运行态
    wallet: Option<Wallet>,
    job: Option<SignJob>,
    btc_addr: String,
    eth_addr: String,
    summary: String,
    out_type: String,
    out_payload: Vec<u8>,
    in_file: String,
    out_file: String,
    // 输出二维码
    qr_frames: Vec<String>,
    qr_idx: usize,
    qr_tex: Option<egui::TextureHandle>,
    qr_tex_idx: Option<usize>,
    last_switch: f64,
    done_msg: String,
    #[cfg(feature = "camera")]
    scan: Option<ScanState>,
}

impl GuiApp {
    pub fn new(network: Network, default_keystore: Option<String>) -> Self {
        let net_idx = NETWORKS.iter().position(|n| *n == network).unwrap_or(0);
        GuiApp {
            screen: Screen::Welcome,
            error: None,
            keystore: default_keystore.unwrap_or_default(),
            password: String::new(),
            passphrase: String::new(),
            net_idx,
            wallet: None,
            job: None,
            btc_addr: String::new(),
            eth_addr: String::new(),
            summary: String::new(),
            out_type: String::new(),
            out_payload: Vec::new(),
            in_file: String::new(),
            out_file: String::new(),
            qr_frames: Vec::new(),
            qr_idx: 0,
            qr_tex: None,
            qr_tex_idx: None,
            last_switch: 0.0,
            done_msg: String::new(),
            #[cfg(feature = "camera")]
            scan: None,
        }
    }

    fn network(&self) -> Network {
        NETWORKS[self.net_idx]
    }

    // ---------- 动作（复用 ops） ----------

    fn try_unlock(&mut self) {
        self.error = None;
        let blob = match std::fs::read(self.keystore.trim()) {
            Ok(b) => b,
            Err(e) => {
                self.error = Some(format!("读取 keystore 失败: {e}"));
                return;
            }
        };
        match ops::load_wallet(&blob, &self.password, &self.passphrase, self.network()) {
            Ok(w) => {
                // 派生地址供概览确认（加载的是不是预期钱包/网络）。
                self.btc_addr = ops::address(&w, true, 0, false, 0).unwrap_or_default();
                self.eth_addr = ops::address(&w, false, 0, false, 0).unwrap_or_default();
                self.wallet = Some(w);
                self.screen = Screen::Overview;
            }
            Err(e) => self.error = Some(format!("解锁失败: {e}")),
        }
    }

    fn load_input_file(&mut self) {
        self.error = None;
        match crate::file_channel::read_signing_input(Path::new(self.in_file.trim())) {
            Ok((ty, payload)) => self.enter_verify(&ty, &payload),
            Err(e) => self.error = Some(format!("读取待签文件失败: {e}")),
        }
    }

    fn enter_verify(&mut self, ur_type: &str, payload: &[u8]) {
        let wallet = match &self.wallet {
            Some(w) => w,
            None => {
                self.error = Some("内部错误：钱包未解锁".into());
                return;
            }
        };
        match ops::parse_job(ur_type, payload) {
            Ok(job) => match ops::summarize(wallet, &job) {
                Ok(summary) => {
                    self.summary = summary;
                    self.job = Some(job);
                    self.error = None;
                    self.screen = Screen::Verify;
                }
                Err(e) => self.error = Some(format!("生成核对摘要失败: {e}")),
            },
            Err(e) => self.error = Some(format!("解析待签数据失败: {e}")),
        }
    }

    fn do_sign(&mut self) {
        let (wallet, job) = match (&self.wallet, &self.job) {
            (Some(w), Some(j)) => (w, j),
            _ => return,
        };
        match ops::sign(wallet, job) {
            Ok((ty, payload)) => {
                self.out_type = ty;
                self.out_payload = payload;
                self.build_qr_frames();
                self.screen = Screen::Output;
            }
            Err(e) => self.error = Some(format!("签名失败: {e}")),
        }
    }

    fn build_qr_frames(&mut self) {
        use btc_wallate_core::airgap;
        let single = airgap::encode_single(&self.out_type, &self.out_payload);
        if qr_image::fits_single(&single) {
            self.qr_frames = vec![single];
        } else {
            let frag = 120usize;
            let parts = (((self.out_payload.len() / frag) + 1) * 3).clamp(8, 64);
            self.qr_frames = airgap::encode_parts(&self.out_type, &self.out_payload, frag, parts)
                .unwrap_or_else(|_| vec![single]);
        }
        self.qr_idx = 0;
        self.qr_tex = None;
        self.qr_tex_idx = None;
    }

    fn save_output_file(&mut self) {
        self.error = None;
        match crate::file_channel::write_signed(
            Path::new(self.out_file.trim()),
            &self.out_type,
            &self.out_payload,
        ) {
            Ok(()) => {
                self.done_msg = format!("已写入 {}", self.out_file.trim());
                self.screen = Screen::Done;
            }
            Err(e) => self.error = Some(format!("写入失败: {e}")),
        }
    }

    fn reset_flow(&mut self) {
        self.job = None;
        self.summary.clear();
        self.out_type.clear();
        self.out_payload.clear();
        self.qr_frames.clear();
        self.qr_tex = None;
        self.qr_tex_idx = None;
        self.error = None;
        self.screen = Screen::ChooseInput;
    }

    // ---------- 摄像头扫码 ----------

    #[cfg(feature = "camera")]
    fn start_scan(&mut self) {
        self.error = None;
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let cancel_thread = cancel.clone();
        let handle = std::thread::spawn(move || {
            let tx_evt = tx.clone();
            let mut on_event = move |ev: crate::camera::ScanEvent| {
                use crate::camera::ScanEvent;
                let msg = match ev {
                    ScanEvent::Preview { width, height, rgb } => {
                        ScanMsg::Preview { width, height, rgb }
                    }
                    ScanEvent::Started { width, height } => {
                        ScanMsg::Line(format!("摄像头已开启 {width}x{height}，扫描中……"))
                    }
                    ScanEvent::Detected(p) => ScanMsg::Line(format!("检测到二维码: {p}…")),
                    ScanEvent::Progress(n) => ScanMsg::Line(format!("已收 {n} 个分片……")),
                    ScanEvent::NonUr => ScanMsg::Line("非 ur:/PSBT 二维码，已忽略".into()),
                };
                let _ = tx_evt.send(msg);
            };
            let result = crate::camera::scan_ur_cb(&cancel_thread, &mut on_event);
            let _ = tx.send(ScanMsg::Done(result));
        });
        self.scan = Some(ScanState {
            cancel,
            rx,
            handle: Some(handle),
            lines: Vec::new(),
            preview: None,
        });
        self.screen = Screen::Scanning;
    }

    #[cfg(feature = "camera")]
    fn stop_scan(&mut self) {
        if let Some(mut scan) = self.scan.take() {
            scan.cancel.store(true, Ordering::Relaxed);
            if let Some(h) = scan.handle.take() {
                let _ = h.join();
            }
        }
    }

    #[cfg(feature = "camera")]
    fn poll_scan(&mut self, ctx: &egui::Context) {
        let mut done = None;
        if let Some(scan) = &mut self.scan {
            while let Ok(msg) = scan.rx.try_recv() {
                match msg {
                    ScanMsg::Line(l) => {
                        scan.lines.push(l);
                        if scan.lines.len() > 8 {
                            scan.lines.remove(0);
                        }
                    }
                    ScanMsg::Preview { width, height, rgb } => {
                        if rgb.len() == width * height * 3 {
                            let img = egui::ColorImage::from_rgb([width, height], &rgb);
                            scan.preview = Some(ctx.load_texture(
                                "preview",
                                img,
                                egui::TextureOptions::default(),
                            ));
                        }
                    }
                    ScanMsg::Done(r) => done = Some(r),
                }
            }
        }
        if let Some(r) = done {
            self.stop_scan();
            match r {
                Ok(Some((ty, payload))) => self.enter_verify(&ty, &payload),
                Ok(None) => self.screen = Screen::ChooseInput,
                Err(e) => {
                    self.error = Some(format!("扫码失败: {e}"));
                    self.screen = Screen::ChooseInput;
                }
            }
        }
    }

    // ---------- 各面板 ----------

    /// 顶部步骤条：1 解锁 · 2 选择交易 · 3 核对 · 4 输出。
    fn step_bar(&self, ui: &mut egui::Ui) {
        let cur = match self.screen {
            Screen::Welcome | Screen::Overview => 0,
            Screen::ChooseInput | Screen::FilePath => 1,
            #[cfg(feature = "camera")]
            Screen::Scanning => 1,
            Screen::Verify => 2,
            Screen::Output | Screen::Done => 3,
        };
        let steps = ["1 解锁", "2 选择交易", "3 核对", "4 输出"];
        ui.horizontal(|ui| {
            for (i, s) in steps.iter().enumerate() {
                if i == cur {
                    ui.strong(*s);
                } else {
                    ui.weak(*s);
                }
                if i + 1 < steps.len() {
                    ui.weak("·");
                }
            }
        });
    }

    fn ui_welcome(&mut self, ui: &mut egui::Ui) {
        ui.heading("解锁钱包");
        let exists = std::path::Path::new(self.keystore.trim()).exists();
        egui::Grid::new("welcome").num_columns(2).show(ui, |ui| {
            ui.label("keystore 路径");
            ui.text_edit_singleline(&mut self.keystore);
            ui.end_row();
            ui.label("状态");
            if exists {
                ui.colored_label(egui::Color32::from_rgb(0x2e, 0xa0, 0x43), "✅ 已找到");
            } else {
                ui.colored_label(egui::Color32::from_rgb(0xd8, 0x8a, 0x00), "⚠ 未找到");
            }
            ui.end_row();
            ui.label("网络");
            egui::ComboBox::from_id_salt("net")
                .selected_text(format!("{:?}", self.network()))
                .show_ui(ui, |ui| {
                    for (i, n) in NETWORKS.iter().enumerate() {
                        ui.selectable_value(&mut self.net_idx, i, format!("{n:?}"));
                    }
                });
            ui.end_row();
            ui.label("币种");
            ui.label("BTC（BIP-84 原生 segwit, bc1/tb1…）· ETH（BIP-44 m/44'/60', 0x…）");
            ui.end_row();
        });
        ui.separator();
        if exists {
            egui::Grid::new("unlock").num_columns(2).show(ui, |ui| {
                ui.label("口令");
                ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
                ui.end_row();
                ui.label("BIP-39 passphrase（可空）");
                ui.add(egui::TextEdit::singleline(&mut self.passphrase).password(true));
                ui.end_row();
            });
            if ui.button("解锁").clicked() {
                self.try_unlock();
            }
        } else {
            ui.label("未找到 keystore。请先用命令行创建：");
            ui.monospace(format!("btc-wallate new --keystore {}", self.keystore.trim()));
            ui.label("或从助记词恢复：");
            ui.monospace(format!("btc-wallate restore --keystore {}", self.keystore.trim()));
            ui.label("创建后修改上方路径（或重开程序）即可解锁。");
        }
    }

    fn ui_overview(&mut self, ui: &mut egui::Ui) {
        ui.heading("钱包概览");
        ui.label("请确认这是你预期的钱包与网络：");
        egui::Grid::new("overview").num_columns(2).show(ui, |ui| {
            ui.label("网络");
            ui.label(format!("{:?}", self.network()));
            ui.end_row();
            ui.label("BTC 首收款地址");
            ui.monospace(&self.btc_addr);
            ui.end_row();
            ui.label("ETH 地址");
            ui.monospace(&self.eth_addr);
            ui.end_row();
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("开始签名 →").clicked() {
                self.screen = Screen::ChooseInput;
            }
            if ui.button("重新选择 keystore").clicked() {
                self.wallet = None;
                self.btc_addr.clear();
                self.eth_addr.clear();
                self.password.clear();
                self.error = None;
                self.screen = Screen::Welcome;
            }
        });
    }

    fn ui_choose(&mut self, ui: &mut egui::Ui) {
        ui.heading("选择待签数据来源");
        if ui.button("从文件读入（.psbt / base64 / ur）").clicked() {
            self.screen = Screen::FilePath;
        }
        #[cfg(feature = "camera")]
        if ui.button("摄像头扫码").clicked() {
            self.start_scan();
        }
        #[cfg(not(feature = "camera"))]
        ui.label("（未编译 camera 特性，仅文件通道；用 --features \"gui camera\" 重建以启用扫码）");
    }

    fn ui_file(&mut self, ui: &mut egui::Ui) {
        ui.heading("待签文件");
        ui.horizontal(|ui| {
            ui.label("路径");
            ui.text_edit_singleline(&mut self.in_file);
        });
        ui.horizontal(|ui| {
            if ui.button("读取并核对").clicked() {
                self.load_input_file();
            }
            if ui.button("返回").clicked() {
                self.screen = Screen::ChooseInput;
            }
        });
    }

    #[cfg(feature = "camera")]
    fn ui_scanning(&mut self, ui: &mut egui::Ui) {
        ui.heading("摄像头扫码");
        ui.label("对准手机屏幕上的二维码；");
        if let Some(scan) = &self.scan {
            if let Some(tex) = &scan.preview {
                let max = egui::vec2(480.0, 360.0);
                ui.add(egui::Image::new(tex).max_size(max));
            } else {
                ui.label("正在启动摄像头……（首次需在系统隐私设置中授权终端/本程序）");
            }
            for l in &scan.lines {
                ui.label(l);
            }
        }
        if ui.button("取消").clicked() {
            self.stop_scan();
            self.screen = Screen::ChooseInput;
        }
    }

    fn ui_verify(&mut self, ui: &mut egui::Ui) {
        ui.heading("核对交易");
        ui.label("请逐项核对（防止被入侵的联网设备偷换收款地址）：");
        egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
            ui.monospace(&self.summary);
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("✅ 确认无误，签名").clicked() {
                self.do_sign();
            }
            if ui.button("取消").clicked() {
                self.reset_flow();
            }
        });
    }

    fn ui_output(&mut self, ui: &mut egui::Ui) {
        ui.heading("签名结果");
        if self.qr_frames.is_empty() {
            ui.label("（无二维码）");
        } else {
            // 需要时（首帧/切帧）重建纹理。
            if self.qr_tex_idx != Some(self.qr_idx) || self.qr_tex.is_none() {
                match qr_image::qr_image(&self.qr_frames[self.qr_idx], 6) {
                    Ok(img) => {
                        self.qr_tex = Some(ui.ctx().load_texture(
                            "qr",
                            img,
                            egui::TextureOptions::NEAREST,
                        ));
                        self.qr_tex_idx = Some(self.qr_idx);
                    }
                    Err(e) => self.error = Some(e.to_string()),
                }
            }
            if self.qr_frames.len() > 1 {
                ui.label(format!("动画二维码 帧 {}/{}", self.qr_idx + 1, self.qr_frames.len()));
            } else {
                ui.label("扫描此二维码，在手机端广播：");
            }
            if let Some(tex) = &self.qr_tex {
                ui.add(egui::Image::new(tex).max_size(egui::vec2(420.0, 420.0)));
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("或写入文件");
            ui.text_edit_singleline(&mut self.out_file);
            if ui.button("保存").clicked() {
                self.save_output_file();
            }
        });
        if ui.button("完成").clicked() {
            self.done_msg = "已完成。".into();
            self.screen = Screen::Done;
        }
    }

    fn ui_done(&mut self, ui: &mut egui::Ui) {
        ui.heading("完成");
        ui.label(&self.done_msg);
        if ui.button("再签一笔").clicked() {
            self.reset_flow();
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(feature = "camera")]
        self.poll_scan(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("btc-wallate 离线签名机");
                ui.label(format!("· {:?}", self.network()));
            });
            self.step_bar(ui);
            if let Some(e) = &self.error {
                ui.colored_label(egui::Color32::RED, format!("⚠ {e}"));
            }
            ui.separator();
            match self.screen {
                Screen::Welcome => self.ui_welcome(ui),
                Screen::Overview => self.ui_overview(ui),
                Screen::ChooseInput => self.ui_choose(ui),
                Screen::FilePath => self.ui_file(ui),
                #[cfg(feature = "camera")]
                Screen::Scanning => self.ui_scanning(ui),
                Screen::Verify => self.ui_verify(ui),
                Screen::Output => self.ui_output(ui),
                Screen::Done => self.ui_done(ui),
            }
        });

        // 扫码时持续刷新以拉取预览/进度。
        #[cfg(feature = "camera")]
        if matches!(self.screen, Screen::Scanning) {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        // 多帧二维码动画。
        if matches!(self.screen, Screen::Output) && self.qr_frames.len() > 1 {
            let now = ctx.input(|i| i.time);
            if now - self.last_switch > 0.2 {
                self.qr_idx = (self.qr_idx + 1) % self.qr_frames.len();
                self.last_switch = now;
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

#[cfg(feature = "camera")]
impl Drop for GuiApp {
    fn drop(&mut self) {
        self.stop_scan();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> GuiApp {
        GuiApp::new(Network::Signet, Some("wallet.ks".into()))
    }

    #[test]
    fn new_starts_at_welcome_and_prefills_keystore() {
        let a = app();
        assert!(matches!(a.screen, Screen::Welcome));
        assert_eq!(a.keystore, "wallet.ks");
        assert_eq!(a.network(), Network::Signet);
    }

    #[test]
    fn unlock_flow_reaches_overview_with_addresses() {
        // 造一个临时 keystore，走 try_unlock → Overview，并核对派生地址。
        let (_phrase, blob) = crate::ops::create_keystore(
            Some("test test test test test test test test test test test junk"),
            12,
            Network::Signet,
            "pw",
        )
        .unwrap();
        let path = std::env::temp_dir().join("btc_wallate_gui_unlock.ks");
        std::fs::write(&path, &blob).unwrap();

        let mut a = GuiApp::new(Network::Signet, Some(path.to_string_lossy().into_owned()));
        a.password = "pw".into();
        a.try_unlock();

        assert!(a.wallet.is_some());
        assert!(matches!(a.screen, Screen::Overview));
        assert!(a.btc_addr.starts_with("tb1"), "btc={}", a.btc_addr);
        assert!(a.eth_addr.starts_with("0x"), "eth={}", a.eth_addr);
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn unlock_missing_keystore_sets_error() {
        let mut a = GuiApp::new(Network::Signet, Some("/nonexistent/xx.ks".into()));
        a.password = "pw".into();
        a.try_unlock();
        assert!(a.wallet.is_none());
        assert!(a.error.is_some());
        assert!(matches!(a.screen, Screen::Welcome));
    }

    #[test]
    fn sign_without_wallet_or_job_is_noop() {
        let mut a = app();
        a.screen = Screen::Verify;
        a.do_sign(); // 无 wallet/job：应安全无操作、不推进
        assert!(matches!(a.screen, Screen::Verify));
    }

    #[test]
    fn enter_verify_without_wallet_sets_error() {
        let mut a = app();
        a.enter_verify("crypto-psbt", &[0u8]);
        assert!(a.error.is_some());
    }

    #[test]
    fn build_qr_frames_produces_frames() {
        let mut a = app();
        a.out_type = "eth-signature".into();
        a.out_payload = vec![1, 2, 3, 4, 5];
        a.build_qr_frames();
        assert!(!a.qr_frames.is_empty());
        assert_eq!(a.qr_idx, 0);
    }
}
