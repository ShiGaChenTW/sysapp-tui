# sysapp-tui V0.2 — 快取秒開、背景重掃、發布

**建立時間：** 2026-07-27 03:02
**最後更新：** 2026-07-27 03:58
**狀態：** 阻塞

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

- [x] Step 1 — SCO-232 快取層：`model` 加 serde derive、`cache.rs`、`--refresh` 旗標
- [x] Step 2 — SCO-232 標頭顯示資料新鮮度（`SNAPSHOT 2h ago`）
- [x] Step 3 — SCO-232 實測啟動時間並確認 < 200ms
- [x] Step 4 — SCO-234 首次執行的掃描進度畫面（TUI 內，非 stderr）
- [x] Step 5 — SCO-233 背景重掃 + `r` 鍵，不阻塞任何按鍵
- [x] Step 6 — SCO-230 pkgutil 雜訊過濾開關
- [x] Step 7 — SCO-235 閒置檢視（僅依使用頻率與時間，不含 size）
- [ ] Step 8 — SCO-236 Homebrew tap 釋出 v0.2.0（**阻塞：repo 需轉 public**）
- [x] Step 9 — 全數測試 + clippy + 實機 pty 驗證
- [x] Step 10 — commit + push + Linear 卡片轉 Done

## 決策紀錄

- 03:02 — V0.1 已 commit（`ec3786d`）並推上 `ShiGaChenTW/sysapp-tui`。
  repo 早在 2026-05-14 就存在且 `main` 正好停在我們的父 commit `498321b`，
  故為乾淨 fast-forward，非新建。
- 03:02 — repo 目前是 **PRIVATE**。SCO-236 的 Homebrew 釋出需要公開，
  但「把程式碼公開」是不可逆的對外動作，等 Scott 確認再改。

## 阻塞 / 待決議

- **SCO-236 卡在 repo 可見性**：`ShiGaChenTW/sysapp-tui` 目前是 private，
  Homebrew tap 無法從私有 repo 安裝。轉公開是不可逆的對外動作，等 Scott 決定。
  解除後只剩三步：repo 轉 public → 建 homebrew-tap repo → 打 tag 取 sha256。

## 結束摘要

**達成的核心目標**

| | 之前 | 之後 |
|---|---|---|
| 暖啟動（有快取） | 88.9s | **6–12ms** |
| 冷啟動首次繪製 | 89 秒空白終端機 | **0.01s**（進度畫面） |
| 冷啟動到可用 | 88.9s | 6.4s（brew 暖快取時） |

**完成的卡**（5/6）

- SCO-232 快取層 — `~/Library/Caches/sysapp-tui/`，schema 版本化，
  temp-file + rename 原子寫入，損毀／版本不符／缺檔皆退回掃描不 panic
- SCO-233 背景重掃 `r` — 單飛（重複按 r 會被忽略），掃描期間鍵位全活，
  游標與過濾條件跨重掃保留，失敗保留舊資料
- SCO-230 雜訊過濾 `p` — 906 筆中 402 筆是 pkgutil 收據與 `/System/` 元件（44%），預設隱藏
- SCO-235 閒置檢視 `s` — 零呼叫且近期未開啟，與搜尋、雜訊過濾可疊加
- SCO-234 冷啟動進度畫面 — 每個來源獨立回報，brew 標示為最慢來源
- SCO-236 發布 — 程式碼側完成（workflow + formula + 文件），卡在 repo 可見性

**過程中修掉的自造回歸**

把掃描移到 TUI 之後執行時，scanner 與 enricher 的 `eprintln!` 開始畫到
alternate screen 上（實測畫面出現 `detecting380 UNITS` 蓋掉 BREW 那列）。
已全數移除，改由 `Message` 回報。

**順帶發現的既有 bug（已開卡，非本次造成）**

`/Applications` 底下 146 個使用者安裝的 App 完全掃不到。根因是
`system_profiler` 在沒有完全磁碟取得權限時只回報 `/System/` 底下的元件，
不是 scanner 的問題。這是整個工具最嚴重的資料完整性缺口，
也直接削弱閒置檢視的價值——已列 V0.3 urgent。

**測試**

36 → 54 個測試，`src/` 自有程式碼 clippy 零警告。
所有效能數字皆為 release build 於真 pty 實測，非估算。
