//! btc-wallate 离线签名机 CLI。
//!
//! 运行在**永久断网**的 x86 Linux（开发期在 macOS 上）。所有涉及资金的操作在签名前
//! 都会打印交易摘要并要求人工确认；私钥仅以加密 keystore 形式落盘。
//!
//! 命令：
//!   new       生成新助记词并加密存为 keystore
//!   restore   从已有助记词恢复并加密存为 keystore
//!   address   显示接收地址（可选二维码）
//!   sign      读入待签数据（文件）→ 屏幕核对 → 签名 → 输出（文件/二维码）
//!
//! 空气隙：本机严格遵循 BC-UR / ERC-4527 标准，手机端用现成的 Keystone 兼容观察钱包
//! （BTC: BlueWallet 等；ETH: MetaMask+Keystone）即可互操作。

use std::io::{self, Write};
use std::path::PathBuf;

use anyhow::{bail, Context};
use bitcoin::Network;
use clap::{Parser, Subcommand};

use btc_wallate_app::{file_channel, ops, qr};

#[derive(Parser)]
#[command(name = "btc-wallate", about = "离线空气隙签名机 (BTC + ETH)")]
struct Cli {
    /// 比特币网络（影响 BTC 地址与派生 coin_type；ETH 不受影响）。
    #[arg(long, global = true, default_value = "bitcoin")]
    network: String,
    /// 子命令。省略则进入交互式 TUI（聚焦签名流程）。
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// 生成新助记词并加密存为 keystore。
    New {
        /// keystore 输出路径。
        #[arg(long)]
        keystore: PathBuf,
        /// 助记词词数（12 或 24）。
        #[arg(long, default_value_t = 24)]
        words: usize,
    },
    /// 从已有助记词恢复并加密存为 keystore。
    Restore {
        #[arg(long)]
        keystore: PathBuf,
    },
    /// 显示接收地址。
    Address {
        #[arg(long)]
        keystore: PathBuf,
        /// 币种：btc 或 eth。
        #[arg(long, default_value = "btc")]
        coin: String,
        #[arg(long, default_value_t = 0)]
        account: u32,
        #[arg(long, default_value_t = 0)]
        index: u32,
        /// BTC 找零链（change=1）。
        #[arg(long, default_value_t = false)]
        change: bool,
        /// 同时以二维码显示（便于手机核对）。
        #[arg(long, default_value_t = false)]
        qr: bool,
    },
    /// 导出观察钱包凭据（BTC 输出描述符 / ETH 地址），供手机建只读钱包。
    Export {
        #[arg(long)]
        keystore: PathBuf,
        /// 币种：btc 或 eth。
        #[arg(long, default_value = "btc")]
        coin: String,
        #[arg(long, default_value_t = 0)]
        account: u32,
        /// 同时以二维码显示。
        #[arg(long, default_value_t = false)]
        qr: bool,
    },
    /// 读入待签数据 → 核对 → 签名 → 输出。
    Sign {
        #[arg(long)]
        keystore: PathBuf,
        /// 输入文件（base64 / 二进制 PSBT 或 `ur:` 文本）。与 --scan 二选一。
        #[arg(long)]
        r#in: Option<PathBuf>,
        /// 用摄像头扫描动画二维码读入（需 `--features camera` 构建）。与 --in 二选一。
        #[arg(long, default_value_t = false)]
        scan: bool,
        /// 输出文件（PSBT 写 base64，其它写 UR 文本）。省略则以二维码显示结果。
        #[arg(long)]
        out: Option<PathBuf>,
        /// 结果以动画二维码显示时的单帧最大字节数。
        #[arg(long, default_value_t = 180)]
        frag: usize,
    },
}

fn parse_network(s: &str) -> anyhow::Result<Network> {
    s.parse::<Network>()
        .with_context(|| format!("无法识别网络: {s}（用 bitcoin/testnet/signet/regtest）"))
}

fn prompt_line(prompt: &str) -> anyhow::Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

/// 读取 keystore 口令（隐藏输入）。
fn prompt_keystore_password(confirm: bool) -> anyhow::Result<String> {
    let pw = rpassword::prompt_password("keystore 口令: ")?;
    if confirm {
        let pw2 = rpassword::prompt_password("再次输入 keystore 口令: ")?;
        if pw != pw2 {
            bail!("两次口令不一致");
        }
    }
    if pw.is_empty() {
        bail!("口令不能为空");
    }
    Ok(pw)
}

/// 读取可选的 BIP-39 passphrase（第 25 词；隐藏输入，允许为空）。
fn prompt_bip39_passphrase() -> anyhow::Result<String> {
    Ok(rpassword::prompt_password(
        "BIP-39 passphrase（无则直接回车）: ",
    )?)
}

