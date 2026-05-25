#!/usr/bin/env bash
#
# Superset adapter — 薄包裝，呼叫工具中立的 bootstrap
# 若移除 Superset 改用其他工具，刪這個檔案即可。邏輯保留在 scripts/bootstrap.sh。
#
exec bash "$(dirname "$0")/../scripts/bootstrap.sh"
