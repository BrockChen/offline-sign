# btc-wallate ESP32 固件（DIY 硬件签名器）

把离线签名机移植到 **ESP32-S3** 的实验性固件工程。**用途限定：学习 / 测试网 / 小额。**

> ⚠️ **非防篡改**。ESP32 的 flash 加密 + secure boot 已被公开的物理故障注入完整绕过
> （含 ESP32 V3 / C3 / C6），拿到设备的攻击者可提取种子。真正的安全依赖**不上机的 BIP-39 passphrase**、
> PIN 限速/擦除，以及**只放小额/测试网**。真实资金请用成品硬件钱包或专用安全元件。

## 为什么不直接复用 x86 版
x86 版 `crates/core` 依赖 rust-bitcoin（C 版 secp256k1）与 alloy（重、std 密集），不适合嵌入式。
固件改用**纯 Rust** 密码学（`bip32`+`k256`、`sha2`、`ripemd`、`sha3`、`bech32`、`bip39`），
可在 PC 上单元测试、也能编进 esp-idf(std) 目标。**空气隙线格式**（UR/CBOR：eth-sign-request/
eth-signature/crypto-hdkey）计划从 `crates/core/src/airgap/` 移植（`ur`/`minicbor` 支持嵌入式）。

## 进度
- ✅ **Phase B**：`firmware/signer-core` 纯 Rust 密钥核心——BIP-39 种子、BTC BIP-84、ETH BIP-44 地址；
  对齐 BIP-84 官方向量与 Hardhat 向量逐字节一致。
- ✅ **Phase C（ETH，本次）**：移植 `airgap`（UR 分帧 + CBOR：eth-sign-request/eth-signature/crypto-hdkey/
  crypto-keypath；crypto-psbt 改为字节版）；ETH 签名（`keccak(sign_data)` + k256 可恢复签名 → 65B r‖s‖v）；
  最小 RLP 解码 EIP-1559 供屏幕核对。验证：**私钥与 x86 版逐字节一致**、签名恢复出派生地址、真实 MetaMask
  请求解码/摘要/签名闭环、airgap 黄金向量全过（`cargo test -p esp-signer-core`，20 tests）。
- ✅ **Phase C.2（BTC，本次）**：`btc.rs` 手写最小 PSBT(BIP-174) 解析（保留全部 kv）+ P2WPKH
  **BIP-143 sighash** + k256 DER 签名 + 回填 partial_sig + 忠实重组 PSBT。验证：**固件签名与 rust-bitcoin
  逐字节一致**（`firmware_signature_matches_rust_bitcoin`），无本钱包输入时报错。
- ✅ **设备操作层 + keystore（本次）**：`keystore.rs`（Argon2id+XChaCha20 移植）；`ops.rs` 提供
  App/固件要调的高层 API——`unlock`（解密种子）、`parse_unsigned`（识别 adb push 进来的 ur/二进制/
  base64 PSBT）、`summarize`（BTC 输出/手续费、ETH 字段，供屏幕核对）、`sign`/`sign_to_ur_frames`
  （→ crypto-psbt / eth-signature 的 UR 二维码帧）；`btc.rs` 加 `summarize_psbt`。host 测试全绿（30 tests）。
- ➡️ 至此**「解锁→解析→核对→签名→UR 输出」整条设备逻辑在 PC 上验证与 x86 一致**。剩余为平台胶水：
  Android(JNI/APK) 或 ESP32(esp-idf 驱动/显示/存储)。

## Android（Duoqin Qin 1S 等）落地：adb push 进 + 屏幕二维码出
无摄像头设备的数据流：**手机导出未签名 PSBT 文件 → `adb push` 到设备 → App 内核对+签名 → 屏幕显示
crypto-psbt 二维码 → 手机扫回广播**。适用 **BTC**（Nunchuk/Sparrow 可导 PSBT 文件）；**ETH 受限**
（MetaMask 只出二维码、无文件，且无摄像头扫不进）。设备逻辑全在 `ops`，Android 侧只需 JNI 薄封装 +
framework-only UI + 手绘二维码 Bitmap。注意 Android 4.4/armv7 需先验证 Rust `.so` 能否编译并在真机运行。
- ⏳ Phase A/D/E：esp-idf 工程 + ST7789 显示 + microSD 输入 + PIN/指纹解锁 + NVS 加密存储 + TFT 核对页。
- ⏳ Phase F（可选）：OV2640 摄像头扫码入。

## 硬件（建议）
ESP32-S3-WROOM-1（8MB PSRAM）+ ST7789 SPI TFT + microSD（SPI，MVP 输入）+ 按键/旋钮（PIN）+
R307/AS608 指纹（UART，便利解锁）。详见 `/Users/.../.claude/plans` 中的移植计划。

## 构建/测试（当前可做）
```bash
cargo test -p esp-signer-core     # 纯 Rust 派生核心，对齐官方向量
```
（esp-idf 目标构建/刷机需安装 esp 工具链与真板子，后续阶段接入。）
