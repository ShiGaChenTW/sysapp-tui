# sysapp-tui V0.3 — 分類、啟動、介面類型、時間軸、來源卡

**建立時間：** 2026-07-27 15:23
**最後更新：** 2026-07-27 20:50
**狀態：** 已完成

## 目標

V0.2 讓清單「看得到」。V0.3 讓它「用得動」：程式能被分類、能被 Enter 啟動、
能看出是 GUI 還是終端工具、能看出裝了多久沒碰過。外加把來源統計從記錄面板
搬到頂端做成紅底卡片。

## 五項需求對應

| # | 需求 | 落點 |
|---|------|------|
| 1 | 程式分類（自動 + 自訂） | `model::Category`、`enricher/category.rs`、`config.rs` |
| 2 | Enter 執行 | `exec.rs`、`keymap.rs`、`main.rs` 外層 loop |
| 3 | 介面類型欄位 | `model::UiKind`、`enricher/interface.rs`、`table.rs` |
| 4 | 安裝日期 / 最後啟動日期 | `scanner/*`、`enricher/usage.rs`、`table.rs` 第 8 欄 |
| 5 | 來源統計卡片 | `components/sourcecards.rs`、`tui/mod.rs` view |

---

## F1 — 程式分類

**資料模型**：`Category` enum，內建 `Development / Media / Productivity /
System / Network / Security / Design / Data / Communication / Gaming /
Uncategorized`，外加 `Custom(String)` 承接使用者自訂名稱。
`AppEntry.category: Option<Category>`。

**自動分類**（`enricher/category.rs`，純函式、可單測）依序：
1. 使用者 override（最高優先，見下）
2. 關鍵字比對 `description` + `name`（brew `desc` 是最好的訊號源）
3. `source` + `path` 推論（`/Applications` 且無關鍵字 → Productivity；
   `cargo`/`go`/`npm` → Development）
4. 落空 → `Uncategorized`

**自訂**：`~/.config/sysapp-tui/categories.json`（`dirs::home_dir()` 組出，
不硬編路徑）。
```json
{ "overrides": { "ghostty": "Terminal" },
  "rules": [ { "contains": "kubectl", "category": "DevOps" } ] }
```
讀檔的每個失敗模式（缺檔、壞 JSON、權限）一律 fallback 到空設定，
與 `cache.rs` 同樣的「設定不該讓程式掛掉」原則。

**互動**：
- `c` — 依序循環分類篩選（無 → 分類 A → … → 無），狀態顯示在 stats line
- `C` — 對選取項目輸入分類名稱（沿用 `SearchBox` 的文字輸入模式，
  新增 `Mode::Category`），Enter 寫回 overrides 檔並即時重算

## F2 — Enter 執行

**兩條路徑**，由 `UiKind` 決定：

- **GUI**：`open <path>`（或 cask 用 `open -a <name>`）。背景啟動，TUI 不中斷，
  footer 顯示「已啟動 X」。
- **CLI / TUI 工具**：需要把終端機還給子行程。做法是
  **suspend-relaunch**：`update` 設 `exec_request`，tears program 結束並把
  request 交還 `main`，`main` 還原終端 → 執行 → 等待 → 「按任意鍵返回」→
  帶著 `ResumeState`（cursor 名稱、filter、排序、語言、分類篩選）重開 TUI。
  這樣不必動 tears 內部。

**安全邊界（不可簡化）**：
- Enter 後 footer 出現 `執行 <name>？[y/N]`，需二次確認才跑
- 一律 `Command::new(program).args([])`，**絕不經 shell**——名稱含 `;`、空白、
  引號都不可能變成注入
- 執行檔位置：優先 `entry.path`；沒有就掃 PATH（F3 已建好的 PATH 索引），
  找不到就 notice「無法定位執行檔」，不猜

**Enter 讓位**：記錄面板改由 `i` / `Tab` 開關（`i` 早已綁定，只是移除 Enter）。

## F3 — 介面類型（UI KIND）

`UiKind`：`Gui / Tui / Cli / Service / Library / Unknown`。
`enricher/interface.rs`，**零 subprocess**，單次建立 PATH 執行檔索引後判斷：

