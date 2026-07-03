#!/usr/bin/env bash
# 交叉编译离线签名机到 x86_64 Linux（部署到断网设备）。
# 在 macOS 上开发、验证后运行本脚本产出 x86 Linux 二进制。
set -euo pipefail

TARGET=x86_64-unknown-linux-gnu
PKG=btc-wallate

cd "$(dirname "$0")/.."

if command -v cross >/dev/null 2>&1; then
  # 推荐：cross 在容器内自带 linux 工具链（本机已装 docker）。
  echo ">> 使用 cross 交叉编译到 $TARGET"
  cross build --release --locked --target "$TARGET" -p "$PKG"
  echo ">> 产物: target/$TARGET/release/$PKG"
elif command -v cargo-zigbuild >/dev/null 2>&1; then
  echo ">> 使用 cargo-zigbuild 交叉编译到 $TARGET"
  rustup target add "$TARGET" >/dev/null 2>&1 || true
  cargo zigbuild --release --locked --target "$TARGET" -p "$PKG"
  echo ">> 产物: target/$TARGET/release/$PKG"
else
  cat <<'EOF'
未找到 cross 或 cargo-zigbuild。三选一：

  1) cross（需 docker，本机已装）—— 推荐：
       cargo install cross
       ./scripts/build-x86-linux.sh

  2) cargo-zigbuild（需安装 zig）：
       brew install zig && cargo install cargo-zigbuild
       ./scripts/build-x86-linux.sh

  3) 直接在一台 x86_64 Linux 上原生构建（最简单、最易复现）：
       cargo build --release --locked -p btc-wallate
       # 摄像头联调再加： --features camera

构建完成后请核对 target/.../release/btc-wallate 的 sha256，
在断网前将其与本机记录的校验和比对，确认无误后再导入助记词。
EOF
  exit 1
fi
