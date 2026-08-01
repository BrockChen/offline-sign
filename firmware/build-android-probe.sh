#!/usr/bin/env bash
# 交叉编译 signer-probe 到 Android arm64，push 到真机运行，验证纯 Rust 密钥核心
# （导入/概览/签名/导出，走和 iOS 相同的 escore_* FFI）端到端跑通且结果一致。
#
# 依赖： rustup target add aarch64-linux-android;  brew install --cask android-ndk;  adb 连上设备。
# 已验证设备： SM901 (Android 6.0.1 / arm64-v8a)。期望 PROBE_ALL_OK=true。
set -euo pipefail
cd "$(dirname "$0")/.."   # 到 workspace 根

NDK="$(brew --prefix)/share/android-ndk"
TC="$(ls -d "$NDK"/toolchains/llvm/prebuilt/*/bin | head -1)"
API=23                    # 匹配设备 Android 6 = API 23
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TC/aarch64-linux-android${API}-clang"
export CC_aarch64_linux_android="$TC/aarch64-linux-android${API}-clang"
export AR_aarch64_linux_android="$TC/llvm-ar"

rustup target add aarch64-linux-android >/dev/null 2>&1 || true
cargo build --release --target aarch64-linux-android -p signer-probe

BIN=target/aarch64-linux-android/release/signer-probe
adb push "$BIN" /data/local/tmp/signer-probe
adb shell chmod 755 /data/local/tmp/signer-probe
echo "==== 真机运行 ===="
adb shell /data/local/tmp/signer-probe