| 判準 | 結果 |
|------|------|
| path 以 `.app` 結尾 / source 為 `Applications`、`HomebrewCask` | `Gui` |
| `description` 或 name 命中 TUI 關鍵字（`TUI`、`terminal UI`、`ncurses`、`curses`、`terminal-based`）或在內建已知清單（htop/btop/lazygit/vim/neovim/tig/ranger/k9s/gitui/…） | `Tui` |
| 名稱在 PATH 索引中 | `Cli` |
| pip/npm/gem 套件但不在 PATH | `Library` |
| 名稱以 `d` 結尾且在 sbin，或 plist 存在 | `Service` |
| 其餘 | `Unknown` |

PATH 索引：對 `$PATH` 每個目錄做一次 `read_dir`，收成 `HashSet<String>`。
一次掃描，之後 O(1) 查詢，同時供 F2 定位執行檔。

## F4 — 安裝日期與最後啟動日期

**現況缺口**：install_date 只有 `/Applications` 有；last_used 只有 GUI 有
（mdls），CLI 工具永遠是空的。

**補法（全部零額外 subprocess）**：
1. **通用 fallback**：`install_date` 空 → 取 `path` 的 mtime
2. **brew**：formula 補 `$(brew --prefix)/Cellar/<name>`、
   cask 補 `Caskroom/<token>` 的目錄 mtime，順便把 `path` 填起來
3. **CLI 的 last_used**：`~/.zsh_history` 的 extended 格式
   `: 1753600000:0;cmd` **本來就帶 epoch，現在被丟掉**。改成解析時間戳，
   取該程式最大值寫進 `last_used`。這一改讓 CLI 工具首次有「最後使用時間」，
   `is_idle()` 的判斷也從此對 CLI 有效。
4. `cargo`/`go`/`npm`/`gem`/`pip` 交給第 1 條（各自的 bin 路徑 mtime）

**顯示**：`Column::LastUsed` 成為獨立第 8 欄（可排序、有數字鍵），
USAGE 欄不再兼差顯示日期，回歸純計量條。記錄面板兩個欄位改顯示
「日期（相對時間）」，如 `2026-03-11（4 個月前）`。

**欄位預算**：116 欄下 grid 內容只有 65 欄，9 個欄位塞不下。
改成**三段響應式**：
- `< 70`：NAME · SRC · UI · INSTALLED · USAGE
- `70–95`：加 CATEGORY · LAST USED
- `> 95`：加 LANG · VERSION

數字鍵永遠綁邏輯欄位（即使該欄目前隱藏），`?` 說明列出全部。

## F5 — 來源統計卡片

從記錄面板底部**移除**（`DetailPanel.sources` 欄位一併刪掉），
改放頂端 masthead 與清單卡片之間。

**卡片**：紅底（`theme.masthead()` 的 band 色，非高彩度 accent——理由同
`theme.rs` 既有註解）＋ `cast_shadow` 一格投影。3 列高（上下留白各一），
內容單行 `BREW 323`，來源名 bold、數字次之。依數量遞減排列。

**塞不下時**：能放幾張放幾張，剩下的收成一張 `+N 其他`。
**高度不足時**（`area.height < 20`）整條不渲染——清單本身永遠優先。

---

## Plan Steps

- [x] Step 0 — 共用契約：`UiKind` / `Category` / `AppEntry` 兩個新欄位、
      13 處建構點補齊、`SCHEMA_VERSION` 升 2。`cargo build` 已綠。
- [x] Step 1 — F5 來源統計卡片（獨立、零相依，先做拿到視覺回饋）
- [x] Step 2 — F3 介面類型：`UiKind` + PATH 索引 + `enricher/interface.rs`
- [x] Step 3 — F4 日期補完：zsh history 時間戳、brew Cellar mtime、通用 mtime fallback
- [x] Step 4 — 表格響應式欄位分段 + `Column::LastUsed` + `Column::Category`
- [x] Step 4b — 實機掃描抓到的四個缺陷（見決策紀錄 18:05），已修並以第二次
      實機掃描驗證：Unknown 318→221、Cli 152→249、假 Productivity 289→9、
      `ripgrep` 正確判為 CLI、記錄面板日期不再截斷
- [x] Step 5 — F1 分類：model + 自動分類器 + `categories.json` 讀寫
- [x] Step 6 — F1 互動：`c` 篩選 / `C` 指派（`Mode::Category`）。
      實機驗證：按 `c` 篩到「分類：開發」，507→345，與雜訊篩選正確疊加
- [x] Step 7 — F2 執行：`exec.rs` + 確認流程 + GUI 背景啟動。
      `Mode::Confirm` 併入 `StatusBar`（右側模式／位置區塊要能像 notice 一樣存活）；
      GUI 走 `Command::perform`，`update` 不因為等一個行程而卡住畫面；
      Enter 已從 Detail 鍵位釋出，`Tab` 與 `i` 同為開關
