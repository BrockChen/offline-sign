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

**结论：BTC（signet）与 ETH（Sepolia）现在都可与手机现成钱包端到端联调**（BTC 见下，ETH 见文末）。
未签名数据走文件或摄像头扫码进 Mac，签名结果用二维码/文件回手机。ETH 的 MetaMask 配对用
`crypto-multi-accounts`、签名往返用 ERC-4527，均已实现（首次对具体 App 版本联调可能需按报错微调）。

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

## ETH（Sepolia）完整步骤

iPhone 端用 **MetaMask 移动版**（以「连接硬件钱包 → QR / Keystone」方式接入本签名机）。
本机需带摄像头构建：`cargo build --release --features camera`。网络在 MetaMask 里选 **Sepolia**。

### 1. 账户配对（一次性）
```bash
$BW export --keystore signet.ks --coin eth --qr
```
- 输出 `ur:crypto-multi-accounts/...` 配对二维码（含账户级 `m/44'/60'/0'` 扩展公钥）。
- MetaMask：账户菜单 →「连接硬件钱包」→ 选 **QR / Keystone** → 扫这个二维码 → 选择要导入的 ETH 账户。
- 导入后 MetaMask 显示的地址应与 `$BW address --coin eth` 一致。
  （keystore 与网络无关，ETH 用哪把 `--network` 都行；这里沿用 signet 的 keystore 即可。）

### 2. 领 Sepolia 测试币
- 打开水龙头（如 https://cloud.google.com/application/web3/faucet/ethereum/sepolia ），
  发到 `$BW address --coin eth` 显示的地址；在 https://sepolia.etherscan.io 查到账。

### 3. 在 MetaMask 发起交易（Sepolia）
- 发一笔 ETH（或 ERC-20）给任意 Sepolia 地址，确认时 MetaMask 弹出
  **未签名交易的动画二维码**（`eth-sign-request`）。

### 4. 用摄像头扫入并核对签名
```bash
$BW sign --keystore signet.ks --scan
```
- Mac 摄像头扫 MetaMask 屏幕上的动画二维码 → 打印 **ETH 交易核对**（chainId=11155111、收款地址、金额、gas；
  ERC-20 会解出代币/收币/数量，未知 calldata 会警示勿盲签）。核对无误后确认签名。
- 签名机显示 `eth-signature` 二维码（一帧即可）。

### 5. 回 MetaMask 广播
- MetaMask 里点「扫描签名」→ 用 iPhone 扫 Mac 屏幕上的 `eth-signature` 二维码 → 广播。
- 回 https://sepolia.etherscan.io 查交易确认。

> 互操作说明：账户配对用的是 Keystone 兼容的 `crypto-multi-accounts`（旧标签 303/304/1103），
> 签名往返用 [ERC-4527](https://eips.ethereum.org/EIPS/eip-4527) 的 `eth-sign-request`/`eth-signature`。
> 均已按规范实现并有字节级断言，但**首次对具体 MetaMask 版本联调时若配对/签名被拒，把 MetaMask 的报错发来**，
> 多半是某个可选字段（如 use-info / 路径层级）的细节，按需微调即可。

---

## 常见问题

- **地址对不上**：确认 CLI `--network signet` 与 iPhone 钱包网络一致；两边都应是 BIP-84 (`tb1...`)。
- **iPhone 扫不动 Mac 的动画二维码**：调大终端字号、降低 `--frag`（如 `--frag 120`）让每帧更稀疏，保持屏幕稳定。
- **导入 PSBT 报格式错**：本机接受 base64 / 二进制 PSBT / `ur:` 文本三种；若钱包只给二维码，可先存成文件再传。