/// 摄像头扫码读入（按是否编译 camera 特性分派）。
#[cfg(feature = "camera")]
fn scan_input() -> anyhow::Result<(String, Vec<u8>)> {
    btc_wallate_app::camera::scan_ur()
}
#[cfg(not(feature = "camera"))]
fn scan_input() -> anyhow::Result<(String, Vec<u8>)> {
    anyhow::bail!("本二进制未编译 camera 特性；请用 `cargo build --features camera` 重新构建，或改用 --in <文件>")
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let network = parse_network(&cli.network)?;

    // 无子命令 → 进入交互式 TUI（聚焦签名流程）。
    let cmd = match cli.cmd {
        Some(c) => c,
        None => return btc_wallate_app::tui::run(network, None),
    };

    match cmd {
        Cmd::New { keystore, words } => {
            if keystore.exists() {
                bail!("{} 已存在，拒绝覆盖", keystore.display());
            }
            let pw = prompt_keystore_password(true)?;
            let (phrase, blob) = ops::create_keystore(None, words, network, &pw)?;
            std::fs::write(&keystore, &blob)
                .with_context(|| format!("写入 {} 失败", keystore.display()))?;
            println!("\n================ 助记词（务必离线抄写/金属备份，勿拍照/联网）================");
            println!("{phrase}");
            println!("==========================================================================");
            println!("\nkeystore 已写入 {}", keystore.display());
        }
        Cmd::Restore { keystore } => {
            if keystore.exists() {
                bail!("{} 已存在，拒绝覆盖", keystore.display());
            }
            let phrase = prompt_line("输入助记词（空格分隔）:\n")?;
            let pw = prompt_keystore_password(true)?;
            let (_p, blob) = ops::create_keystore(Some(&phrase), 0, network, &pw)?;
            std::fs::write(&keystore, &blob)
                .with_context(|| format!("写入 {} 失败", keystore.display()))?;
            println!("keystore 已写入 {}", keystore.display());
        }
        Cmd::Address {
            keystore,
            coin,
            account,
            index,
            change,
            qr: show_qr,
        } => {
            let blob = std::fs::read(&keystore)
                .with_context(|| format!("读取 {} 失败", keystore.display()))?;
            let pw = prompt_keystore_password(false)?;
            let passphrase = prompt_bip39_passphrase()?;
            let wallet = ops::load_wallet(&blob, &pw, &passphrase, network)?;
            let coin_is_btc = match coin.as_str() {
                "btc" | "BTC" => true,
                "eth" | "ETH" => false,
                other => bail!("未知币种: {other}（btc/eth）"),
            };
            let addr = ops::address(&wallet, coin_is_btc, account, change, index)?;
            println!("{addr}");
            if show_qr {
                qr::print(&addr)?;
            }
        }
        Cmd::Export {
            keystore,
            coin,
            account,
            qr: show_qr,
        } => {
            let blob = std::fs::read(&keystore)
                .with_context(|| format!("读取 {} 失败", keystore.display()))?;
            let pw = prompt_keystore_password(false)?;
            let passphrase = prompt_bip39_passphrase()?;
            let wallet = ops::load_wallet(&blob, &pw, &passphrase, network)?;
            let coin_is_btc = match coin.as_str() {
                "btc" | "BTC" => true,
                "eth" | "ETH" => false,
                other => bail!("未知币种: {other}（btc/eth）"),
            };
            let cred = ops::export_watch_only(&wallet, coin_is_btc, account)?;
            if coin_is_btc {
                println!("BTC 观察钱包输出描述符（导入 Sparrow/BlueWallet/Bitcoin Core）:");
            } else {
                println!("ETH 观察地址（在 MetaMask/区块浏览器观察，广播用现成钱包）:");
            }
            println!("{cred}");
            if show_qr {
                qr::print(&cred)?;
            }
        }
        Cmd::Sign {
            keystore,
            r#in,
            scan,
            out,
            frag,
        } => {
            let blob = std::fs::read(&keystore)
                .with_context(|| format!("读取 {} 失败", keystore.display()))?;
            let (ur_type, payload) = if scan {
                scan_input()?
            } else {
                let path = r#in.context("请用 --in <文件> 指定输入，或用 --scan 摄像头扫码")?;
                file_channel::read_signing_input(&path)?
            };
            let job = ops::parse_job(&ur_type, &payload)?;

            let pw = prompt_keystore_password(false)?;
            let passphrase = prompt_bip39_passphrase()?;
            let wallet = ops::load_wallet(&blob, &pw, &passphrase, network)?;

            // 强制人工核对。
            let summary = ops::summarize(&wallet, &job)?;
            println!("\n{summary}");
            let ans = prompt_line("确认无误并签名？输入 yes 继续: ")?;
            if ans != "yes" {
                bail!("已取消，未签名");
            }

            let (out_type, out_payload) = ops::sign(&wallet, &job)?;
            match out {
                Some(path) => {
                    file_channel::write_signed(&path, &out_type, &out_payload)?;
                    println!("已签名，结果写入 {}", path.display());
                }
                None => {
                    // 用动画二维码显示结果，供手机扫回广播。
                    let parts = btc_wallate_core::airgap::encode_parts(
                        &out_type,
                        &out_payload,
                        frag,
                        // 帧数取分片数的约 2 倍以增强抗丢帧；这里给一个足够的上限，
                        // 小数据会退化为少数帧循环。
                        16,
                    )?;
                    println!("按 Ctrl-C 结束显示。手机对准持续扫描：\n");
                    // 循环播放若干轮。
                    for round in 0..1000 {
                        let p = &parts[round % parts.len()];
                        qr::print_frame(p, round % parts.len(), parts.len())?;
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                }
            }
        }
    }
    Ok(())
}