- [x] Step 8 — F2 suspend-relaunch + `ResumeState`。實機九項檢查連續三次全過
- [x] Step 9 — i18n 補齊雙語字串、`?` 說明覆蓋層更新、footer 鍵位更新
- [x] Step 10 — `cache::SCHEMA_VERSION` 升到 2、測試 182 綠、`clippy --all-targets` 淨空
- [x] Step 11 — 實機驗證（冷啟動 909 筆、窄／寬終端、分類篩選、執行交接）
- [x] Step 12 — README 雙語更新、docs/index.html 以真實 0.3.0 畫面重新擷取

## 決策紀錄

- 15:23 — Enter 改綁「執行」，記錄面板讓給 `i`/`Tab`。理由：Enter 在清單型
  介面的通用語意就是「對這一列動手」，開詳情本來就是次要動作。
- 15:23 — CLI 執行選 suspend-relaunch 而非在 TUI 內開 pty。tears 沒有 suspend
  hook，硬做要動它的 runtime；relaunch 只要保存 `ResumeState` 就好，
  程式碼少一個數量級。代價是畫面會閃一下，可接受。
- 15:23 — 執行一律二次確認且不經 shell。這是信任邊界，不適用「先做再說」。
- 15:23 — 自訂分類用 JSON 不用 TOML。`serde_json` 已在相依裡，加 TOML 只為了
  一個設定檔不划算。
- 15:23 — 介面類型判斷不開 subprocess。單次 PATH 掃描換 O(1) 查詢，
  順便給 F2 當執行檔定位器，一份成本兩處用。
- 15:23 — 欄位改響應式而非硬塞。9 欄在 116 欄終端只會被 ratatui 靜默裁掉，
  那比明確少顯示兩欄更糟。
- 15:41 — 共用的 model 契約由主線先落地，兩條實作軌才能吃到不重疊的檔案集
  （Track A = `enricher/` `config.rs` `scanner/`；Track B = `tui/`）。
  否則兩個 agent 都會改 `model.rs`，必衝突。
- 15:41 — F2（Enter 執行）刻意排在第二階段，不與這兩軌並行：它要動
  `keymap.rs` + `tui/mod.rs` + `main.rs`，與 Track B 檔案重疊。
- 15:58 — **F3 的關鍵字判斷降為輔助，改以名單為主**。Grok 拿本機
  homebrew-core（8,517 筆 desc）＋官方 365 天安裝統計實測：原本規劃的 7 個
  關鍵字對 64 個已知 TUI 只命中 15 個（recall 23%）。漏掉的不是冷門貨——
  `fzf`（年 58.6 萬次安裝）的 desc 是「Command-line fuzzy finder」，
  `glow` 寫的是「Render markdown on the CLI」，字面上還說自己是 CLI。
  結論：策展名單為主，五個零偽陽性字串為輔，其餘誠實回 `—`。
- 15:58 — 原規劃的 TUI 名單有四筆**永遠不會命中**：`mc` `btm` `nvim` `hx`
  都是 alias 或 binary 名，`brew info --json` 只回正規名
  （`midnight-commander` / `bottom` / `neovim` / `helix`）。
- 15:58 — `"text-based interface"` 在全部 8,517 筆 desc 裡**零命中**，刪除。
  裸 `"tui"` 會命中 "in<b>tui</b>tive"，誤標 sd / cli11 / dust / packetry
  四個純 CLI；改為 desc-only 且加詞界。`curses` 要擋掉函式庫自己
  （ncurses / notcurses / cdk / libgnt）。
  完整實測表在 scratchpad `v03-classifier-data.md`。
- 16:34 — 記錄面板改「漸進省略」，而非把卡片列的高度門檻調高。
  120x24 加上卡片列後記錄面板內部只剩約 9 行，但內容需要 16 行。
  卡片是摘要、記錄是內容，該讓步的是排版不是資料：
  先掉空白行 → 再掉 `[ LOCATION ]` 與路徑 → 最後才掉區段標題，
  ACTIVITY 三個欄位（安裝／最後使用／呼叫次數）永遠最後才動——
  這個工具存在的理由就是找出沒在用的東西。

