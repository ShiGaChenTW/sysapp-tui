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

### 透過 Homebrew（即將推出）

```bash
brew install sysapp-tui
```

### 從原始碼編譯

```bash
# 複製倉庫
git clone https://github.com/yourname/sysapp-tui.git
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
| `↑` / `↓` | 上下移動選取列 |
| `1` - `6` | 切換排序欄位（名稱/來源/語言/版本/安裝日期/使用次數/路徑） |
| `/` | 進入搜尋模式 |
| `Esc` | 取消搜尋 / 返回列表 |
| `Enter` | 確認搜尋（搜尋模式）/ 無動作（一般模式） |
| `i` | 檢視選取項目的詳細資訊 |
| `q` | 離開程式 |

### 搜尋模式

按下 `/` 進入搜尋模式後，直接輸入關鍵字即可即時過濾。比對對象為套件名稱（不分大小寫）。再次按 `/` 或 `Esc` 可取消搜尋。

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
│   ├── main.rs          # 進入點：掃描 → 豐富化 → TUI
│   ├── model.rs         # 資料模型（AppEntry、Source、Language）
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
│   │   ├── mod.rs       # 豐富化流程編排
│   │   ├── language.rs  # 程式語言識別
│   │   └── usage.rs     # 使用頻率分析
│   └── tui/
│       └── mod.rs       # Ratatui 終端介面
├── Cargo.toml
└── README.md
```

### 建置

```bash
cargo build            # 除錯模式
cargo build --release  # 正式釋出
cargo check            # 快速型別檢查（不產出二進位檔）
```

---

## 授權

MIT
