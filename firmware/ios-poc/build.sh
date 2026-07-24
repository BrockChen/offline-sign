#!/usr/bin/env bash
# 一键准备可用 Xcode 打开的工程：编 Rust 静态库 → 打 xcframework → 生成 .xcodeproj。
# 用法： cd firmware/ios-poc && ./build.sh   然后  open SignerPoC.xcodeproj
set -euo pipefail
cd "$(dirname "$0")"
ROOT=../..

echo ">> [0/4] 生成 App 图标 + 启动屏 logo"
swift gen-icons.swift

echo ">> [1/4] 编译 Rust 静态库（真机 arm64 + 模拟器 arm64）"
( cd "$ROOT" && rustup target add aarch64-apple-ios aarch64-apple-ios-sim >/dev/null 2>&1 || true )
( cd "$ROOT" && IPHONEOS_DEPLOYMENT_TARGET=12.0 cargo build --release --target aarch64-apple-ios     -p esp-signer-core )
( cd "$ROOT" && IPHONEOS_DEPLOYMENT_TARGET=12.2 cargo build --release --target aarch64-apple-ios-sim -p esp-signer-core )

echo ">> [2/4] 打包 xcframework"
rm -rf EspSignerCore.xcframework hdr && mkdir -p hdr && cp escore.h hdr/
xcodebuild -create-xcframework \
  -library "$ROOT/target/aarch64-apple-ios/release/libesp_signer_core.a"     -headers hdr \
  -library "$ROOT/target/aarch64-apple-ios-sim/release/libesp_signer_core.a" -headers hdr \
  -output EspSignerCore.xcframework >/dev/null
rm -rf hdr

echo ">> [3/4] 生成 Xcode 工程"
xcodegen generate

echo ""
echo "完成。用 Xcode 打开：  open $(pwd)/SignerPoC.xcodeproj"
echo "  模拟器：选任意 iPhone 模拟器 → Cmd+R"
echo "  真机：Signing & Capabilities 选你的免费 Apple ID 团队 → 选中 iPhone → Cmd+R（证书 7 天，需每周重装）"
