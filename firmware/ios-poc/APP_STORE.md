# btc-wallate · App Store 上架材料

> ⚠️ **账号前提**：加密钱包类 App 按 Guideline **3.1.5(b)(i)** 要求以**组织(Organization)** 身份提交。
> **个人(Individual)账号提交本 App 大概率被拒**。若坚持个人账号，仅建议走「Ad Hoc / 内部 TestFlight」自用分发（不进商店）。
> 下列材料适用于组织账号正式上架，或个人账号 TestFlight 提审时的备注。

---
## 一、App 描述（App Store Connect → App 信息）

### 中文（简体）
```
btc-wallate 是一款非托管、全程离线的比特币 / 以太坊交易签名器。

• 离线签名：私钥永不触网。App 不联网、不收集、不上传任何数据。
• 空气隙传输：通过二维码在本机与联网「观察钱包」之间传递未签名交易与签名结果。
• 本机加密：助记词经 Argon2 + ChaCha20-Poly1305 加密，存于 Keychain / Secure Enclave，
  以 Face ID / Touch ID 解锁授权。
• 逐项核对：签名前在屏幕上核对收款地址、金额、手续费、链 ID，防被入侵设备偷换。
• 支持 BTC（原生隔离见证 bc1）与 ETH（含 EIP-1559）。

btc-wallate 是非托管工具：不保管你的资金，不做币币兑换、不做法币出入金、不提供投资建议。
请离线备份你的助记词。
```

### English
```
btc-wallate is a non-custodial, fully offline transaction signer for Bitcoin & Ethereum.

• Offline signing: private keys never touch the network. The app makes no network
  requests and collects/uploads nothing.
• Air-gapped transfer: unsigned transactions and signatures move between this device and
  an online "watch-only" wallet via QR codes.
• On-device encryption: your mnemonic is encrypted with Argon2 + ChaCha20-Poly1305 and
  stored in the Keychain / Secure Enclave, unlocked with Face ID / Touch ID.
• Verify every field: check recipient, amount, fee and chain ID on screen before signing.
• Supports BTC (native SegWit bc1) and ETH (incl. EIP-1559).

btc-wallate is a non-custodial tool: it never holds your funds, offers no exchange,
no fiat on/off-ramp, and no investment advice. Keep an offline backup of your mnemonic.
```

- **关键词(≤100 字符)**：`离线签名,冷钱包,非托管,比特币,以太坊,PSBT,air-gap,签名,BTC,ETH`
- **分类**：主 = Finance（财务）或 Utilities（工具）；本 App 无价格、无内购。
- **年龄分级**：4+（无不良内容）。

---
## 二、隐私政策（需公开 URL，填入 App 隐私政策字段）

### 中文
```
btc-wallate 隐私政策

btc-wallate 是一款离线运行的应用。我们不收集、不存储、不传输任何个人数据或使用数据。

• 无网络：App 不发起任何网络请求，无服务器，无后端。
• 本地存储：你的助记词以加密形式仅保存在你设备的 Keychain 中，我们无法访问。
• 无第三方：不集成任何分析、广告或崩溃统计 SDK。
• 相机：仅用于在本机扫描二维码，图像不被保存或上传。

如有疑问，请联系：<你的邮箱>
```

### English
```
btc-wallate Privacy Policy

btc-wallate runs offline. We do not collect, store, or transmit any personal or usage data.

• No network: the app makes no network requests; there is no server or backend.
• Local storage: your mnemonic is stored encrypted in your device Keychain only; we cannot access it.
• No third parties: no analytics, ads, or crash-reporting SDKs are integrated.
• Camera: used solely to scan QR codes on-device; images are never saved or uploaded.

Contact: <your-email>
```

---
## 三、App 隐私「营养标签」问卷（App Store Connect → App Privacy）
- 选择 **Data Not Collected（不收集数据）** —— 全部类别都不勾。
- 依据：App 无网络、无 SDK、无账号体系；所有数据仅本地加密存储。

---
## 四、App 审核备注（App Review Information → Notes）
> 关键：Release 构建无内置演示钱包、无预填示例，审核员必须按下述步骤 + 提供的测试数据才能跑通完整签名流程（否则触发 2.1 完整性）。

### English（审核员多为英文，建议以英文为主）
```
This app is a NON-CUSTODIAL, FULLY OFFLINE transaction signer for Bitcoin & Ethereum.
It makes no network calls, has no accounts and no in-app purchases, and collects no data.
It pairs with a separate online watch-only wallet (not required for review).

HOW TO REVIEW — no real funds or second device needed:

1. On first launch you land on "Import Wallet". Paste this standard BIP-39 TEST mnemonic
   (public test vector, holds no funds):
       test test test test test test test test test test test junk
   Enter any password, e.g.  pw   → tap "Import & encrypt".

2. You are taken to the home screen showing the wallet's BTC and ETH addresses.

3. Tap "Enter transaction". Paste this sample Ethereum (Sepolia testnet) signing request:

ur:eth-sign-request/osadtpdahddkeyjpjsehkneokninieinehktenknjziniyjnhsjeksksksimksecenhsknjyiyjeechsetetaohdeyaowtlspkenoslalscmjkhelrnyvsgwztlfgmaymwjtjtryctcssrvasesavowsfeuolswpemdkpkmedlltcnlnwzjlseaeaelartaxaaaacyaepkenosahtaaddyoeadlecsdwykcsfnykaeykaewkaewkaocydmttmkoxamghswoeeefgkgjphpihuolbhklglpfmjevotevyzmnbatioinjnghjljeihjtyaksckrh

4. Tap "Review" — a human-readable summary is shown (chainId, to, value, gas...).

5. Tap "Confirm with Touch ID & sign". (On Simulator, biometrics can be enrolled via
   Features → Face ID/Touch ID → Enrolled, or it proceeds if none is set.)
   A QR code of the signed result is displayed — this QR is scanned back by the
   online watch-only wallet to broadcast. Signing happens entirely offline.
```

### 中文（可附）
```
本 App 为非托管、全程离线的 BTC/ETH 交易签名器：不联网、无账号、无内购、不收集数据，
配合联网观察钱包使用（审核无需该钱包）。

评审步骤（无需真实资金或第二台设备）：
1. 首屏「导入钱包」，粘贴标准 BIP-39 测试助记词（公开测试向量，无资金）：
   test test test test test test test test test test test junk
   口令任意（如 pw）→「导入并加密保存」。
2. 进入首页，显示该钱包 BTC / ETH 地址。
3. 点「手动输入交易」，粘贴上方英文备注中的示例 UR（以太坊 Sepolia 测试网）。
4. 点「核对」查看交易摘要（chainId / 收款地址 / 金额 / gas）。
5. 点「Touch ID 确认并签名」，显示签名结果二维码（由观察钱包扫码广播）。全程离线。
```

---
## 五、提交前自检
- [ ] 用 **Release / Distribution** 归档：确认无测试预填、无 DEMO 入口（均 `#if DEBUG` 隔离）。
- [ ] 「手动输入交易」入口在 Release 中**可见**（供审核员测试）。
- [ ] `ITSAppUsesNonExemptEncryption=NO` 已在 Info.plist（见 [project.yml](project.yml)）。
- [ ] 隐私政策 URL 可访问；隐私问卷选「不收集」。
- [ ] 审核备注已粘贴上文测试助记词 + 示例 UR + 步骤。
- [ ] 应用内「设置 → 关于与免责声明」展示非托管/离线/不兑换/风险自负声明。
