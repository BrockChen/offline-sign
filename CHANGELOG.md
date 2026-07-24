# Changelog

本项目为 **btc-wallate**：运行在永久断网 x86 Linux 上的离线空气隙签名机，
从单一 BIP-39 助记词管理 BTC 与 ETH，配合手机上的现成观察钱包完成交易。

开发与测试在 macOS 上进行；验证通过后交叉编译部署到 x86 Linux（`x86_64-unknown-linux-gnu`）。

格式参考 [Keep a Changelog](https://keepachangelog.com/)，版本遵循语义化版本。

## [Unreleased]

### iOS App：过审可测性修正 + 关于页 + 上架材料
- **过审可测性修正（重要）**：Release 曾把「粘贴 UR」入口设为 `#if DEBUG`，导致审核员（Distribution 构建）
  只剩「扫码签名」而无法完成签名流程（触发 Guideline 2.1）。现将其转为**正式功能**「手动输入交易」
  （Release 保留），仅**预填示例内容**保持 DEBUG；审核员按审核备注粘贴示例 UR 即可跑通。
- **关于与免责声明页**（`AboutVC`）：设置 → 关于进入，展示非托管 / 离线 / 不兑换 / 风险自负等合规声明。
- **上架材料文档** `firmware/ios-poc/APP_STORE.md`：App 描述（中/英）、隐私政策（中/英）、
  隐私问卷答案、**审核备注模板**（含 BIP-39 测试助记词 + 示例 UR + 步骤）、提交前自检清单；
  置顶标注 3.1.5(b) 组织账号前提。
- 导航栏设置入口由文字「设置」改为「•••」more 图标（+ accessibilityLabel）。
- 验证：Debug/Release 均 BUILD SUCCEEDED；模拟器截图确认关于页 / 手动输入入口。

### iOS App：深色硬件钱包风 UI 重设计 + App Store 过审硬化
- **深色主题**：`app.swift` 新增 `Theme` 设计令牌（近黑藏蓝底 + 比特币橙）+ 组件层
  （`primaryButton`/`outlineButton`/`card`/`field`/`pill`/`AddressRow`/`BrandMark`/`toast`），
  全局导航栏/状态栏深色化，`BaseVC` 统一底色与浅色状态栏。
- **逐屏重做**：Setup/Unlock（盾牌+₿ logo + 橙主按钮）、Home（网络胶囊徽标 + BTC/ETH 地址卡片带复制）、
  Settings（分组卡片 + 危险区）、Scan（取景框）、Verify（琥珀警示条 + 收据卡）、Result（白底二维码卡）。
- **品牌资产**：`gen-icons.swift`（CoreGraphics+CoreText 矢量重绘）生成 `Assets.xcassets/AppIcon.appiconset`
  全尺寸图标（1024 无 alpha）+ `LaunchLogo.imageset`；新增 `LaunchScreen.storyboard`（深色启动屏）。
- **App Store 过审硬化**：真实 bundle id `com.ecohash.btcwallate`、`ITSAppUsesNonExemptEncryption=NO`（导出合规豁免）、
  权限文案本地化（`Resources/{en,zh-Hans}.lproj/InfoPlist.strings`）、测试脚手架（预填/粘贴入口/DEMO 截图入口）
  全部 `#if DEBUG` 隔离、零联网（隐私问卷可全选“不收集”）；README 增「App Store 上架清单」章节。
- 验证：iOS `xcodebuild` Debug/Release 均 **BUILD SUCCEEDED**；模拟器逐屏截图确认深色主题与图标；
  `esp-signer-core` 测试不受影响（Rust 侧零改动）。

### iOS App：设置页 + 进入即解锁 + 中英切换 + 默认主网
- `app.swift` 重构导航：**UnlockVC**（钱包存在时的入口，口令框自动聚焦）→ HomeVC（精简为概览+扫码/粘贴/设置）；
  **SettingsVC**（网络 主网/测试网、语言 跟随系统/中/英、重置钱包需口令确认）；AppDelegate 依 keystore/解锁态
  选择根（`makeRoot`），语言切换后 `rebuildRoot` 重建界面。
- 默认**主网**：`Session.net` 默认 0，启动从 UserDefaults `"net"` 载入、设置页改动持久化。
- i18n：`t(zh,en)` + `L10n`（UserDefaults `"lang"`，缺省跟随系统）；所有 Swift 文案双语。
- Rust：`ops::summarize` 加 `en: bool`、`escore_summarize` 加 `lang: u8`（`escore.h`/Swift 同步），核对文本随语言中/英。
- 重置钱包：设置页按钮 → 口令密文输入 → `escore_wallet_info` 校验，成功才 `KC.clear()`。
- 验证：`cargo test -p esp-signer-core` 31 tests 零警告；iOS `xcodebuild BUILD SUCCEEDED`，模拟器安装启动显示真实界面。

### iOS：完整 UIKit 签名机 App（模拟器编译+运行通过）
- `esp-signer-core/ffi.rs`：C-ABI 扩展——`escore_import_mnemonic`（校验+加密成 keystore blob）、
  `escore_wallet_info`（解锁→BTC/ETH 地址）、`escore_summarize`（解析→屏幕核对文本）、`escore_sign`
  （解锁+解析+签名→签名结果 UR；种子只在函数内瞬时存在）、`escore_sample_unsigned`（示例，供无摄像头测试）。
  约定 `>=0` 成功/`<0` 失败。host 测试 `ffi_import_then_sign_eth` 端到端跑通。
- `firmware/ios-poc/app.swift`：完整 UIKit App（iOS 12：无 SwiftUI/async）——导入助记词(存 Keychain)→
  解锁+概览→扫码(AVFoundation)/粘贴 UR→屏幕核对→Touch ID(LocalAuthentication)→签名→CoreImage 二维码。
- `escore.h`/`project.yml` 更新（相机/FaceID 用途、bridging header）；`xcodebuild BUILD SUCCEEDED`，
  模拟器安装启动显示真实「导入钱包」界面。全仓 core 31 tests、零警告。

### iOS：Rust 核心已在 UIKit App（iOS 12 兼容）跑通（模拟器 PoC）
- `esp-signer-core` lib crate-type 加 `staticlib`（iOS 静态链接）；`aarch64-apple-ios`(真机) 与
  `aarch64-apple-ios-sim`(模拟器) 均秒编，无 Android 那种 API 底线问题。
- `firmware/ios-poc/`：UIKit（**非 SwiftUI**，SwiftUI 需 iOS13+；**无 async**，需 iOS15+）最小 App，
  经 C 头 bridging 调 `escore_probe`，部署目标 iOS 12.2（兼容 iPhone 6/iOS 12.5.8）。
- 模拟器实测：App 显示 `BIP-84[0]=bc1qcr8...306fyu`（官方向量一致）→ Rust↔Swift↔iOS 打通。
- Xcode 16.4 SDK 最低部署目标 12.0（可覆盖 iOS 12.5.8）；静态库仅需链 `Security`+`CoreFoundation`。
- 真机分发：免费 Apple ID 的 Personal Team 签名（证书 7 天、需每周重装）。全仓 71 tests 不变。

### Android：Rust 核心已在 Qin 1S（Android 4.4.4/armv7）真机跑通
- 新增 `firmware/signer-probe`（运行时探针 bin）与 `esp-signer-core` 的 cdylib crate-type + `ffi.rs`
  （C-ABI `escore_probe`：走 BIP-39→BIP-84 派生并返回地址）。
- 实测结论：Android 4.4 低于 Rust std 的 API 底线（bin 缺 `signal`；.so 缺 `dl_iterate_phdr`，均 API 21+）。
  **JNI 的 cdylib `.so`（`panic=abort`）可链接**；再为 `#[cfg(target_os="android")]` 提供一个
  `dl_iterate_phdr` 空桩（std backtrace 用不到），`.so` 即可在 4.4 上 `dlopen` 成功——对真实 APK 同样有效。
- 已在 Qin 1S 真机验证：`.so` + C 加载器返回 `bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu`
  （BIP-84 官方向量，逐字节一致）→ **纯 Rust 密钥核心可在该设备运行**。
- 交叉编译用 NDK r25c 的 `armv7a-linux-androideabi19-clang`。全仓 71 tests、零警告不变。

### ESP32/Android 固件：设备操作层 + keystore（host 可验证）
- `firmware/signer-core/keystore.rs`：移植 Argon2id + XChaCha20-Poly1305 助记词加密（解锁用）。
- `firmware/signer-core/ops.rs`：设备/App 要调的高层 API——`unlock`（keystore→种子）、`parse_unsigned`
  （识别 adb push 进来的 `ur:` / 二进制 PSBT / base64 PSBT）、`summarize`（屏幕核对：BTC 输出+手续费、
  ETH chainId/to/value/gas）、`sign` / `sign_to_ur_frames`（→ crypto-psbt / eth-signature 的 UR 二维码帧）。
- `btc.rs`：新增 `summarize_psbt`（输出金额/地址 + 手续费；bech32 v0 地址）。
- 依赖：`argon2`/`chacha20poly1305`/`getrandom`/`base64`（均纯 Rust，可嵌入）。
- 测试：解锁 round-trip；BTC 从二进制 PSBT → 摘要(金额/手续费) → 签名 → crypto-psbt UR → 解回含 partial_sig；
  ETH 从 ur:eth-sign-request 文本 → 摘要 → eth-signature；拒绝垃圾输入。全仓 71 tests、零警告。
- 说明：面向无摄像头设备（如 Duoqin Qin 1S）「adb push 文件进 + 屏幕二维码出」的流程，逻辑层已就绪；
  剩余为 Android(JNI/framework-only APK) 平台胶水，且需先验证 Android 4.4/armv7 上 Rust `.so` 可运行。

### ESP32 固件移植 Phase C.2（BTC）：PSBT + BIP-143 签名（host 可验证）
- `firmware/signer-core/btc.rs`：纯 Rust、不依赖 rust-bitcoin——最小 PSBT(BIP-174) 解析（每段解析为原始
  key-value 列表，忠实保留所有字段）+ 从 bip32_derivation 反推路径并匹配我方公钥 + P2WPKH **BIP-143 sighash**
  + k256 DER 签名（SIGHASH_ALL）+ 回填 partial_sig(0x02) + 原样重组 PSBT。
- 交叉验证：**固件签名与 x86 rust-bitcoin 的 partial_sig 逐字节一致**（同一可签 PSBT）；无本钱包输入时报错。
- 至此两条链的嵌入式签名核心全部完成并与 x86 版交叉验证一致。全仓 63 tests、零警告。

### ESP32 固件移植 Phase C（ETH）：airgap 移植 + ETH 签名（host 可验证）
- `firmware/signer-core/airgap/`：从 `crates/core/src/airgap` 移植 UR 分帧 + CBOR registry
  （`eth-sign-request`/`eth-signature`/`crypto-hdkey`/`crypto-keypath`），`crypto-psbt` 改为纯字节版
  （不依赖 rust-bitcoin）。依赖 `ur` + `minicbor`（可嵌入）。
- `firmware/signer-core/eth.rs`：ETH 签名（`keccak256(sign_data)` + k256 可恢复签名 → 65 字节 r‖s‖v，
  v=0/1）；`sign_request` 按派生路径签；最小手写 RLP 解码 EIP-1559 供屏幕核对（chainId/nonce/to/value/gas）。
- `derive.rs`：新增 `eth_privkey` 导出 ETH 私钥字节。
- 交叉验证（dev-dep 引 x86 core）：**固件派生私钥与 x86 rust-bitcoin 逐字节一致**；签名可恢复出派生地址；
  用真实 MetaMask `eth-sign-request` 跑通「解码→摘要→签名→eth-signature」；airgap 黄金向量全过。
- 全仓 61 tests、零警告。待续 Phase C.2：BTC PSBT 手写解析 + BIP-143 签名。

### ESP32 固件移植 Phase B：纯 Rust 密钥核心（host 可验证）
- 新增 workspace 成员 `firmware/signer-core`（`esp-signer-core`）：不依赖 rust-bitcoin/alloy 的 C 版 secp256k1，
  改用纯 Rust `bip39` + `bip32`(k256) + `sha2`/`ripemd`/`sha3`/`bech32`，可编进 esp-idf(std)、也能在 PC 单测。
- 实现：BIP-39 种子、BTC BIP-84 (bech32) 地址、ETH BIP-44 (EIP-55) 地址。
- 测试：对齐 BIP-84 官方向量与 Hardhat ETH 向量**逐字节一致**（= 与 x86 版 core 产出相同），验证纯 Rust
  派生的正确性；testnet→tb1、passphrase 生效。零警告。
- 文档：`firmware/README.md`（安全声明：非防篡改，限学习/测试网/小额；后续阶段路线）。

### 修复：eth-sign-request data-type 枚举值弄反（导致 EIP-1559 交易被误判为 typed-data 拒签）
- 现象：扫 MetaMask/imToken 的交易签名二维码报「暂不支持解码 data-type TypedData」。
- 根因：Keystone `ur-registry-eth` 的 data-type 实为 `1=transaction, 2=typed-data, 3=personal-message,
  4=typed-transaction`；此前把 `2=TypedTransaction / 4=TypedData` 写反，使 data-type=4 的 EIP-1559
  交易被判成 typed-data 而拒签。已按规范修正 `DataType` 的数值映射。
- 回归测试：用真实 MetaMask/imToken 的 `eth-sign-request`（Sepolia EIP-1559）断言解为 TypedTransaction、
  chainId=11155111、sign-data 以 0x02 开头、路径 m/44'/60'/0'/0/0。

### GUI：`--gui` 启动标志 + 一步步引导的解锁流程
- `main.rs`：新增全局 `--gui` 标志显式启动图形界面（未编 `gui` 特性则明确报错）；无 `--gui` 且无子命令 → 打印帮助。
- `gui/app.rs`：`Setup` 屏重设计为 `Welcome`——启动即检测 keystore 是否存在（`✅ 已找到`/`⚠ 未找到`），
  显示路径/网络/币种；存在则输入口令解锁，不存在则引导用 CLI `new`/`restore` 创建。
- 新增 `Overview` 概览页：解锁后显示 BTC 首收款地址 + ETH 地址供确认，再「开始签名」进入原流程。
- 顶部新增步骤条（1 解锁 · 2 选择交易 · 3 核对 · 4 输出）。
- 测试：`unlock_flow`（造临时 keystore → try_unlock → Overview + 地址断言）、缺 keystore 报错。

### 界面：撤销 ratatui TUI，改用 egui GUI（编译开关 `gui`，默认关）
- 移除 `crates/app/src/tui/` 与 `ratatui` 依赖。
- 新增 `crates/app/src/gui/`（`eframe`/`egui`，`gui` 特性）：签名流程 Setup→ChooseInput→
  File/Scanning→Verify→Output→Done，逻辑复用 `ops`；口令 password 掩码输入；二维码渲染为清晰
  可缩放的纹理图像；启动时尽力加载系统 CJK 字体以显示中文。
- `camera.rs`：`ScanEvent` 增加限速 `Preview{w,h,rgb}`（每~4帧），供 egui 扫码屏**实时预览**；
  CLI `scan_ur()` 忽略之（行为不变）。egui 后台线程跑 `scan_ur_cb`，`AtomicBool` 协作取消，
  窗口关闭经 `Drop` 停止线程。
- `main.rs`：无子命令时——`gui` 开→进 egui；`gui` 关→打印帮助提示。CLI 子命令全部保留。
- `gui`/`camera` 正交组合：`--features gui`（仅文件通道）、`--features "gui camera"`（含扫码+预览）。
- 测试：GuiApp 纯状态流转单测（初始屏/无 wallet 不签/enter_verify 报错/生成二维码帧）；
  默认、`--features gui`、`--features "gui camera"` 三种构建均通过零警告。

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

### Phase 3（部分）— export 命令 + 交叉编译配置 + 文档
- `derive.rs`：新增 `master_fingerprint()`。
- `ops.rs` + `main.rs`：新增 `export` 命令——BTC 导出**输出描述符**
  `wpkh([fp/84h/ch/ah]xpub/<0;1>/*)`（Sparrow/BlueWallet/Core/BDK 可直接导入），
  ETH 导出账户地址；均可选 `--qr`。补全「手机建观察钱包」这一环。
- `rust-toolchain.toml`：固定工具链 1.95.0（服务可复现构建）。
- `scripts/build-x86-linux.sh`：交叉编译到 `x86_64-unknown-linux-gnu`（优先 cross/docker，
  回退 zigbuild 或 x86 原生构建），并提示上机前核对二进制校验和。
- `README.md`：完整使用说明（架构/安全模型/构建/命令示例/交互流程图/互操作/路线图）。
- 全仓 36 tests 通过。

### Phase 3（部分）— 文件通道兼容 base64 + 测试网联调文档
- `file_channel.rs`：读入 PSBT 兼容 **base64 文本**（BlueWallet/Nunchuk/Sparrow 导出格式）
  与二进制 / `ur:` 三种；新增 `write_signed()`——签名结果为 crypto-psbt 时写 base64（钱包通用），
  其它类型写 UR。启用 bitcoin `base64` 特性。
- `docs/TESTNET.md`：Mac + iPhone 测试网联调详细步骤。BTC(signet) 全流程今日可跑
  （PSBT 文件进、二维码/文件出）；ETH(Sepolia) 标注待补（摄像头扫码 + crypto-multi-accounts 账户导出）。
- 测试：新增 base64 PSBT 文件签名往返。全仓 37 tests 通过、无警告。

### Phase 3（部分）— 摄像头扫码（camera 特性）
- `camera.rs`（`camera` 特性）：nokhwa 采集 + rqrr 解码，就地 RGB→灰度喂 `PartCollector`，
  收齐动画二维码分片后重组回 `(ur_type, payload)`；带分片进度反馈。
- `airgap::PartCollector::resolved_fragments()`：进度查询。
- CLI `sign` 新增 `--scan`（与 `--in` 二选一）：手机→签名机方向改用摄像头扫二维码，
  贴近真实空气隙。未编译 camera 特性时给出明确提示。
- 依赖：`nokhwa`（input-native）+ `rqrr`，均为 `camera` 可选特性，默认构建不含。
- 验证：默认与 `--features camera` 两种构建均通过、零警告；macOS 上 nokhwa 正常编译链接。
  实际扫码需真机 + 摄像头授权，在测试网联调时验证。
- 至此手机↔签名机**双向二维码**通路打通（BTC 全二维码流程可用）。

### Phase 3（部分）— TUI 交互（聚焦签名流程）
- 依赖：新增 `ratatui`（含 crossterm 后端，经 `ratatui::crossterm` 复用）。
- `camera.rs` 重构：新增 `ScanEvent` + `scan_ur_cb(cancel, on_event)`（事件回调 + 可取消），
  `scan_ur()` 改为薄封装（println 回调、不可取消），CLI `sign --scan` 行为不变。
- 新增 `tui/`：`app.rs`（状态机 `Setup→ChooseInput→FilePath/Scanning→Verify→OutputChoose→
  OutFile/ShowQr→Done`，逻辑全复用 `ops`）、`ui.rs`（ratatui 渲染）、`mod.rs`（`try_init`
  优雅失败、事件循环、摄像头后台线程经通道回主循环、Esc 协作取消、二维码动画轮播）。
- `main.rs`：`Cli.cmd` 改 `Option`，无子命令 → `tui::run`；全部 CLI 子命令保留（可脚本化）。
- 交互要点：口令/passphrase 掩码输入；**Verify 屏强制人工核对**后按 y 才签名；
  结果可写文件或显示动画二维码；非真实终端（管道）下 `try_init` 返回友好错误而非 panic。
- 测试：`App::on_key` 状态流转与输入域单测（字段切换/网络选择/口令掩码/camera 门控/Done 退出）；
  两种构建通过、零警告；全仓 43 tests 通过。TUI 渲染与终端循环需真实终端手动验证。

### Phase 2 收尾 — ETH 账户配对导出（crypto-multi-accounts）
- `airgap/eth.rs`：新增 `AccountKey` + `encode_multi_accounts`/`multi_accounts_to_ur_single`，
  按 Keystone 旧标签实现 `crypto-multi-accounts`(1103) 内含 `crypto-hdkey`(303) + `crypto-keypath`(304)；
  hdkey 携带账户级 `m/44'/60'/account'` 压缩公钥 + 链码 + origin(含 source-fingerprint) + parent-fingerprint。
- `ops.rs`：`export_watch_only` 的 ETH 分支改为输出 `crypto-multi-accounts` UR（供 MetaMask
  「连接硬件钱包 → QR」配对），收款地址仍由 `address --coin eth` 提供。
- 测试：`multi_accounts_wire_format` 字节级断言（tag 1103=0xD9 0x04 0x4F、内含 303/304）。全仓 44 tests。
- 文档：`docs/TESTNET.md` 补齐 ETH(Sepolia) 端到端步骤（配对→领币→构造→扫码核对→签名→广播）。
- 至此 BTC 与 ETH 两条链均可与手机现成钱包端到端联调（首次对具体 App 版本可能需按报错微调可选字段）。