- 16:52 — 刪掉 `PathEnvGuard` / `empty_path_index()`。它為了做出一個空的
  PATH 索引，去改行程全域的 `PATH` 環境變數（`unsafe set_var`）。
  在多執行緒 tokio runtime 下這是 UB，而且 `Drop` guard 救不了：
  abort 時不會執行 drop，別的執行緒也照樣能在空窗期讀到空 PATH。
  正解是 `PathIndex::empty()` 直接建一個空 map——這個構造沒有正確版本，
  只有不該存在。

- 17:10 — 記錄面板的「名稱」永遠不省略。原本 agent 把 Name 排進可捨棄清單，
  但一個顯示著 SOURCE / INSTALLED 數值卻沒有名稱的面板比短面板更糟——
  寬終端下面板會隨游標更新，名稱是唯一把數值綁回某一列的東西。
  捨棄順序定為 Spacer → Location → SectionHead，Name 與 Field 一起活到最後。
- 17:10 — `config.rs` 的測試暫存目錄與 `cache.rs` 撞名（`corrupt`／`empty`／
  `missing` 三個 tag 完全相同，路徑同為 `sysapp-tui-test-{tag}-{pid}`），
  而 `Drop` 會 `remove_dir_all(parent)`。平行測試下互刪對方的檔案，
  症狀是「單獨跑會過、整包跑隨機失敗」。修法是把目錄前綴命名空間化
  （`sysapp-tui-config-tests-`），不是改 tag 名——改 tag 只是把陷阱留給下一個
  複製這段 helper 的模組。

- 18:05 — **實機掃描 909 筆，抓到四個單元測試看不到的缺陷。**
  這類問題是「對真實資料的涵蓋率」，不是「某個分支對不對」，只有跑真的才會現形。
  1. `ripgrep` 判成 Unknown——它的執行檔叫 `rg`，而我們拿套件名去查 PATH。
     `Unknown` 318 筆是第二大宗，多半是這個原因。修法：讀 Cellar 版本目錄下的
     `bin/`，那才是 Homebrew 裝進去的真實執行檔名，一次 `read_dir` 就有。
  2. Uncategorized 307 筆＝Homebrew 204 ＋ Gem 42 ＋ Cask 37 ＋ Pip 24。
     我原本的結構性 fallback 只寫了 Cargo/Go/Npm/Applications/Pkgutil，
     漏掉最大宗的 Homebrew——這是我的規劃疏漏。
  3. Productivity 289 筆裡有 280 筆是 Applications 的無條件 fallback。
     整台機器的 GUI app 全被貼上「生產力」，這個桶子不帶任何資訊。
     刪掉，改回 Uncategorized——與 UiKind 同一個原則：**錯的標籤比誠實的空白更糟**。
  4. 記錄面板日期欄位截斷：`2026-05-19（69` 少了後括號，值超出 36 欄面板。
- 18:05 — `last_used` 只有 82 筆且全是 GUI，**不是 bug**。這台機器的
  `~/.zsh_history` 完全沒有 extended 格式的行（`EXTENDED_HISTORY` 沒開），
  根本沒有時間戳可解析。實作與測試都對，是輸入沒有資料。
  → 這代表 F4「CLI 最後使用時間」在 Scott 目前的環境是無效的，
  要嘛在文件說明需開啟 `EXTENDED_HISTORY`，要嘛另尋資料來源。

- 18:52 — 裁定：Homebrew 的結構性 fallback 條件放寬為
  **`Homebrew|Gem|Pip 且非 Gui` → Development**，而非我原本寫的
  `UiKind 為 Cli|Tui|Library`。理由是 agent 回報的實測：323 個 Homebrew
  formula 裡有 109 個根本不裝任何執行檔，因此判為 `Unknown` 而非 `Library`
  （`Library` 只對不在 PATH 的 Npm/Pip/Gem 推論）。照字面規則的話，
  連我自己舉的例子 `abseil` 都會漏掉。關鍵字比對排在結構性 fallback 之前，
  所以落到這條的都是關鍵字沒命中的，預設 Development 是合理的。

- 21:30 — suspend-relaunch 的關鍵限制：`tears::Runtime::run` 會**吃掉**
  `Application` 且不還回來，所以 `App` 上的欄位在 runtime 結束後讀不到。
  解法是 `Arc<Mutex<Option<Suspend>>>` 經 `Flags` 傳進去、`run` 在 runtime
  回來後才讀。`update` 依然只「記錄」不執行，純度沒破。
