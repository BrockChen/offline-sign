//! TUI 状态机（与终端渲染、线程解耦，便于单元测试）。
//!
//! 签名流程：`Setup`（解锁钱包）→ `ChooseInput`（文件/摄像头）→ `FilePath`/`Scanning`
//! → `Verify`（核对+确认）→ `OutputChoose`（文件/二维码）→ `OutFile`/`ShowQr` → `Done`。
//!
//! 摄像头相关的副作用（后台线程、事件通道）由 `mod.rs` 处理；本模块只在 `Scanning` 屏
//! 接收已格式化的进度行与最终结果，故不直接依赖 `camera` 模块，无 `camera` 特性也能编译。

use std::path::Path;

use bitcoin::Network;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use btc_wallate_core::seed::Wallet;

use crate::file_channel;
use crate::ops::{self, SignJob};

/// 键处理后交给事件循环执行的动作（涉及线程的副作用留给 `mod.rs`）。
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    /// 进入摄像头扫描（由事件循环启动后台线程）。
    StartScan,
}

/// 极简文本输入域。
pub struct Input {
    pub value: String,
    pub masked: bool,
}

impl Input {
    pub fn text(value: impl Into<String>) -> Self {
        Input { value: value.into(), masked: false }
    }
    pub fn masked() -> Self {
        Input { value: String::new(), masked: true }
    }
    pub fn push(&mut self, c: char) {
        self.value.push(c);
    }
    pub fn backspace(&mut self) {
        self.value.pop();
    }
    /// 显示用：口令域显示为等长 `*`。
    pub fn display(&self) -> String {
        if self.masked {
            "*".repeat(self.value.chars().count())
        } else {
            self.value.clone()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Setup,
    ChooseInput,
    FilePath,
    Scanning,
    Verify,
    OutputChoose,
    OutFile,
    ShowQr,
    Done,
}

pub const NETWORKS: [Network; 4] = [
    Network::Bitcoin,
    Network::Testnet,
    Network::Signet,
    Network::Regtest,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupField {
    Keystore,
    Password,
    Passphrase,
    Network,
}

pub struct App {
    pub screen: Screen,
    pub should_quit: bool,
    pub error: Option<String>,

    // Setup
    pub keystore: Input,
    pub password: Input,
    pub passphrase: Input,
    pub network_idx: usize,
    pub setup_field: SetupField,

    // 运行态
    pub wallet: Option<Wallet>,
    pub job: Option<SignJob>,
    pub summary: String,
    pub out_type: String,
    pub out_payload: Vec<u8>,
    pub in_file: Input,
    pub out_file: Input,

    // 选择项
    pub choose_input_idx: usize, // 0=文件 1=摄像头
    pub output_idx: usize,       // 0=文件 1=二维码

    // 扫码进度（由事件循环填充为文本行）
    pub scan_lines: Vec<String>,

    // 二维码动画
    pub qr_frames: Vec<String>,
    pub qr_idx: usize,

    pub done_msg: String,
    pub camera_available: bool,
}

impl App {
    pub fn new(network: Network, default_keystore: Option<String>, camera_available: bool) -> Self {
        let network_idx = NETWORKS.iter().position(|n| *n == network).unwrap_or(0);
        App {
            screen: Screen::Setup,
            should_quit: false,
            error: None,
            keystore: Input::text(default_keystore.unwrap_or_default()),
            password: Input::masked(),
            passphrase: Input::masked(),
            network_idx,
            setup_field: SetupField::Keystore,
            wallet: None,
            job: None,
            summary: String::new(),
            out_type: String::new(),
            out_payload: Vec::new(),
            in_file: Input::text(""),
            out_file: Input::text(""),
            choose_input_idx: 0,
            output_idx: 1,
            scan_lines: Vec::new(),
            qr_frames: Vec::new(),
            qr_idx: 0,
            done_msg: String::new(),
            camera_available,
        }
    }

    pub fn network(&self) -> Network {
        NETWORKS[self.network_idx]
    }

    // ---------- 键处理 ----------

    pub fn on_key(&mut self, key: KeyEvent) -> Action {
        match self.screen {
            Screen::Setup => self.on_key_setup(key),
            Screen::ChooseInput => self.on_key_choose(key),
            Screen::FilePath => self.on_key_file(key, /*input=*/ true),
            Screen::Scanning => self.on_key_scanning(key),
            Screen::Verify => self.on_key_verify(key),
            Screen::OutputChoose => self.on_key_output(key),
            Screen::OutFile => self.on_key_file(key, /*input=*/ false),
            Screen::ShowQr => self.on_key_showqr(key),
            Screen::Done => {
                self.should_quit = true;
                Action::Quit
            }
        }
    }

    fn on_key_setup(&mut self, key: KeyEvent) -> Action {
        self.error = None;
        match key.code {
            KeyCode::Esc => {
                self.should_quit = true;
                return Action::Quit;
            }
            KeyCode::Tab | KeyCode::Down => self.setup_field = next_field(self.setup_field),
            KeyCode::BackTab | KeyCode::Up => self.setup_field = prev_field(self.setup_field),
            KeyCode::Left if self.setup_field == SetupField::Network => {
                self.network_idx = (self.network_idx + NETWORKS.len() - 1) % NETWORKS.len();
            }
            KeyCode::Right if self.setup_field == SetupField::Network => {
                self.network_idx = (self.network_idx + 1) % NETWORKS.len();
            }
            KeyCode::Backspace => match self.setup_field {
                SetupField::Keystore => self.keystore.backspace(),
                SetupField::Password => self.password.backspace(),
                SetupField::Passphrase => self.passphrase.backspace(),
                SetupField::Network => {}
            },
            KeyCode::Char(c) => match self.setup_field {
                SetupField::Keystore => self.keystore.push(c),
                SetupField::Password => self.password.push(c),
                SetupField::Passphrase => self.passphrase.push(c),
                SetupField::Network => {}
            },
            KeyCode::Enter => self.try_unlock(),
            _ => {}
        }
        Action::None
    }

    fn try_unlock(&mut self) {
        let blob = match std::fs::read(self.keystore.value.trim()) {
            Ok(b) => b,
            Err(e) => {
                self.error = Some(format!("读取 keystore 失败: {e}"));
                return;
            }
        };
        match ops::load_wallet(&blob, &self.password.value, &self.passphrase.value, self.network()) {
            Ok(w) => {
                self.wallet = Some(w);
                self.screen = Screen::ChooseInput;
            }
            Err(e) => self.error = Some(format!("解锁失败: {e}")),
        }
    }

    fn on_key_choose(&mut self, key: KeyEvent) -> Action {
        self.error = None;
        match key.code {
            KeyCode::Esc => self.screen = Screen::Setup,
            KeyCode::Up | KeyCode::Down => self.choose_input_idx ^= 1,
            KeyCode::Enter => {
                if self.choose_input_idx == 0 {
                    self.screen = Screen::FilePath;
                } else if self.camera_available {
                    self.scan_lines.clear();
                    self.screen = Screen::Scanning;
                    return Action::StartScan;
                } else {
                    self.error = Some("本二进制未编译 camera 特性；请用文件通道，或用 --features camera 重新构建".into());
                }
            }
            _ => {}
        }
        Action::None
    }

    // input=true → 读入待签文件（in_file）；input=false → 输出文件（out_file）。
    fn on_key_file(&mut self, key: KeyEvent, input: bool) -> Action {
        self.error = None;
        let field = if input { &mut self.in_file } else { &mut self.out_file };
        match key.code {
            KeyCode::Esc => {
                self.screen = if input { Screen::ChooseInput } else { Screen::OutputChoose };
            }
            KeyCode::Backspace => field.backspace(),
            KeyCode::Char(c) => field.push(c),
            KeyCode::Enter => {
                if input {
                    self.load_input_file();
                } else {
                    self.write_output_file();
                }
            }
            _ => {}
        }
        Action::None
    }

    fn load_input_file(&mut self) {
        let path = self.in_file.value.trim().to_string();
        match file_channel::read_signing_input(Path::new(&path)) {
            Ok((ur_type, payload)) => self.enter_verify(&ur_type, &payload),
            Err(e) => self.error = Some(format!("读取待签文件失败: {e}")),
        }
    }

    /// 由文件或扫码得到 `(ur_type, payload)` 后：解析 + 生成核对摘要 → Verify。
    pub fn enter_verify(&mut self, ur_type: &str, payload: &[u8]) {
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

    fn on_key_scanning(&mut self, key: KeyEvent) -> Action {
        if key.code == KeyCode::Esc {
            // 取消：回到选择屏，事件循环会据此停止扫码线程。
            self.screen = Screen::ChooseInput;
        }
        Action::None
    }

    /// 事件循环推送的扫码进度行（保留最近若干条）。
    pub fn push_scan_line(&mut self, line: String) {
        self.scan_lines.push(line);
        let n = self.scan_lines.len();
        if n > 8 {
            self.scan_lines.drain(0..n - 8);
        }
    }

    /// 扫码线程结束：Some=成功、None=取消、Err=出错。
    pub fn on_scan_done(&mut self, result: anyhow::Result<Option<(String, Vec<u8>)>>) {
        // 若用户已离开扫描屏（取消），忽略结果。
        if self.screen != Screen::Scanning {
            return;
        }
        match result {
            Ok(Some((ur_type, payload))) => self.enter_verify(&ur_type, &payload),
            Ok(None) => self.screen = Screen::ChooseInput,
            Err(e) => {
                self.error = Some(format!("扫码失败: {e}"));
                self.screen = Screen::ChooseInput;
            }
        }
    }

    fn on_key_verify(&mut self, key: KeyEvent) -> Action {
        self.error = None;
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') => self.screen = Screen::ChooseInput,
            KeyCode::Char('y') | KeyCode::Enter => self.do_sign(),
            _ => {}
        }
        Action::None
    }

    fn do_sign(&mut self) {
        let wallet = match &self.wallet {
            Some(w) => w,
            None => return,
        };
        let job = match &self.job {
            Some(j) => j,
            None => return,
        };
        match ops::sign(wallet, job) {
            Ok((ur_type, payload)) => {
                self.out_type = ur_type;
                self.out_payload = payload;
                self.screen = Screen::OutputChoose;
            }
            Err(e) => self.error = Some(format!("签名失败: {e}")),
        }
    }

    fn on_key_output(&mut self, key: KeyEvent) -> Action {
        self.error = None;
        match key.code {
            KeyCode::Esc => self.screen = Screen::Verify,
            KeyCode::Up | KeyCode::Down => self.output_idx ^= 1,
            KeyCode::Enter => {
                if self.output_idx == 0 {
                    self.screen = Screen::OutFile;
                } else {
                    self.build_qr_frames();
                    self.qr_idx = 0;
                    self.screen = Screen::ShowQr;
                }
            }
            _ => {}
        }
        Action::None
    }

    fn build_qr_frames(&mut self) {
        let frag = 180usize;
        // 帧数取「payload 分片数 × 3」，小数据退化为少数帧循环。
        let parts = ((self.out_payload.len() / frag) + 1) * 3;
        let parts = parts.clamp(8, 64);
        match btc_wallate_core::airgap::encode_parts(&self.out_type, &self.out_payload, frag, parts) {
            Ok(frames) => self.qr_frames = frames,
            Err(e) => {
                self.error = Some(format!("生成二维码失败: {e}"));
                self.qr_frames = vec![btc_wallate_core::airgap::encode_single(
                    &self.out_type,
                    &self.out_payload,
                )];
            }
        }
    }

    fn write_output_file(&mut self) {
        let path = self.out_file.value.trim().to_string();
        match file_channel::write_signed(Path::new(&path), &self.out_type, &self.out_payload) {
            Ok(()) => {
                self.done_msg = format!("已签名，结果写入 {path}");
                self.screen = Screen::Done;
            }
            Err(e) => self.error = Some(format!("写入失败: {e}")),
        }
    }

    fn on_key_showqr(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.done_msg = "已显示签名结果二维码，请在手机端扫码并广播。".into();
                self.screen = Screen::Done;
            }
            _ => {}
        }
        Action::None
    }

    /// 二维码动画走一帧（事件循环定时调用）。
    pub fn qr_tick(&mut self) {
        if !self.qr_frames.is_empty() {
            self.qr_idx = (self.qr_idx + 1) % self.qr_frames.len();
        }
    }

    pub fn current_qr_frame(&self) -> Option<&str> {
        self.qr_frames.get(self.qr_idx).map(|s| s.as_str())
    }
}

fn next_field(f: SetupField) -> SetupField {
    match f {
        SetupField::Keystore => SetupField::Password,
        SetupField::Password => SetupField::Passphrase,
        SetupField::Passphrase => SetupField::Network,
        SetupField::Network => SetupField::Keystore,
    }
}
fn prev_field(f: SetupField) -> SetupField {
    match f {
        SetupField::Keystore => SetupField::Network,
        SetupField::Password => SetupField::Keystore,
        SetupField::Passphrase => SetupField::Password,
        SetupField::Network => SetupField::Passphrase,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn k(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn app() -> App {
        App::new(Network::Signet, Some("wallet.ks".into()), true)
    }

    #[test]
    fn setup_field_cycles_and_network_selects() {
        let mut a = app();
        assert_eq!(a.setup_field, SetupField::Keystore);
        a.on_key(k(KeyCode::Tab));
        assert_eq!(a.setup_field, SetupField::Password);
        a.on_key(k(KeyCode::BackTab));
        assert_eq!(a.setup_field, SetupField::Keystore);

        // 走到 Network 字段，左右切换。
        a.setup_field = SetupField::Network;
        let start = a.network_idx;
        a.on_key(k(KeyCode::Right));
        assert_eq!(a.network_idx, (start + 1) % NETWORKS.len());
        a.on_key(k(KeyCode::Left));
        assert_eq!(a.network_idx, start);
    }

    #[test]
    fn typing_edits_focused_field_and_password_is_masked() {
        let mut a = app();
        a.setup_field = SetupField::Password;
        for c in "s3cr3t".chars() {
            a.on_key(k(KeyCode::Char(c)));
        }
        assert_eq!(a.password.value, "s3cr3t");
        assert_eq!(a.password.display(), "******");
        a.on_key(k(KeyCode::Backspace));
        assert_eq!(a.password.value, "s3cr3");
    }

    #[test]
    fn choose_input_toggles_and_camera_gate() {
        let mut a = App::new(Network::Signet, None, false); // 无摄像头
        a.screen = Screen::ChooseInput;
        a.choose_input_idx = 1; // 选摄像头
        let act = a.on_key(k(KeyCode::Enter));
        assert_eq!(act, Action::None);
        assert!(a.error.is_some(), "无 camera 特性应报错而非进入扫描");
        assert_eq!(a.screen, Screen::ChooseInput);

        // 有摄像头时应请求启动扫描。
        let mut b = app();
        b.screen = Screen::ChooseInput;
        b.choose_input_idx = 1;
        assert_eq!(b.on_key(k(KeyCode::Enter)), Action::StartScan);
        assert_eq!(b.screen, Screen::Scanning);
    }

    #[test]
    fn verify_confirm_requires_wallet_and_job() {
        // 没有 wallet/job 时 do_sign 应安全无操作（不 panic）。
        let mut a = app();
        a.screen = Screen::Verify;
        a.on_key(k(KeyCode::Char('y')));
        assert_eq!(a.screen, Screen::Verify); // 未推进
    }

    #[test]
    fn done_quits_on_any_key() {
        let mut a = app();
        a.screen = Screen::Done;
        assert_eq!(a.on_key(k(KeyCode::Char('x'))), Action::Quit);
        assert!(a.should_quit);
    }
}
