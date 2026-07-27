# sysapp-tui

**macOS 全系統套件掃描器 & TUI 儀表板**

**[→ 產品介紹頁](https://shigachentw.github.io/sysapp-tui/)**

`sysapp-tui` 是一款命令列工具，能一次掃描 macOS 系統中來自八個不同來源的所有已安裝套件、應用程式與工具鏈，並以互動式終端介面（TUI）呈現。

---

## 功能特色

- **8 種資料來源**：Homebrew 公式、Homebrew Cask、`/Applications`、Cargo、Go、npm、pip、gem、pkgutil
- **智慧去重**：同名套件自動合併，保留資訊最豐富的來源（優先級：Homebrew > Cask > Applications > Cargo > Go > npm/pip/gem > pkgutil）
- **程式語言識別**：自動標記每項套件的程式語言（Rust、Go、Python、JavaScript、Ruby、C、Swift 等）
- **介面類型判斷**：標記每個項目是 GUI、TUI、CLI、服務或函式庫——這也決定了 `Enter` 是能在背景啟動它，還是必須把終端機交出去
- **程式分類**：依套件描述自動歸入十個分類，可逐項覆寫並寫入 `~/.config/sysapp-tui/categories.json`
- **按 `Enter` 執行**：GUI 應用在背景啟動；終端機程式則由介面讓位、把 tty 交給它，結束後再回到原本的位置——一律先經過確認
- **使用頻率分析**：解析 `.zsh_history` 取得 CLI 工具使用次數，透過 `mdls` 查詢 GUI 應用程式最後使用時間
- **互動式 TUI**：Ratatui 驅動的純終端介面，支援排序、搜尋、分類過濾、詳情檢視
- **響應式表格**：寬終端顯示九個欄位，窄終端減為五個，而不是把表頭裁掉
- **完全離線**：不發送任何網路請求，所有資料皆來自本機系統命令

---

## 安裝

### 透過 Homebrew

```bash
brew install ShiGaChenTW/tap/sysapp-tui
```

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
| `1` – `9` | 依欄位排序（名稱／來源／語言／版本／安裝日期／使用次數／分類／介面／最後使用），再按一次反向 |
| `Enter` | 執行選取的項目，需先回答 `[y/N]` 確認（搜尋模式下則是保留過濾條件） |
| `i` / `Tab` | 檢視選取項目的詳細資訊 |
| `/` | 進入搜尋模式 |
| `Esc` | 取消搜尋／關閉浮層 |
| `c` | 循環切換分類過濾 |
| `C` | 為選取項目指定分類 |
| `p` | 顯示／隱藏封裝雜訊（pkgutil 收據、`/System/` 元件） |
| `s` | 只顯示沒有使用跡象的項目 |
| `r` | 背景重新掃描——介面全程維持可用 |
| `?` | 開關按鍵說明浮層 |
| `q` / `Ctrl-C` | 離開 |

數字鍵綁定的是**邏輯欄位**，所以即使某個欄位因終端機太窄而沒顯示，按下對應數字
仍然會依它排序；`?` 浮層列出全部九個。

使用次數與安裝日期兩欄第一次選取時預設由大到小排序——數量與日期真正有意義的
那一端是大的那頭。`▲`／`▼` 指示永遠反映實際排序方向。


### 快取與重新掃描

第一次執行沒有快取，會先開啟進度畫面並在背後掃描，每個來源完成就即時回報。
完整掃描約需 90 秒，其中絕大部分耗在 `brew info`。結果會寫入快取，
**之後每次啟動約 10 毫秒開啟**。

```bash
sysapp-tui --refresh   # 忽略快取，強制重新掃描
sysapp-tui --help
```

在 TUI 內按 `r` 可背景重新掃描而不需重啟，掃描期間介面完全不卡。
標頭永遠顯示資料的新鮮度（`SNAPSHOT 2H AGO`，重掃後為 `LIVE SCAN`）。

### 雜訊與閒置過濾

`pkgutil` 回報的是 Apple 安裝收據——reverse-DNS 形式的 id，沒有版本、沒有語言、
沒有使用資料；`system_profiler` 則會回報所有 `/System/` 底下的元件。
在一般機器上這是 906 筆中的 402 筆，佔 44%，稀釋了每一次排序與搜尋。
兩者預設隱藏，`p` 可切換，標頭會顯示目前隱藏了幾筆。

`s` 只保留沒有使用跡象的項目：零 shell 呼叫**且**近期沒有被 Spotlight 開啟過。
兩個條件缺一不可，因為兩份資料來源本質不對稱——呼叫次數只涵蓋 CLI 工具，
Spotlight 的最後使用時間則只有 GUI 應用才有。

> **CLI 工具的「最後使用」需要 `EXTENDED_HISTORY`。** zsh 只有在開啟這個選項時
> 才會為每一筆指令寫入時間戳；沒開的話 `~/.zsh_history` 只有純指令行，
> 因此**呼叫次數仍然正確**，但「最後使用」欄位對每個 CLI 工具都會是 `—`。
> 在 `.zshrc` 加上 `setopt EXTENDED_HISTORY` 即可，但只對之後寫入的指令有效，
> 不會回溯既有紀錄。

> **已知限制**：目前 `/Applications` 底下使用者自行安裝的應用不會出現在清單中。
> `system_profiler` 在沒有「完全取得磁碟權限」的情況下只會回報 `/System/` 底下的
> 元件。已列入下一版追蹤。

### 搜尋模式

按下 `/` 進入搜尋模式後，直接輸入關鍵字即可即時過濾。比對對象包含套件名稱、來源、偵測到的語言與安裝路徑（皆不分大小寫）。`Enter` 保留過濾條件並回到瀏覽，`Esc` 清除過濾並還原完整清單。

### 程式分類

每個項目會依描述與來源歸入十個分類之一：開發、媒體、生產力、系統、網路、
安全、設計、資料、通訊、遊戲。比對不到的維持「未分類」並顯示 `—`，
而不是硬給一個看起來合理的標籤——**錯的分類比誠實的空白更糟**，
因為一旦顯示在畫面上，你已經分不出哪個是猜的。

`c` 會在清單中實際存在的分類之間循環過濾，並且是**疊加**在搜尋與雜訊過濾之上，
不是取代它們。`C` 為選取項目指定分類，名稱可自訂、不限於內建那十個，
覆寫會寫入 `~/.config/sysapp-tui/categories.json`，重新掃描後依然保留。
送出空白名稱則是移除覆寫，讓自動分類回來。

該檔案是純 JSON，也可以直接編輯：

```json
{
  "overrides": { "ghostty": "Terminal" },
  "rules": [ { "contains": "kubectl", "category": "DevOps" } ]
}
```

檔案不存在、讀不到、格式壞掉、型別不對——每一種失敗都退回空設定。
壞掉的設定檔不會擋住介面。

### 執行程式

`Enter` 執行選取的項目，一律先在底部顯示 `[y/N]` 確認。這個確認是一個**模式**，
在回答之前所有清單按鍵都不作用——一個原本要給清單的誤觸，不可能啟動任何東西。

接下來的行為取決於介面類型：

- **GUI** 應用交給 `open` 在背景啟動，介面不中斷。
- **CLI 與 TUI** 程式需要終端機，所以介面會結束、把終端機還原、讓程式獨佔 tty，
  等你看完之後再重新開啟——並回到你離開時的游標位置、過濾條件、排序與語言。
  回程會重讀快取，因為你剛才執行的有可能是安裝器。
- **函式庫、服務與未分類**項目不可執行，`Enter` 會直接說明而不是亂猜。

指令一律不經過 shell：程式與參數直接交給 `execve`，所以套件名稱裡的 `;`、
引號或換行都只是一個不透明的參數，不可能變成第二個指令。

### 詳細資訊

選取任一項目後按 `i` 或 `Tab`，可檢視該套件的完整資訊，包括：
- 名稱與版本
- 來源（brew / cask / cargo / npm 等）
- 介面類型（GUI / TUI / CLI / 服務 / 函式庫）
- 分類
- 識別的程式語言
- 安裝日期與最後使用時間，各自附上相對時間
- 使用次數
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
│   ├── main.rs          # 進入點：CLI 旗標、快取查詢、啟動
│   ├── cache.rs         # 磁碟上的盤點快照
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
│           ├── scanning.rs    # 冷啟動進度畫面
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
cargo test                          # 54 個測試
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
