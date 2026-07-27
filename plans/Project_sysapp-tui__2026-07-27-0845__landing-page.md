# sysapp-tui 產品介紹頁（GitHub Pages）

**建立時間：** 2026-07-27 08:45
**最後更新：** 2026-07-27 08:45
**狀態：** 進行中

## 目標

做出一頁能對外展示、證明顧問能力的產品介紹頁，掛在 GitHub Pages。
不是 README 的翻譯版——是作品頁。

## 為什麼現在才做

V0.2 規劃時的漏洞：SCO-236 被寫成「Homebrew tap 釋出」並宣稱服務 G0
（公開第一個顧問作品），但把「可安裝」等同於了「可展示」。使用者問起才發現。

## 設計方向

`industrial-brutalist-ui` 的 Tactical Telemetry。這個 skill 本來就是為 web 寫的
（CSS Grid、`clamp()`、scanlines、SVG 濾鏡、`mix-blend-mode`），TUI 那次才是
勉強套用。配色直接沿用 `src/tui/theme.rs` 的語意色彩槽，
讓介紹頁與產品本身是同一個視覺系統——這本身就是說服力。

## Plan Steps

- [ ] Step 1 — 抓真實 TUI 畫面（browse / idle / cold-start / detail）
- [ ] Step 2 — 把 ANSI 畫面轉成保留配色的 HTML
- [ ] Step 3 — 寫 `docs/index.html`（brutalist 版面 + 真實素材）
- [ ] Step 4 — 深淺色主題、手機寬度、無橫向捲動驗證
- [ ] Step 5 — 啟用 GitHub Pages 並實測線上可存取
- [ ] Step 6 — README 加上介紹頁連結
- [ ] Step 7 — commit + push + Linear 轉 Done

## 決策紀錄

- 08:45 — 選 GitHub Pages 而非 Vercel。零額外基礎設施、repo 已公開、
  網址掛在 repo 底下對顧問定位是加分。
- 08:45 — 用 `docs/` 目錄而非 `gh-pages` branch。單一 branch 好維護，
  且介紹頁與程式碼同進同退。

## 阻塞 / 待決議

無

## 結束摘要

（工作結束時補上）
