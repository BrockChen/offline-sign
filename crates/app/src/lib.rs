//! btc-wallate 离线签名机 CLI 的可测逻辑层。
//!
//! `main.rs` 负责命令行解析与交互（口令输入、确认提示）；本 lib 提供无交互、可单元测试的
//! 操作函数与两个传输实现（终端二维码显示、U盘/SD 文件通道）。摄像头扫码在 `camera`
//! 特性下提供（默认关闭，目标机联调时开启）。

pub mod file_channel;
pub mod ops;
pub mod qr;

#[cfg(feature = "camera")]
pub mod camera;

#[cfg(feature = "gui")]
pub mod gui;
