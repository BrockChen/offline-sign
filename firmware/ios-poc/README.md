# iOS PoC：Rust 密钥核心跑在 UIKit App 里（iOS 12 兼容）

验证 `esp-signer-core`（纯 Rust）能编成 iOS 静态库、链进 UIKit App、在设备上运行并派生正确地址。
面向 iPhone 6 / iOS 12.5.8（A8/arm64，有 Secure Enclave）。**iOS 12 用 UIKit（SwiftUI 需 13+）、不能用 async（需 15+）。**

## 这是什么
完整的 UIKit 签名机 App（iOS 12 兼容）：
**导入助记词（加密存 Keychain）→ 解锁+概览(BTC/ETH 地址) → 扫码/粘贴未签名 → 屏幕核对 →
Touch ID → 签名 → 结果二维码**。密码学全在 Rust（`esp-signer-core` 经 `escore.h` 的 C-ABI），
私钥/种子只在 Rust 内瞬时存在。无摄像头时用「粘贴 UR」页（预填示例 eth-sign-request）测试。

## 已验证
- 模拟器：App 编译并运行，显示真实「导入钱包」界面（`xcodebuild BUILD SUCCEEDED` + 启动截图）。
- 逻辑：Rust FFI 的 host 测试端到端跑通 `import → wallet_info → summarize → sign`（`cargo test -p esp-signer-core ffi`）。
- 早期探针：`escore_probe` 返回 `bc1qcr8...306fyu`（BIP-84 官方向量一致）。

## 用 Xcode 打开并编译（推荐）
需要：Xcode、`xcodegen`（`brew install xcodegen`）、Rust + iOS target。
```bash
cd firmware/ios-poc
./build.sh                 # 编 Rust 静态库 → 打 xcframework → 生成 SignerPoC.xcodeproj
open SignerPoC.xcodeproj   # 用 Xcode 打开
```
- **模拟器**：顶部选任意 iPhone 模拟器 → Cmd+R。
- **真机（iPhone 6）**：选中 target → Signing & Capabilities → Team 选你的**免费 Apple ID**（Personal Team）
  → 顶部选中你的 iPhone → Cmd+R。首次在 iPhone「设置→通用→VPN与设备管理」信任开发者证书。
  免费证书**7 天过期，需每周重连 Mac 重装**。

> 改了 Rust 代码后，重跑 `./build.sh` 刷新 xcframework 再在 Xcode 里 Cmd+R。

## 纯命令行验证（可选，无需 Xcode 工程）
```bash
IPHONEOS_DEPLOYMENT_TARGET=12.2 cargo build --release --target aarch64-apple-ios-sim -p esp-signer-core
SDK=$(xcrun --sdk iphonesimulator --show-sdk-path)
xcrun -sdk iphonesimulator swiftc -target arm64-apple-ios12.2-simulator -sdk "$SDK" \
  -import-objc-header escore.h app.swift \
  -L ../../target/aarch64-apple-ios-sim/release -lesp_signer_core \
  -framework Security -framework CoreFoundation -o SignerPoC
# 再手工组装 .app + simctl install/launch
```

## 上真机 iPhone 6（免费 Apple ID）
- 真机切片：`IPHONEOS_DEPLOYMENT_TARGET=12.0 cargo build --release --target aarch64-apple-ios -p esp-signer-core`，
  打包 `.a` 成 `.xcframework`。
- 需在 Xcode 里建一个 **UIKit** App 工程（部署目标 12.2），链 xcframework，用 **Personal Team（免费 Apple ID）**
  签名安装——证书 **7 天有效，需每周重装**；一次最多 3 个自签 App。
- 完整签名机流程：AVFoundation 扫未签名二维码 → 屏幕核对 → 签名 → CoreImage 显签名二维码；
  keystore 用 Keychain + Secure Enclave（生物解锁）保护；使用时开**飞行模式**。

## 界面
深色·硬件钱包风（`app.swift` 内 `Theme` 令牌 + 组件层；iPhone 6/iOS 12.5 无系统深浅色，故定死一套深色主题）：
近黑藏蓝底 + 比特币橙强调、卡片化输入/概览、网络胶囊徽标（主网绿/测试网琥珀）、核对页琥珀警示条、
结果二维码白底卡片。App 图标/启动屏 logo 为程序化生成的「盾牌 + ₿」（`gen-icons.swift`）。

## App Store 上架清单
> 定位：**非托管本地离线签名器**（不联网、不托管资金、不做币币兑换/法币出入金、不提供投资建议）。

### 已在本仓库内完成（代码/配置）
- [x] App 图标全尺寸 + 1024 营销图（**无 alpha**）：`gen-icons.swift` → `Assets.xcassets/AppIcon.appiconset`
- [x] 启动屏：`LaunchScreen.storyboard`（深色 + logo）
- [x] 真实 bundle id 占位 `com.ecohash.btcwallate`（上架前换成你注册的正式 App ID）
- [x] **导出合规**：`ITSAppUsesNonExemptEncryption=NO`（仅标准密码学做本地保护/签名，适用豁免）
- [x] 权限文案**本地化**：`Resources/{en,zh-Hans}.lproj/InfoPlist.strings`（相机/Face ID，含「离线不上传」）
- [x] 测试脚手架 `#if DEBUG` 隔离：助记词/口令预填、「粘贴 UR」入口、`DEMO` 截图入口 —— **Release 不含**
- [x] **零联网**：无 `URLSession`、无网络权限 → 隐私问卷可全选「不收集数据」

### 需你在 Apple 后台 / App Store Connect 侧完成（代码代替不了）
- [ ] **付费开发者账号**：免费 Apple ID **不能上架**（仅真机 7 天自签）。加入 Apple Developer Program（$99/年）
- [ ] **组织主体注册（强烈建议）**：加密钱包类依 **Guideline 3.1.5(b)** 常要求 Organization 身份
      （需 D-U-N-S 邓白氏编码），个人账号易被拒
- [ ] 注册正式 App ID 并替换 `PRODUCT_BUNDLE_IDENTIFIER`
- [ ] App 隐私（"nutrition label"）：全选「不收集数据」
- [ ] 隐私政策 URL：可写「本 App 不收集、不传输任何用户数据」
- [ ] 商店截图（各机型）、描述、关键词、分类（工具 / 财务）
- [ ] **审核备注**：Release 无内置演示钱包，须提供一组**测试网测试助记词** + 步骤
      （导入 → 解锁 → 内置示例 UR 核对 → Touch ID → 签名 → 二维码），并说明
      「离线签名机，配合观察钱包使用，评审可用内置示例 UR 完成全流程」

### 过审要点解读
- **2.1 完整性**：审核员需能跑通。本 App 无内置资金/演示钱包，故必须在审核备注给测试助记词与示例 UR。
- **3.1.5(b) 加密货币**：非托管、不兑换、不挖矿，风险较低；**主体身份**（组织 vs 个人）是主要门槛。
- **5.1.1 数据收集**：零收集是过审优势，如实声明即可；**切勿引入任何分析 / 崩溃统计 SDK**（会破坏「不收集」声明）。
- **导出合规**：Argon2 / ChaCha20-Poly1305 / secp256k1 用于本地保护与签名，属豁免类；已设 `ITSAppUsesNonExemptEncryption=NO`。
