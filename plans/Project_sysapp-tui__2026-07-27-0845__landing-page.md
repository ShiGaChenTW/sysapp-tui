# sysapp-tui 產品介紹頁（GitHub Pages）

**建立時間：** 2026-07-27 08:45
**最後更新：** 2026-07-27 09:05
**狀態：** 已完成

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

- [x] Step 1 — 抓真實 TUI 畫面（browse / idle / cold-start / detail）
- [x] Step 2 — 把 ANSI 畫面轉成保留配色的 HTML
- [x] Step 3 — 寫 `docs/index.html`（brutalist 版面 + 真實素材）
- [x] Step 4 — 深淺色主題、手機寬度、無橫向捲動驗證
- [x] Step 5 — 啟用 GitHub Pages 並實測線上可存取
- [x] Step 6 — README 加上介紹頁連結
- [x] Step 7 — commit + push + Linear 轉 Done

## 決策紀錄

- 08:45 — 選 GitHub Pages 而非 Vercel。零額外基礎設施、repo 已公開、
  網址掛在 repo 底下對顧問定位是加分。
- 08:45 — 用 `docs/` 目錄而非 `gh-pages` branch。單一 branch 好維護，
  且介紹頁與程式碼同進同退。

## 阻塞 / 待決議

無

## 結束摘要

**上線位址**：https://shigachentw.github.io/sysapp-tui/

**做法重點**

頁面上的終端機畫面是**真的**——`capture.py` 用 pty 驅動已發布的 binary，
把 ANSI 輸出轉成保留原色的 HTML。沒有 mockup、沒有圖片檔，整頁零外部請求。
配色直接取自 `src/tui/theme.rs`，所以介紹頁與產品是同一個視覺系統；
終端綠只出現在擷取的畫面裡（使用次數計量條），與 TUI 自己的單一元件規則一致。

頁面上每個數字都是實測來的：10ms、906 筆、402 筆雜訊、brew 的 38 秒。

**刻意寫進去的東西**

`/Applications` 掃不到的限制寫在頁面上，沒有藏。只列優點的頁面不是憑證。
另外把 tears vs hojicha 的版本衝突排查、以及四個實機才抓到的 bug 寫成
Engineering notes——那些才是顧問能力的證據，不是功能列表。

**驗證（真 Chrome，非 CDP）**

- 848px 與 1440px 皆無 body 橫向溢出，寬畫面在自己的容器內捲動
- 頁面自身 console 零錯誤（唯一警告來自無關的 Chrome extension）
- CSS-only 分頁切換，無 JavaScript 也能運作
- 線上內容與 commit 的檔案 byte-identical

**驗證過程中修掉的兩個 bug**

1. `.hero` 與 `section` 用了 `padding` 簡寫，把 `.wrap` 設的水平 padding 歸零
   ——hero 整個貼齊視窗邊緣。改用 `padding-block` 讓兩個軸不再打架。
2. metric 區塊原本是 full-bleed，其他區塊置中，讀起來像沒對齊而非刻意斷開。
   已改為共用同一條 1180 網格線。
