#!/usr/bin/env bash
# 生成 App 图标 + 启动屏 logo（深色·盾牌+₿）到 Assets.xcassets。
# 用法： cd firmware/ios-poc && ./gen-icons.sh
set -euo pipefail
cd "$(dirname "$0")"
swift gen-icons.swift
