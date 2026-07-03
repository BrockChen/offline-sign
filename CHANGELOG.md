# Changelog

本项目为 **btc-wallate**：运行在永久断网 x86 Linux 上的离线空气隙签名机，
从单一 BIP-39 助记词管理 BTC 与 ETH，配合手机上的现成观察钱包完成交易。

开发与测试在 macOS 上进行；验证通过后交叉编译部署到 x86 Linux（`x86_64-unknown-linux-gnu`）。

格式参考 [Keep a Changelog](https://keepachangelog.com/)，版本遵循语义化版本。

## [Unreleased]

### Phase 0 — workspace 骨架 + 密钥派生
- 建立 Cargo workspace 与 `crates/core`（纯逻辑、无 IO、可单元测试）。
- `seed.rs`：BIP-39 助记词生成/恢复、可选 passphrase、BIP-32 主私钥；`Debug` 屏蔽密钥材料。
- `derive.rs`：
  - BTC BIP-84 原生 segwit 地址（`m/84'/coin'/account'/change/index`，`bc1...`）。
  - ETH BIP-44 地址（`m/44'/60'/account'/0/index`，EIP-55 校验和）。
  - 账户级 xpub 导出（供手机观察钱包）。
- 测试：BTC 地址对照 BIP-84 官方向量、ETH 地址对照 Hardhat 向量、passphrase 生效、恢复确定性。

### Phase 1（部分）— BTC 交易解析与签名
- `btc.rs`：
  - `summarize()`：解析 PSBT 输出/手续费/找零识别，作为签名前屏幕核对的数据源
    （空气隙防偷换收款地址的核心安全属性）。
  - `sign()`：用主私钥对本钱包拥有的输入签名（委托 rust-bitcoin，不自实现密码学）。
- 测试：摘要正确识别外部输出/找零/手续费；签名后写入 partial_sigs。
- 待续：crypto-psbt / crypto-account 的 UR 二维码编解码。
