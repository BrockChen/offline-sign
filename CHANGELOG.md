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

### Phase 2（部分）— ETH 交易解析与签名
- 依赖：引入 `alloy`（consensus/eips/signers/signer-local），避免旧库对
  EIP-1559/2930/7702 交易可锻性校验缺失（CVE-2025-53359）。
- `derive.rs`：新增 `eth_secret_bytes()` 派生 ETH 账户私钥字节。
- `eth.rs`：
  - `summarize()`：解析 EIP-1559 交易的 chainId/nonce/to/value/gas，并解码 ERC-20
    `transfer(address,uint256)` calldata，供屏幕核对；未知 data 不做解码（提示勿盲签）。
  - `sign()`：用派生私钥签名，产出可广播的 EIP-2718 原始交易，并从签名+哈希恢复
    发送方做自检。
- 测试：ETH 转账摘要、ERC-20 transfer 解码、签名恢复地址 == 派生地址且原始交易前缀为 0x02。

### Phase 1+2 传输层 — BC-UR 动画二维码 + CBOR registry
- 依赖：`ur`（fountain code 分帧）、`minicbor`（手写 CBOR registry 类型）。
- `airgap/mod.rs`：UR 分帧层——`encode_single` / `encode_parts`（动画二维码）、
  `PartCollector` 增量收帧重组、`parse_type` 解析 UR 类型。传输通道无关（二维码/文件通用）。
- `airgap/psbt.rs`：`crypto-psbt`（BCR-2020-006）PSBT ↔ CBOR ↔ UR，
  与 Sparrow/Keystone/BlueWallet 同标准。
- `airgap/eth.rs`：按 ERC-4527 手写 `eth-sign-request`/`eth-signature` 与
  `crypto-keypath`（BCR-2020-007，tag 304）的 CBOR，字段键位对齐 Keystone `ur-registry-eth`。
- 测试：单帧/多帧 UR round-trip；crypto-psbt 完整二维码往返还原 PSBT；
  eth-sign-request/eth-signature/keypath CBOR round-trip；**字节级线格式断言**
  （map/tag37=0xD8 0x25/字节串长度头）锁死 RFC-8949 编码以保证与外部钱包互操作。
### Phase 2 收尾 — ETH 端到端胶水
- `airgap/eth.rs`：`KeyPath::eth_account_index()` 从派生路径安全提取 (account, index)，
  拒绝非常规路径（防误签）。
- `eth.rs`：
  - `summarize_sign_request()`：从 `eth-sign-request.sign_data`（`0x02 || rlp`）用
    alloy `TxEip1559::rlp_decode` 解出交易做屏幕核对；非交易类型不解码（不盲签）。
  - `sign_sign_request()`：按请求路径取私钥，对 `keccak256(sign_data)` 签名，返回 65 字节
    `r‖s‖v`（v = y-parity 0/1）；带 `address` 时核对派生地址一致，不符则拒签。
  - `handle_sign_request()`：串起「核对摘要 + 签名 + 产出 eth-signature 单帧 UR」。
- 测试：端到端 `eth-sign-request → 摘要 → 签名 → 恢复==派生地址`；完整 UR 闭环
  （请求动画二维码 → 收帧 → 处理 → eth-signature → 解回，request-id 原样带回、签名有效）；
  地址不匹配拒签。至此 **BTC 与 ETH 两条链的离线签名核心均已端到端打通（24 tests）**。

### Phase 3（部分）— 界面定为纯 CLI + 加密 keystore
- 决策：界面由 egui 改为**纯 CLI**——放私钥的离线设备，代码越少越好审计/复现，
  且可 headless 跑在最小 Linux TTY，无需图形栈；二维码用终端字符渲染、摄像头解码与
  是否有 GUI 无关。
- `keystore.rs`：助记词落盘加密。Argon2id 派生密钥 + XChaCha20-Poly1305 认证加密，
  自描述 blob（magic/version/salt/nonce/ciphertext），magic+version 作 AAD 防降级。
- 测试：加解密 round-trip、错误口令失败、密文篡改被认证检出、随机盐/nonce 使同明文两次加密不同。

### Phase 3（部分）— CLI 应用（文件通道 + 终端二维码）
- 新增 `crates/app`（bin `btc-wallate` + lib，lib 便于无交互单元测试）。
- `ops.rs`：无交互的操作层——`create_keystore`/`load_wallet`/`address`、`parse_job`
  （crypto-psbt / eth-sign-request）、`summarize`（BTC 输出/找零/手续费，ETH
  chainId/nonce/to/value/gas + ERC-20，未知 calldata 警示勿盲签）、`sign`。
- `qr.rs`：终端半块字符渲染二维码 + 动画逐帧显示。
- `file_channel.rs`：U盘/SD 文件通道——读 `.ur`（单/多帧自动分流）与原始 `.psbt`，
  写单条 `.ur`；配套 core 新增 `airgap::decode_single`。
- `main.rs`：clap CLI，命令 `new`/`restore`/`address`/`sign`；口令隐藏输入（rpassword）、
  签名前强制打印摘要并要求输入 `yes` 确认。
- 摄像头扫码列为可选特性 `camera`（默认关闭，目标机 x86 Linux 联调时开启），保持基础构建精简。
- 测试（app 6 + 集成 2）：keystore 建/载、生成 24 词、PSBT 解析/摘要/无输入拒签、
  未知 UR 类型拒绝、二维码渲染；**端到端**：可签 PSBT → 写文件 → `read_signing_input`
  → 签名 → 结果含 partial_sigs（原始 .psbt 与 .ur 文本两种文件都覆盖）。
- 全仓 35 tests 通过、无警告。CLI 二进制 `--help` 正常。
