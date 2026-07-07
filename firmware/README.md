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
- ✅ **Phase B（本次）**：`firmware/signer-core` 纯 Rust 密钥核心——BIP-39 种子、BTC BIP-84 地址、
  ETH BIP-44 地址；**对齐 BIP-84 官方向量与 Hardhat 向量逐字节一致**（`cargo test -p esp-signer-core`，
  与 x86 版产出相同地址）。此层无需硬件即可在 PC 验证。
- ⏳ Phase C：移植 `airgap`（UR/CBOR）+ 交易签名（BTC BIP-143 sighash / ETH EIP-1559，k256 签名）。
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
