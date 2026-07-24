# iOS PoC：Rust 密钥核心跑在 UIKit App 里（iOS 12 兼容）

验证 `esp-signer-core`（纯 Rust）能编成 iOS 静态库、链进 UIKit App、在设备上运行并派生正确地址。
面向 iPhone 6 / iOS 12.5.8（A8/arm64，有 Secure Enclave）。**iOS 12 用 UIKit（SwiftUI 需 13+）、不能用 async（需 15+）。**

## 已验证（模拟器）
App 显示 `BIP-84[0]: bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu`（官方向量一致）→ Rust↔Swift↔iOS 打通。

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