- 21:30 — `ratatui::restore()` 改成無條件、且放在 `?` 之前。runtime 正常結束、
  出錯、或要交接給子行程，終端機都必須還回來——這是「使用者被留在 raw mode
  出不去」那個最壞情況的唯一防線。
- 21:30 — 子行程失敗只印訊息不往上拋。介面馬上就要回來，為了一次啟動失敗
  就終止整個 session，等於拿掉使用者的工作階段來換一個他重按一次就好的錯誤。
- 21:30 — 回程重讀快取而非把舊 inventory 帶回去：子行程可能是安裝器，
  磁碟上的快照永遠不會比我們離開時舊。

## 阻塞 / 待決議

- Step 8 的 suspend-relaunch 需確認 `tears` 結束後終端狀態能乾淨還原
  （`install_signal_restore()` 已存在，但那是給訊號用的路徑）。
  若還原不乾淨，退路是：CLI 工具改用 `open -a Terminal <path>` 開新視窗。

## 結束摘要

五項需求全部完成。183 測試綠、`clippy --all-targets` 淨空、版號 0.3.0。
已推 `main`（4 個 commit）、已發 `v0.3.0` release、Homebrew tap 已更新。
實測 `brew upgrade sysapp-tui` 由 0.2.3 升到 0.3.0，安裝後 `--version` 回報正確。

### 這次真正學到的一件事

**測試全綠不等於做對了。** Phase 1 交出來的時候 152 個測試全過，同時：
280 個 app 被貼上假的「生產力」標籤、`ripgrep` 判成 Unknown、
307 筆該有分類的沒有。這些沒有一個是測試抓得到的——它們不是「某個分支寫錯」，
而是「對真實資料的涵蓋率不夠」。跑一次真機器（909 筆）才全部現形。

同一件事發生了三次：
1. Grok 拿 8,517 筆真實 desc 實測，證明我的關鍵字規則 recall 只有 23%
2. 實機掃描抓到四個缺陷，全部是涵蓋率而非正確性問題
3. 產品頁用真實畫面重新擷取，才看到 `sorted byUSAGE` 與 `catDevelopment`
   兩個少空格、以及 CAT 欄幾乎每列都是 `Develo…`

第 3 點特別值得記：那頁的整個賣點就是「畫面是真的、不是 mockup」，
**也正因為是真的，它才把 bug 顯示出來**。如果當初用的是手繪 mockup，
這三個缺陷會直接上線。

### 交付內容

| 需求 | 落點 | 實機驗證 |
|------|------|----------|
| 程式分類 | `enricher/category.rs`、`config.rs`、`c`/`C` | 按 `c` 篩到「開發」507→345 |
| Enter 執行 | `exec.rs`、`Mode::Confirm`、suspend-relaunch | `jq` 交接九項檢查連三次全過 |
| 介面類型 | `enricher/interface.rs`（零 subprocess） | Unknown 318→221、Cli 152→249 |
| 安裝／最後啟動日期 | brew Cellar mtime、zsh 時間戳、通用 mtime | 717/909 有安裝日期 |
| 來源統計卡片 | `components/sourcecards.rs` | `BREW 323 APP 287 …` 紅底＋投影 |

### 未完成 / 後續建議

- **CLI 的「最後使用」在 Scott 的機器上永遠是 `—`**：`~/.zsh_history`
  沒開 `EXTENDED_HISTORY`，1,527 行零個時間戳。程式是對的，輸入沒資料。
  README 雙語都已寫明如何開啟，但**不會回溯既有紀錄**。
- **`/Applications` 使用者自行安裝的 app 仍掃不到**（V0.2 就存在的限制）。
  快取檔證實 287 筆 Applications 全部在 `/System/` 底下。產品頁沒有藏這件事。
- **產品頁從 123 KB 漲到 256 KB**：五張 150 欄的畫面比原本四張 118 欄大。
  仍然零外部請求，但如果在意載入速度，可以考慮把 `browse` 以外的畫面延後載入。
- 分類關鍵字表還可以再調：`htop` 目前落在「開發」，語意上更接近「系統」。
  不是缺陷，是調參。
- **Huly 看板未更新（受阻）**：`https://huly.shigachen.me/` 已登入且 client
  連得上 server（console 印出 `Connected to server: 0.7.426`、`onConnect 1`、
  零錯誤），front / model / server 三邊版本一致，93 個資源全部載入成功——
  但 workbench 始終停在載入動畫，超過三分鐘沒有畫面。
  這是該部署自身的問題，不是權限或網路。待 Huly 能開再補上 V0.3 的收尾。
