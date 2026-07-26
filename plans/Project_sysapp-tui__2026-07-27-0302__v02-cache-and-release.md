# sysapp-tui V0.2 — 快取秒開、背景重掃、發布

**建立時間：** 2026-07-27 03:02
**最後更新：** 2026-07-27 03:02
**狀態：** 進行中

## 目標

把啟動時間從 **88.9 秒**降到 **200 毫秒以內**，並讓這個工具能被別人裝起來。
沒人會用一個要等 89 秒才開得起來的工具——這是 V0.2 唯一真正要解決的事。

## 實測基準（release build，906 筆，本機）

| 階段 | 耗時 |
|---|---|
| Scan | 81.6s |
| Enrich | 7.3s |
| **總計到 TUI 開啟** | **88.9s** |

底層指令：`brew info --json=v2 --installed` **38.1s** ·
`system_profiler SPApplicationsDataType` 8.6s · `gem list` 3.4s ·
`cargo install --list` 1.9s · `pkgutil --pkgs` 0.7s

架構本身沒問題（`tokio::join!` + `tokio::process::Command` 是真並行，
brew 只呼叫一次就同時取得 formula 與 cask）。純粹是 `brew info` 本身慢。
**優化掃描指令救不了這 89 秒，只有快取可以。**

## Plan Steps

- [ ] Step 1 — SCO-232 快取層：`model` 加 serde derive、`cache.rs`、`--refresh` 旗標
- [ ] Step 2 — SCO-232 標頭顯示資料新鮮度（`SNAPSHOT 2h ago`）
- [ ] Step 3 — SCO-232 實測啟動時間並確認 < 200ms
- [ ] Step 4 — SCO-234 首次執行的掃描進度畫面（TUI 內，非 stderr）
- [ ] Step 5 — SCO-233 背景重掃 + `r` 鍵，不阻塞任何按鍵
- [ ] Step 6 — SCO-230 pkgutil 雜訊過濾開關
- [ ] Step 7 — SCO-235 閒置檢視（僅依使用頻率與時間，不含 size）
- [ ] Step 8 — SCO-236 Homebrew tap 釋出 v0.2.0
- [ ] Step 9 — 全數測試 + clippy + 實機 pty 驗證
- [ ] Step 10 — commit + push + Linear 卡片轉 Done

## 決策紀錄

- 03:02 — V0.1 已 commit（`ec3786d`）並推上 `ShiGaChenTW/sysapp-tui`。
  repo 早在 2026-05-14 就存在且 `main` 正好停在我們的父 commit `498321b`，
  故為乾淨 fast-forward，非新建。
- 03:02 — repo 目前是 **PRIVATE**。SCO-236 的 Homebrew 釋出需要公開，
  但「把程式碼公開」是不可逆的對外動作，等 Scott 確認再改。

## 阻塞 / 待決議

- **SCO-236 卡在 repo 可見性**：需要 Scott 決定是否將 repo 轉為 public。

## 結束摘要

（工作結束時補上）
