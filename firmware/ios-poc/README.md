# iOS PoC：Rust 密钥核心跑在 UIKit App 里（iOS 12 兼容）

验证 `esp-signer-core`（纯 Rust）能编成 iOS 静态库、链进 UIKit App、在设备上运行并派生正确地址。
面向 iPhone 6 / iOS 12.5.8（A8/arm64，有 Secure Enclave）。**iOS 12 用 UIKit（SwiftUI 需 13+）、不能用 async（需 15+）。**

## 已验证（模拟器）
App 显示 `BIP-84[0]: bc1qcr8te4kr609gcawutmrza0j4xv80jy8z306fyu`（官方向量一致）→ Rust↔Swift↔iOS 打通。

## 构建 + 跑模拟器
```bash
# 1) 编 Rust 静态库（模拟器切片；Apple Silicon = arm64 sim）
IPHONEOS_DEPLOYMENT_TARGET=12.2 cargo build --release --target aarch64-apple-ios-sim -p esp-signer-core
# 2) 编 App（UIKit, ios12.2）并链接静态库
SDK=$(xcrun --sdk iphonesimulator --show-sdk-path)
xcrun -sdk iphonesimulator swiftc -target arm64-apple-ios12.2-simulator -sdk "$SDK" \
  -import-objc-header escore.h app.swift \
  -L ../../target/aarch64-apple-ios-sim/release -lesp_signer_core \
  -framework Security -framework CoreFoundation -o SignerPoC
# 3) 组装 .app（见本目录脚本/Info.plist 模板）并 simctl install/launch
```

## 上真机 iPhone 6（免费 Apple ID）
- 真机切片：`IPHONEOS_DEPLOYMENT_TARGET=12.0 cargo build --release --target aarch64-apple-ios -p esp-signer-core`，
  打包 `.a` 成 `.xcframework`。
- 需在 Xcode 里建一个 **UIKit** App 工程（部署目标 12.2），链 xcframework，用 **Personal Team（免费 Apple ID）**
  签名安装——证书 **7 天有效，需每周重装**；一次最多 3 个自签 App。
- 完整签名机流程：AVFoundation 扫未签名二维码 → 屏幕核对 → 签名 → CoreImage 显签名二维码；
  keystore 用 Keychain + Secure Enclave（生物解锁）保护；使用时开**飞行模式**。
