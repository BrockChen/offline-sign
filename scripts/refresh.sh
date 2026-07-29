#!/usr/bin/env bash
# refresh.sh — 每 6 天重签重装，免去手动。Apple ID 只存在 Xcode → Preferences → Accounts。
set -euo pipefail
UDID="97071b724c9029322d40aaadd157dfc58e61b511"          # xcrun xctrace list devices 查
TEAM="PQ5V7F9M2L"                        # Team ID（免费个人账号 Personal Team，非邮箱）

pushd /Users/aa01035/work/mcode/btc-wallate/firmware/ios-poc

# 1) 用 Xcode 自动签名重新构建（-allowProvisioningUpdates 会用已登录的 Apple ID 自动申请免费证书/profile）
xcodebuild -project SignerPoC.xcodeproj -scheme SignerPoC -sdk iphoneos -configuration Release \
  -allowProvisioningUpdates -destination "id=$UDID" \
  DEVELOPMENT_TEAM="$TEAM" build

# 2) 打包 IPA 并安装（iOS 12.5 用 ideviceinstaller；devicectl 只支持较新 iOS）
#    需先： brew install ideviceinstaller
APP=$(ls -d ~/Library/Developer/Xcode/DerivedData/SignerPoC-*/Build/Products/Release-iphoneos/SignerPoC.app | head -1)
WORK=$(mktemp -d)
mkdir -p "$WORK/Payload" && cp -R "$APP" "$WORK/Payload/"
( cd "$WORK" && zip -qr app.ipa Payload )
ideviceinstaller -u "$UDID" install "$WORK/app.ipa"   # 旧版 ideviceinstaller 用： -i "$WORK/app.ipa"
rm -rf "$WORK"
popd
echo "✅ 已重签并安装（证书 7 天有效，建议每 6 天由 launchd/cron 触发本脚本）"
