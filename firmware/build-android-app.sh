#!/usr/bin/env bash
# 构建 Android app：cargo-ndk 编 native .so → gradlew 打 APK → 装真机并启动。
# 依赖： rustup target add aarch64-linux-android; brew install --cask android-ndk android-commandlinetools;
#        brew install gradle; cargo install cargo-ndk; adb 连上设备。
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
export ANDROID_NDK_HOME="$(brew --prefix)/share/android-ndk"
export ANDROID_HOME="$(brew --prefix)/share/android-commandlinetools"

# 1) 交叉编译 JNI native 库到 arm64（设备主 ABI）
( cd "$HERE/.." && cargo ndk -t arm64-v8a \
    -o firmware/android-app/app/src/main/jniLibs build --release -p btcwallate-jni )
rm -f "$HERE/android-app/app/src/main/jniLibs/arm64-v8a/libesp_signer_core.so"  # 多余(已静态链进 jni.so)

# 2) 打 APK（首次会用 wrapper 下载 Gradle 8.7 + AGP）
cd "$HERE/android-app"
[ -f local.properties ] || echo "sdk.dir=$ANDROID_HOME" > local.properties
./gradlew assembleDebug

# 3) 装真机并启动
adb install -r app/build/outputs/apk/debug/app-debug.apk
adb shell am start -n com.ecohash.btcwallate/.MainActivity
echo "✅ 已安装并启动。阶段 A：native 自检；阶段 B 做对标 iOS 的完整 UI。"
