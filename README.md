# btc-wallate — 离线空气隙签名机（BTC + ETH）

在一台**永久断网**的 x86 设备上，用一个 BIP-39 助记词管理 BTC 与 ETH；配合手机上的
**现成观察钱包**完成交易。核心安全目标：**私钥永不触网**。

纯命令行（CLI），无图形依赖，可在最小化 Linux TTY 上运行。空气隙数据通过**二维码**或
**U盘/SD 文件**在两台设备间传递——只传数据，绝不联网。

> ⚠️ 安全与免责：本项目为自建冷钱包，涉及真实资金。请务必：审阅代码、可复现构建并核对二进制
> 校验和、离线（金属板）备份助记词、先小额测试网演练。作者不对资金损失负责。

---

## 架构

| 角色 | 设备 | 职责 |
|---|---|---|
| **签名机（冷）** | x86，永久断网 | 保管助记词/私钥、派生地址、对交易签名 |
| **观察钱包（热）** | 手机，联网 | 只读观察余额、构造未签名交易、广播已签名交易 |
| **空气隙通道** | 二维码 / U盘·SD 文件 | 只传数据、绝不联网 |

**关键设计：签名机严格遵循行业标准空气隙协议**，因此手机端直接用现成钱包即可，无需自研 App：

- **BTC**：PSBT（BIP-174）经 `crypto-psbt`（BC-UR）二维码传输 —— 兼容 Sparrow / BlueWallet / Bitcoin Core。
- **ETH**：EIP-1559 交易经 [ERC-4527](https://eips.ethereum.org/EIPS/eip-4527) 的
  `eth-sign-request` / `eth-signature` 二维码传输 —— 兼容 MetaMask（连接 Keystone 空气隙）/ OneKey。

---

## 安全模型

- **不自实现密码学**：secp256k1/BIP32/ECDSA 用 rust-bitcoin，ETH 用维护活跃的 alloy
  （规避旧库对 EIP-1559/2930/7702 交易可锻性校验缺失，CVE-2025-53359）。
- **签名前强制人工核对**：签名机在自己的屏幕上打印收款地址/金额/手续费（ETH 另含
  chainId/nonce/gas 与 ERC-20 解码），必须输入 `yes` 才签 —— 这是空气隙防止被入侵手机
  偷换收款地址的核心属性。**未知 calldata 不解码即警示，拒绝盲签。**
- **私钥落盘加密**：助记词经 Argon2id 派生密钥 + XChaCha20-Poly1305 认证加密成 keystore；
  口令隐藏输入。BIP-39 passphrase（第 25 词）与 keystore 口令相互独立。
- **可复现构建 + 上机前验签**：`Cargo.lock` 入库、`rust-toolchain.toml` 固定版本、`--locked`
  构建；把二进制校验和带到离线机核对无误后，**先断网再导入助记词**。
- **输入不可信**：对来自二维码/U盘的数据防御性解析。

---

## 目录结构

```
btc-wallate/
├─ crates/core/           纯逻辑，无 IO，可单元测试（28 tests）
│  ├─ seed.rs             BIP-39 助记词 / 主私钥（Debug 屏蔽密钥）
│  ├─ derive.rs           BTC BIP-84 / ETH BIP-44 派生、账户 xpub、主指纹
│  ├─ btc.rs              PSBT 摘要 + 签名
│  ├─ eth.rs              EIP-1559 摘要 + 签名 + ERC-4527 端到端胶水
│  ├─ keystore.rs         Argon2id + XChaCha20-Poly1305 助记词加密
│  └─ airgap/             BC-UR 动画二维码 + CBOR（crypto-psbt / eth-sign-request）
└─ crates/app/            CLI 二进制 btc-wallate（+ 可测 lib，8 tests + 2 集成）
   ├─ ops.rs              无交互操作层（keystore/地址/导出/解析/摘要/签名）
   ├─ qr.rs              终端二维码显示（半块字符）
   ├─ file_channel.rs    U盘/SD 文件通道（.ur / .psbt）
   └─ main.rs            clap CLI + 交互提示
```

---

## 构建

**开发/测试（macOS 或任意平台）：**
```bash
cargo test            # 全仓测试
cargo build --release # 本机构建（体验/联调）
```

**交叉编译到 x86 Linux（部署到断网设备）：**
```bash
./scripts/build-x86-linux.sh     # 优先用 cross（需 docker），产物在 target/x86_64-unknown-linux-gnu/release/
```
或直接在一台 x86_64 Linux 上原生构建：`cargo build --release --locked -p btc-wallate`。
摄像头扫码为可选特性：加 `--features camera`（在目标机联调时开启，见路线图）。

---

## 使用

全局参数 `--network`（`bitcoin`/`testnet`/`signet`/`regtest`，默认 `bitcoin`；仅影响 BTC，ETH 不受影响）。

**交互式 TUI（聚焦签名流程）**：不带子命令直接运行即进入 TUI，引导「解锁 → 选择输入(文件/摄像头) →
屏幕核对 → 确认签名 → 输出(文件/动画二维码)」：
```bash
btc-wallate --network signet          # 进入 TUI
btc-wallate --network signet --help   # 其余操作仍走下面的 CLI 子命令
```
（TUI 需在真实终端运行；`new/restore/address/export` 仍用下述 CLI 子命令，便于脚本化。）

```bash
# 1) 初始化（离线）：生成助记词并加密落盘。屏幕会显示助记词——请离线抄写/金属备份。
btc-wallate new --keystore wallet.ks --words 24
#   或从已有助记词恢复：
btc-wallate restore --keystore wallet.ks

# 2) 导出观察钱包凭据 → 手机建只读钱包
btc-wallate export --keystore wallet.ks --coin btc --qr   # BTC 输出描述符（+二维码）
btc-wallate export --keystore wallet.ks --coin eth --qr   # ETH 地址（+二维码）

# 3) 查看接收地址（收款）
btc-wallate address --keystore wallet.ks --coin btc --index 0 --qr
btc-wallate address --keystore wallet.ks --coin eth

# 4) 签名（花费）
#    文件通道：手机把未签名交易存到 U盘（.psbt 或 .ur），本机签名后写回 signed.ur
btc-wallate sign --keystore wallet.ks --in tx.psbt --out signed.ur
#    二维码通道：省略 --out，结果以动画二维码显示，手机扫回广播
btc-wallate sign --keystore wallet.ks --in request.ur
#    调用电脑camra, 用二维码输入，并用二维码输出
btc-wallate sign --keystore wallet.ks --scan
```

签名前会打印交易摘要，务必逐项核对收款地址/金额/手续费，确认后输入 `yes`。

---

## 交互流程

**A. 收款前置（一次性）：导出观察钱包**
```
[x86 签名机]  export  ──►  描述符/地址（二维码或文件）  ──►  [手机观察钱包] 导入，建只读钱包
```

**B. 花费：离线签名闭环**
```
 手机(热,联网)                 空气隙                  x86 签名机(冷,断网)
 ┌───────────┐                                      ┌───────────────┐
 │ 选 UTXO / 构造未签名交易 │                         │               │
 │ (BTC:PSBT / ETH:1559)   │                         │               │
 │        │                │  二维码/U盘 (未签名)     │               │
 │        └───────────────────────────────────────► │ 读入+解析      │
 │                         │                         │ 屏幕核对 ★      │
 │                         │                         │  to/金额/手续费 │
 │                         │                         │  (输入 yes)     │
 │                         │  二维码/U盘 (已签名)     │ 用私钥签名      │
 │ 收回 ◄─────────────────────────────────────────  │ 输出结果        │
 │ 广播到网络              │                         │ (私钥不出机)    │
 └───────────┘                                      └───────────────┘
   ★ 屏幕核对是防止被入侵手机偷换收款地址的关键一步
```

数据格式（跨隙）：BTC = `crypto-psbt`；ETH 未签名 = `eth-sign-request`，签名结果 = `eth-signature`。
大数据自动用 fountain-code 动画二维码分帧，接收侧收齐重组。

---

## 与手机现成钱包互操作

| 链 | 观察/广播端（手机） | 传输 |
|---|---|---|
| BTC | Sparrow / BlueWallet / Bitcoin Core（导入输出描述符） | crypto-psbt 二维码 / .psbt 文件 |
| ETH | MetaMask（连接硬件钱包 → Keystone 空气隙）/ OneKey | eth-sign-request / eth-signature 二维码 |

> 严格遵循标准即可互操作，但具体 App 版本请先用**测试网小额**实测确认。

---

## 状态与路线图

- ✅ 派生（对照 BIP-84 / Hardhat 官方向量）、BTC PSBT 签名、ETH EIP-1559 签名
- ✅ 空气隙 BC-UR / ERC-4527 编解码（字节级线格式断言保障互操作）
- ✅ 加密 keystore、CLI（new/restore/address/export/sign）、文件通道、终端二维码
- ✅ 摄像头扫码（`camera` 特性，nokhwa + rqrr，`sign --scan`）——手机→签名机方向的二维码输入
- ✅ 交互式 TUI（ratatui，无子命令即进入）——聚焦签名流程：解锁/核对/确认/扫码进度/二维码回显
- ✅ ETH 账户配对导出（`crypto-multi-accounts`，`export --coin eth`）——供 MetaMask「连接硬件钱包→QR」配对
- ⏳ 真机端到端演练：signet（BTC，已验证）/ Sepolia（ETH，步骤见 docs/TESTNET.md，待首次对 MetaMask 联调）

BTC 已可完整离线签名（文件或摄像头二维码进、二维码/文件出），与手机现成钱包互操作。
测试网联调详见 [docs/TESTNET.md](docs/TESTNET.md)。
