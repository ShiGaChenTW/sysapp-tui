# sysapp-tui

**macOS 全系統套件掃描器 & TUI 儀表板**

`sysapp-tui` 是一款命令列工具，能一次掃描 macOS 系統中來自八個不同來源的所有已安裝套件、應用程式與工具鏈，並以互動式終端介面（TUI）呈現。

---

## 功能特色

- **8 種資料來源**：Homebrew 公式、Homebrew Cask、`/Applications`、Cargo、Go、npm、pip、gem、pkgutil
- **智慧去重**：同名套件自動合併，保留資訊最豐富的來源（優先級：Homebrew > Cask > Applications > Cargo > Go > npm/pip/gem > pkgutil）
- **程式語言識別**：自動標記每項套件的程式語言（Rust、Go、Python、JavaScript、Ruby、C、Swift 等）
- **使用頻率分析**：解析 `.zsh_history` 取得 CLI 工具使用次數，透過 `mdls` 查詢 GUI 應用程式最後使用時間
- **互動式 TUI**：Ratatui 驅動的純終端介面，支援排序、搜尋、詳情檢視
- **完全離線**：不發送任何網路請求，所有資料皆來自本機系統命令

---

## 安裝

### 透過 Homebrew

尚未發布（追蹤於 SCO-236）。目前請由原始碼建置。

### 從原始碼編譯

```bash
# 複製倉庫
git clone https://github.com/ShiGaChenTW/sysapp-tui.git
cd sysapp-tui

# 編譯（需 Rust 工具鏈）
cargo build --release

# 執行檔位於 target/release/sysapp-tui
./target/release/sysapp-tui
```

**系統需求**：macOS 12+、Rust 1.80+（edition 2024）

---

## 使用方式

### 基本執行

```bash
sysapp-tui
```

啟動後會依序執行：
1. **掃描階段**：並行查詢所有套件管理工具，收集原始資料
2. **豐富化階段**：識別程式語言、分析使用頻率與最後使用時間
3. **TUI 啟動**：開啟互動式儀表板

### TUI 按鍵操作

| 按鍵 | 功能 |
|------|------|
| `j` / `k` / `↑` / `↓` | 上下移動選取 |
| `d` / `u` / `PgDn` / `PgUp` | 翻頁 |
| `g` / `G` / `Home` / `End` | 跳到第一筆／最後一筆 |
| `1` – `7` | 依欄位排序（名稱／來源／語言／版本／安裝日期／使用次數／路徑），再按一次反向 |
| `/` | 進入搜尋模式 |
| `Esc` | 取消搜尋／關閉浮層 |
| `Enter` / `i` | 檢視選取項目的詳細資訊 |
| `?` | 開關按鍵說明浮層 |
| `q` / `Ctrl-C` | 離開 |

使用次數與安裝日期兩欄第一次選取時預設由大到小排序——數量與日期真正有意義的
那一端是大的那頭。`▲`／`▼` 指示永遠反映實際排序方向。


### 搜尋模式

按下 `/` 進入搜尋模式後，直接輸入關鍵字即可即時過濾。比對對象包含套件名稱、來源、偵測到的語言與安裝路徑（皆不分大小寫）。`Enter` 保留過濾條件並回到瀏覽，`Esc` 清除過濾並還原完整清單。

### 詳細資訊

選取任一項目後按 `i`，可檢視該套件的完整資訊，包括：
- 名稱與版本
- 來源（brew / cask / cargo / npm 等）
- 識別的程式語言
- 安裝日期
- 最後使用時間與使用次數
- 完整路徑
- 描述資訊

---

## 資料來源說明

