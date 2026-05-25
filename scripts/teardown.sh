#!/usr/bin/env bash
#
# Universal worktree teardown script
# ─────────────────────────────────────────
# 工具中立的「worktree 銷毀前清理」邏輯。
# 清掉大型 build 產物，避免磁碟膨脹。
#
set -e

WT="$(pwd)"
echo "▶ Teardown worktree at $WT"

for d in node_modules dist build .next .turbo .cache target __pycache__ .pytest_cache; do
  if [ -d "$WT/$d" ]; then
    rm -rf "$WT/$d"
    echo "  ✓ removed $d"
  fi
done

# 個人 override
if [ -f "$WT/scripts/teardown.local.sh" ]; then
  echo "  → running scripts/teardown.local.sh"
  bash "$WT/scripts/teardown.local.sh"
fi

echo "✅ Teardown complete"
