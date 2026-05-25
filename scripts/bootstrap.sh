#!/usr/bin/env bash
#
# Universal worktree bootstrap script
# ─────────────────────────────────────────
# 工具中立的「新 worktree 初始化」邏輯。
# 不綁定 Superset、Conductor、Paseo 或任何特定工具。
#
# 使用情境：
#   - 手動：在新 git worktree 跑 `bash scripts/bootstrap.sh`
#   - Superset：.superset/setup.sh 是 1 行 wrapper 呼叫這支
#   - 任何 hook 系統都能呼叫
#
set -e

WT="$(pwd)"
# 偵測 PROJECT_ROOT：
#   - 若是 git worktree (.git 是檔案)，從 worktree metadata 推
#   - 否則假設 WT 本身就是 PROJECT_ROOT
if [ -f .git ]; then
  GITDIR="$(cat .git | sed 's/^gitdir: //')"
  PROJECT_ROOT="$(cd "$GITDIR/../../.." && pwd)"
else
  PROJECT_ROOT="$WT"
fi

echo "▶ Bootstrap worktree at $WT"
[ "$PROJECT_ROOT" != "$WT" ] && echo "  (project root: $PROJECT_ROOT)"

# ─── 1. 複製 .env 系列（最重要：避免 worktree 缺 secrets） ───
if [ "$PROJECT_ROOT" != "$WT" ]; then
  for envfile in .env .env.local .env.development .env.production; do
    if [ -f "$PROJECT_ROOT/$envfile" ] && [ ! -f "$WT/$envfile" ]; then
      cp "$PROJECT_ROOT/$envfile" "$WT/$envfile"
      echo "  ✓ copied $envfile"
    fi
  done

  # ─── 2. 複製其他常見本機設定檔 ───
  for f in .secrets .npmrc .yarnrc.yml .python-version .nvmrc; do
    if [ -f "$PROJECT_ROOT/$f" ] && [ ! -f "$WT/$f" ]; then
      cp "$PROJECT_ROOT/$f" "$WT/$f"
      echo "  ✓ copied $f"
    fi
  done
fi

# ─── 3. 安裝依賴（依專案 stack 自動偵測） ───

# Rust (cargo) — 預設不預先 build（耗時），需要時手動 cargo build
if [ -f Cargo.toml ]; then
  echo "  → cargo fetch (skipping build)"
  cargo fetch --quiet
fi

# ─── 4. 個人 override（不入庫，可自訂額外初始化） ───
if [ -f "$WT/scripts/bootstrap.local.sh" ]; then
  echo "  → running scripts/bootstrap.local.sh"
  bash "$WT/scripts/bootstrap.local.sh"
fi

echo "✅ Bootstrap complete"
