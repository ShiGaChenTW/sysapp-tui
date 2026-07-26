# sysapp-tui TUI 重新設計 — tears TEA 架構 + 元件化 + 工業粗獷視覺

**建立時間：** 2026-07-27 01:25
**最後更新：** 2026-07-27 02:05
**狀態：** 已完成

## 目標

把 `src/tui/mod.rs`（481 行、單檔、命令式 loop）重構成 The Elm Architecture：
以 `tears` 0.10.2 提供 Model/Message/update/view 執行時，把畫面拆成獨立可組合的
元件，視覺套用 industrial-brutalist-ui（Tactical Telemetry 暗色）與 tui-design
的語意色彩槽、鍵位分層、資料密度規範。scanner / enricher / model 不動。

## 關鍵限制（已用 cargo 實證，非推測）

- `tears` 0.10.2 → ratatui **^0.30**
- `hojicha-pearls` 0.2.1 → ratatui **^0.29**（唯一有元件的版本，2025-08 後未更新）
- 兩者同時加入會拉進 ratatui 0.29 + 0.30 兩份不相容 crate → 型別無法互通
- `hojicha` 0.2.2 已**移除 ratatui**，`Model::view(&self) -> String`（純字串渲染）

→ 「維持 ratatui」與「用 Hojicha」互斥。採 tears，Hojicha 只借元件化設計思路。

## Plan Steps

- [x] Step 1 — Cargo.toml：加 tears 0.10.2，ratatui 0.29→0.30，crossterm 0.28→0.29
- [x] Step 2 — `tui/theme.rs`：語意色彩槽 + Tactical Telemetry 調色盤 + NO_COLOR 降級
- [x] Step 3 — `tui/message.rs`：Message 列舉（TEA 訊息集）
- [x] Step 4 — `tui/keymap.rs`：鍵位分層 L0/L1/L2 → Message 對應
- [x] Step 5 — `tui/components/`：header / table / search / detail / help / statusbar
- [x] Step 6 — `tui/mod.rs`：App 實作 `tears::Application`，組合各元件
- [x] Step 7 — `main.rs`：改為 async 呼叫 tui::run
- [x] Step 8 — cargo build + clippy 通過（tui/ 零警告）＋ 36 個測試
- [x] Step 9 — 實際跑起來驗證（真 pty、906 筆真實資料、80x24 與 120x30）
- [x] Step 10 — README.md / README-ZH.md 按鍵表同步（原本已與程式碼不符）

## 決策紀錄

- 01:10 — 選 tears 作為 TEA 執行時，排除 hojicha。原因：hojicha 0.2.2 無 ratatui、
  0.2.1 綁 ratatui 0.29 與 tears 的 0.30 衝突；tears 2026-07-26 仍在更新。
- 01:15 — bklit-ui / benchmark / canary 三個 skill 為 Web 專用（React 圖表、
  Lighthouse、瀏覽器 canary），對 Rust TUI 無直接適用面；只借其原則
  （token 化主題、不硬寫顏色、效能基線、冒煙驗證）。
- 01:20 — `Application::view(&self)` 為不可變借用，ratatui `TableState` 需 `&mut`
  → `DataGrid` 內用 `RefCell<TableState>` 承載捲動狀態。
- 01:48 — 排序改為「回到頂端」而非「跟隨原選取項目」。實機驗證發現按 `6`
  排使用次數後畫面停在清單中段全是 1x 的區域，違背使用者按下排序的意圖。
- 01:52 — `compare()` 全欄位改回自然升序，改由 `Column::default_ascending()`
  決定首次選取方向（Usage / Installed 預設降序）。原本把 Usage 比較器寫反，
  造成畫面顯示 `USAGE ▲` 卻是最少使用在前——指示箭頭在說謊。
- 02:00 — `truncate()` 由 `chars().count()` 改為 unicode-width 的顯示寬度。
  實機資料含「系統設定」「終端機」等 CJK 名稱，字數與欄寬不等價。

## 阻塞 / 待決議

無

## 結束摘要

**做了什麼**

- `src/tui/` 由單檔 481 行 → 11 檔 2019 行（含測試與文件註解），全部走 TEA：
  `message.rs`（訊息集）、`keymap.rs`（鍵位→訊息）、`theme.rs`（語意色彩）、
  `components/`（header / table / search / detail / help / statusbar 六個獨立元件）。
- `update` 是 `(state, Message)` 的純函數，不碰終端機；`view` 是狀態的純函數，
  不改狀態。輸入處理與狀態轉移因此可在無 tty 下測試。
- 新增 36 個測試：邏輯測試 + 用 ratatui `TestBackend` 實際畫出畫格並斷言的
  render 測試（`cargo test render -- --nocapture` 可直接印出各畫面）。
- 視覺：Tactical Telemetry 暗色、hazard red 單一強調色、終端綠只用在使用次數
  一處、全大寫、ASCII 框架、來源密度長條、`®` 與 `REV / UNIT` 工業標記。
- 三階色彩降級（truecolor / 16 ANSI / mono），`NO_COLOR` 優先於一切。
- 新增功能：`?` 按鍵說明浮層、`g`/`G`/`d`/`u` 導覽、第 7 欄排序、搜尋擴及
  來源／語言／路徑、視窗過小提示。

**實機驗證發現並修掉的 4 個真問題**（純看程式碼不會發現）

1. `USAGE ▲` 顯示最少使用在前——排序指示箭頭與實際方向相反。
2. 按下排序後畫面停在清單中段，看不到剛排出來的頭部。
3. 來源密度條 `PKGUTIL███░░░` 標籤與長條相黏（欄寬未涵蓋最長來源名）。
4. 詳細浮層 `INVOCATIONS96` 標籤與數值相黏（padding 未超過最長標籤）。

**Linear 同步（2026-07-27 02:30）**

專案 `SysApp-TUI`（`21ddf83c-ce9e-4011-b685-3dc66c3392e7`，team SCO）已建立 15 張卡：
- **SCO-217 … SCO-229**（13 張，Done）= V0.1 全部範圍
- **SCO-230 / SCO-231**（2 張，Todo）= V0.2 待辦（pkgutil 過濾、render 效能基線）

⚠️ **milestone 尚未指派**：`orca linear` 完全沒有 milestone 指令，
SCO-217…229 需在 Linear UI 手動拖進 `V0.1`。

**未完成 / 後續建議**

- `bklit-ui`、`benchmark`、`canary` 三個 skill 屬 Web 專用，本次未實質套用，
  僅借用其原則。若要真正落地，對應的 TUI 版本應是：render 效能基線
  （criterion + TestBackend 畫格計時）與啟動冒煙測試。
- pkgutil 來源產生大量無版本、無語言的雜訊條目（906 筆中 115 筆），
  建議之後加一個「隱藏 pkgutil」的過濾開關。
- 目前搜尋為子字串比對，未做模糊比對；清單以已知工具名為主，暫時足夠。