| 來源 | 優先級 | 掃描方式 | 提供欄位 |
|------|--------|----------|----------|
| Homebrew | 7（最高） | `brew info --json=v2 --installed` | 名稱、版本、描述 |
| Homebrew Cask | 6 | `brew info --json=v2 --installed` | 名稱、版本、描述 |
| Applications | 5 | `system_profiler SPApplicationsDataType` | 名稱、版本、路徑、修改日期 |
| Cargo | 4 | 讀取 `~/.cargo/bin/` 目錄 | 名稱、路徑、安裝日期 |
| Go | 3 | 讀取 `~/go/bin/` 目錄 | 名稱、路徑、安裝日期 |
| npm | 2 | `npm list -g --json` | 名稱、版本 |
| pip | 2 | `pip3 list --format=json` | 名稱、版本 |
| gem | 2 | `gem list --local` | 名稱、版本 |
| pkgutil | 1（最低） | `pkgutil --pkgs` | 名稱（從 reverse-DNS 擷取） |

### 去重邏輯

當不同來源掃到同名套件時：

- **高優先級來源**取代低優先級來源，但會保留低優先級來源中高優先級缺少的欄位（如路徑、描述、安裝日期）
- **低優先級來源**僅填補高優先級來源的空缺欄位，不覆蓋既有資料

### 語言識別

- 第一階段：依據來源直接推斷（cargo → Rust、pip → Python、npm → JavaScript、go → Go、gem → Ruby）
- 第二階段：對 Homebrew 二進位檔執行 `file` 命令分析
- 第三階段：對 `.app` 套件檢查內含的 framework 類型

### 使用資料

- CLI 工具：解析 `~/.zsh_history`，統計各指令出現次數
- GUI 應用程式：透過 `mdls` 查詢 Spotlight 中記錄的最後使用時間與使用次數

---

## 開發

### 專案結構

```
sysapp-tui/
├── src/
│   ├── main.rs          # 進入點：掃描 → 補強 → TUI
│   ├── model.rs         # 資料模型（AppEntry, Source, Language）
│   ├── scanner/
│   │   ├── mod.rs       # 掃描排程與去重
│   │   ├── applications.rs
│   │   ├── brew.rs
│   │   ├── cargo_scan.rs
│   │   ├── gem.rs
│   │   ├── go.rs
│   │   ├── npm.rs
│   │   ├── pip.rs
│   │   └── pkgutil.rs
│   ├── enricher/
│   │   ├── mod.rs       # 補強流程統籌
│   │   ├── language.rs  # 語言偵測
│   │   └── usage.rs     # 使用頻率分析
│   └── tui/
│       ├── mod.rs       # App：實作 tears::Application
│       ├── message.rs   # Message / Mode / Column — TEA 詞彙
│       ├── keymap.rs    # (Mode, Key) → Message 對應
│       ├── theme.rs     # 語意色彩槽 + 三階降級
│       └── components/
│           ├── header.rs      # 識別牌、計數器、來源密度
│           ├── table.rs       # 資料網格（游標與排序狀態）
│           ├── search.rs      # 即時過濾輸入
│           ├── detail.rs      # 單筆記錄浮層
│           ├── help.rs        # `?` 按鍵說明浮層
│           └── statusbar.rs   # 模式、位置、必要鍵位
├── Cargo.toml
├── README.md
└── README-ZH.md
```

### 架構

TUI 採 The Elm Architecture，執行時為 [`tears`](https://crates.io/crates/tears)：

- **`Message`** — 應用程式所有可能的狀態轉移
- **`update`** — `(state, Message)` 的純函數，不碰終端機、不繪製
- **`view`** — 狀態的純函數，不改狀態

因此輸入處理與狀態轉移可在沒有 tty 的情況下測試。每個元件自帶狀態並知道
如何把自己畫進一個 rect；元件不反向存取應用程式。

### 測試

```bash
cargo test                          # 36 個測試
cargo test render -- --nocapture    # 印出所有畫格
```

Render 測試透過 ratatui 的 `TestBackend` 實際畫出畫格，
讓版面回歸在沒有終端機的情況下也能被抓到。

### 建置

```bash
cargo build            # 除錯模式
cargo build --release  # 正式釋出
cargo check            # 快速型別檢查（不產出二進位檔）
```

---

## 授權

MIT
