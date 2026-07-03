# 测试网联调（Mac + iPhone）

用一台 Mac（跑 `btc-wallate` 签名机）+ 一台 iPhone（现成观察钱包）在测试网走通
「领测试币 → 观察 → 构造 → 签名 → 广播」全流程。

> 测试阶段 Mac 并非真断网，仅验证功能与互操作；正式使用时签名机应在永久断网的 x86 上运行。

## 当前能力与约束（务必先读）

| 方向 | 通道 | 现状 |
|---|---|---|
| 手机 → Mac（未签名交易进） | **文件**（AirDrop/Files） | ✅ 可用（支持 base64 / 二进制 PSBT / `ur:` 文本） |
| 手机 → Mac | **摄像头扫二维码** | ✅ 可用（`--features camera` 构建，`sign --scan`） |
| Mac → 手机（结果出） | **二维码**（Mac 显示，iPhone 扫） | ✅ 可用 |
| Mac → 手机 | 文件（base64 PSBT） | ✅ 可用 |

**结论：**
- **BTC（signet）现在就能完整跑通** —— 未签名 PSBT 走文件或摄像头扫码进 Mac，签名结果用二维码/文件回 iPhone。
- **ETH（Sepolia）与 MetaMask 联调还差一块** —— 摄像头（`eth-sign-request` 扫入）已就绪，
  但 MetaMask「连接硬件钱包 → QR (Keystone)」配对时需扫 Keystone 账户导出 UR（`crypto-multi-accounts`），
  当前 `export --coin eth` 只给纯地址。补上账户导出后即可联调（见文末）。

> 摄像头权限（macOS）：首次 `sign --scan` 会弹出摄像头授权，需在「系统设置 → 隐私与安全性 → 摄像头」
> 放行你的终端 App（Terminal/iTerm）。用 `cargo build --release --features camera` 构建带摄像头的二进制。

---

## 准备

```bash
# 在 Mac 上构建（测试用 release 即可）
cargo build --release
BW=./target/release/btc-wallate
```

iPhone 端 BTC 观察钱包推荐 **Nunchuk**（支持 signet、描述符导入、PSBT 文件/二维码）。
BlueWallet 亦可（注意其网络与导入格式）。文件在 Mac↔iPhone 间用 **AirDrop** 或 iCloud「文件」App 传递。

---

## BTC（signet）完整步骤

### 1. 创建钱包（离线一步）
```bash
$BW --network signet new --keystore signet.ks --words 24
```
- 按提示设「keystore 口令」（两次）；「BIP-39 passphrase」测试时可直接回车留空。
- 屏幕会打印 24 词助记词 —— 测试钱包也请抄下（用于必要时在别处恢复核对）。

### 2. 导出观察钱包描述符 → 导入 iPhone
```bash
$BW --network signet export --keystore signet.ks --coin btc --qr
```
- 输出形如 `wpkh([<fp>/84h/1h/0h]tpub.../<0;1>/*)` 的描述符 + 二维码。
- iPhone Nunchuk：新增钱包 → 选「导入 / watch-only」→ 扫这个二维码（或粘贴文本）→ 网络选 **signet**。
- 导入后 iPhone 会显示与签名机**一致**的接收地址（BIP-84，`tb1...`）。

### 3. 领 signet 测试币
- 复制一个接收地址：iPhone 钱包里「收款」显示的地址，或用
  `$BW --network signet address --keystore signet.ks --coin btc --index 0 --qr`（两者相同）。
- 打开 signet 水龙头（如 https://signetfaucet.com ），粘贴该 `tb1...` 地址，领取。
- 在浏览器 https://mempool.space/signet 查地址，等 1 个确认（signet 出块约 10 分钟）。
- iPhone 观察钱包余额到账即成功。

### 4. 在 iPhone 上构造未签名交易
- Nunchuk 里发起一笔转账：收款地址填任意 signet 地址（例如再转回水龙头的返还地址，或自己另一个 `tb1`），
  填金额与手续费。
- 因为是 watch-only，钱包会生成**未签名 PSBT**。选「导出 / Export PSBT」→ 存到「文件」或直接 **AirDrop 到 Mac**
  （文件为 `.psbt` 二进制或 base64 文本均可）。

### 5. 在 Mac 上核对并签名
```bash
$BW --network signet sign --keystore signet.ks --in ~/Downloads/unsigned.psbt
```
- 终端打印交易摘要：各输出地址 / 金额 / 找零 / 手续费。**逐项核对无误**后输入 `yes`。
- 随后 Mac 以**动画二维码**显示已签名的 PSBT（`ur:crypto-psbt/...`）。
- 若更想用文件回传：加 `--out ~/Downloads/signed.psbt`（写 base64），再 AirDrop 回 iPhone。

> 全二维码流程（更贴近真实空气隙）：用 `cargo build --release --features camera` 构建，
> 第 5 步改用 `$BW --network signet sign --keystore signet.ks --scan`，让 Mac 摄像头直接扫
> iPhone 屏幕上的未签名 PSBT 动画二维码（Nunchuk 里选「导出为二维码」），无需文件传输。

### 6. 在 iPhone 上广播
- Nunchuk 里选「导入已签名 / 扫描」→ 用 iPhone 摄像头扫 Mac 屏幕上的动画二维码
  （或导入 AirDrop 过来的 `signed.psbt`）→ 广播。
- 回 https://mempool.space/signet 查交易，进入内存池/确认即成功。

至此 BTC 离线签名闭环在测试网跑通。

---

## ETH（Sepolia）—— 待补齐后再联调

Sepolia 测试币可在 https://cloud.google.com/application/web3/faucet/ethereum/sepolia 等水龙头领取
（发到 `$BW address --coin eth` 显示的地址）。与 iPhone MetaMask 的空气隙联调目前**还差一块**：

- ✅ **Mac 摄像头扫码已实现**（`--features camera`，`sign --scan`）：可扫入 MetaMask 的
  `eth-sign-request` 动画二维码，签名后以 `eth-signature` 二维码回显给 MetaMask 扫回。
- ⏳ **账户配对导出**：MetaMask「连接硬件钱包 → QR (Keystone)」配对时要扫 Keystone 的账户导出 UR
  （`crypto-multi-accounts`），当前 `export --coin eth` 只输出纯地址，MetaMask 不据此授权 QR 签名。

补上 `crypto-multi-accounts` 账户导出后，即可在 Sepolia 上对 MetaMask 实测
`eth-sign-request`/`eth-signature` 往返。core 侧的解析/签名/编码、以及摄像头采集均已就绪，
只剩这一段账户配对。

---

## 常见问题

- **地址对不上**：确认 CLI `--network signet` 与 iPhone 钱包网络一致；两边都应是 BIP-84 (`tb1...`)。
- **iPhone 扫不动 Mac 的动画二维码**：调大终端字号、降低 `--frag`（如 `--frag 120`）让每帧更稀疏，保持屏幕稳定。
- **导入 PSBT 报格式错**：本机接受 base64 / 二进制 PSBT / `ur:` 文本三种；若钱包只给二维码，可先存成文件再传。
