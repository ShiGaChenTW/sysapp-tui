# sysapp-tui

**macOS System Package Scanner & TUI Dashboard**

`sysapp-tui` is a command-line tool that scans all installed packages, applications, and toolchains on macOS from eight different sources in a single pass, presenting the results in an interactive Terminal User Interface (TUI).

---

## Features

- **8 data sources**: Homebrew formulae, Homebrew Cask, `/Applications`, Cargo, Go, npm, pip, gem, pkgutil
- **Smart deduplication**: Automatically merges same-name packages, keeping the richest info (priority: Homebrew > Cask > Applications > Cargo > Go > npm/pip/gem > pkgutil)
- **Language detection**: Automatically identifies each package's programming language (Rust, Go, Python, JavaScript, Ruby, C, Swift, etc.)
- **Usage frequency analysis**: Parses `.zsh_history` for CLI tool usage counts; queries `mdls` for GUI app last-used timestamps
- **Interactive TUI**: Ratatui-driven terminal interface with sorting, search, and detail views
- **Completely offline**: No network requests — all data comes from local system commands

---

## Installation

### Via Homebrew

Not published yet — tracked as SCO-236. Build from source for now.

### Build from source

```bash
# Clone the repository
git clone https://github.com/ShiGaChenTW/sysapp-tui.git
cd sysapp-tui

# Build (requires Rust toolchain)
cargo build --release

# Binary at target/release/sysapp-tui
./target/release/sysapp-tui
```

**Requirements**: macOS 12+, Rust 1.80+ (edition 2024)

---

## Usage

### Basic execution

```bash
sysapp-tui
```

After launch it will:
1. **Scan phase**: Query all package managers in parallel to collect raw data
2. **Enrich phase**: Detect programming languages, analyze usage frequency and last-used time
3. **TUI phase**: Open the interactive dashboard

### TUI keybindings

| Key | Function |
|-----|----------|
| `j` / `k` / `↑` / `↓` | Move selection down/up |
| `d` / `u` / `PgDn` / `PgUp` | Move one page |
| `g` / `G` / `Home` / `End` | Jump to first/last entry |
| `1` – `7` | Sort by column (name/source/language/version/install date/usage/path); press again to reverse |
| `/` | Enter search mode |
| `Esc` | Cancel search / close overlay |
| `Enter` / `i` | View detailed info for selected item (in search mode, `Enter` keeps the filter) |
| `?` | Toggle the key reference overlay |
| `q` / `Ctrl-C` | Quit |

Usage and install-date columns start sorted descending — the interesting end of
a count or a date is the large one. The `▲`/`▼` indicator always reflects the
actual direction.

### Search mode

Press `/` to enter search mode, then type a keyword to filter in real time. Matching is case-insensitive and covers the package name, source, detected language and install path. `Enter` keeps the filter and returns to browsing; `Esc` clears it and restores the full inventory.

### Detail view

Select any item and press `i` to see full information:
- Name & version
- Source (brew / cask / cargo / npm, etc.)
- Detected programming language
- Install date
- Last used time & usage count
- Full path
- Description

---

## Data sources

| Source | Priority | Scan method | Fields provided |
|--------|----------|-------------|-----------------|
| Homebrew | 7 (highest) | `brew info --json=v2 --installed` | name, version, description |
| Homebrew Cask | 6 | `brew info --json=v2 --installed` | name, version, description |
| Applications | 5 | `system_profiler SPApplicationsDataType` | name, version, path, mod date |
| Cargo | 4 | Read `~/.cargo/bin/` | name, path, install date |
| Go | 3 | Read `~/go/bin/` | name, path, install date |
| npm | 2 | `npm list -g --json` | name, version |
| pip | 2 | `pip3 list --format=json` | name, version |
| gem | 2 | `gem list --local` | name, version |
| pkgutil | 1 (lowest) | `pkgutil --pkgs` | name (extracted from reverse-DNS) |

### Deduplication logic

When the same package name is found across multiple sources:

- **Higher-priority sources** replace lower-priority ones, but retain fields missing from the higher source (e.g., path, description, install date)
- **Lower-priority sources** only fill gaps in higher-priority data, never overwrite

### Language detection

- Phase 1: Infer from source type (cargo → Rust, pip → Python, npm → JavaScript, go → Go, gem → Ruby)
- Phase 2: Run `file` command on Homebrew binaries for analysis
- Phase 3: Inspect `.app` bundles for embedded framework types

### Usage data

- CLI tools: Parse `~/.zsh_history` to count command occurrences
- GUI apps: Query Spotlight via `mdls` for last-used timestamps and usage counts

---

## Development

### Project structure

```
sysapp-tui/
├── src/
│   ├── main.rs          # Entry point: scan → enrich → TUI
│   ├── model.rs         # Data model (AppEntry, Source, Language)
│   ├── scanner/
│   │   ├── mod.rs       # Scan scheduling & dedup
│   │   ├── applications.rs
│   │   ├── brew.rs
│   │   ├── cargo_scan.rs
│   │   ├── gem.rs
│   │   ├── go.rs
│   │   ├── npm.rs
│   │   ├── pip.rs
│   │   └── pkgutil.rs
│   ├── enricher/
│   │   ├── mod.rs       # Enrichment orchestration
│   │   ├── language.rs  # Language detection
│   │   └── usage.rs     # Usage frequency analysis
│   └── tui/
│       ├── mod.rs       # App: implements tears::Application
│       ├── message.rs   # Message / Mode / Column — the TEA vocabulary
│       ├── keymap.rs    # (Mode, Key) → Message
│       ├── theme.rs     # Semantic color slots + three-tier degradation
│       └── components/
│           ├── header.rs      # Identity plate, counters, source density
│           ├── table.rs       # Data grid (cursor + sort state)
│           ├── search.rs      # Live filter input
│           ├── detail.rs      # Single-record overlay
│           ├── help.rs        # `?` key reference overlay
│           └── statusbar.rs   # Mode, position, essential keys
├── Cargo.toml
├── README.md
└── README-ZH.md
```

### Architecture

The TUI follows The Elm Architecture via the [`tears`](https://crates.io/crates/tears) runtime:

- **`Message`** — every state transition the app can undergo
- **`update`** — a pure function of `(state, Message)`; touches no terminal, draws nothing
- **`view`** — a pure function of state; mutates nothing

Input handling and state transitions are therefore testable without a tty. Each
component owns its own state and knows how to draw itself into a rect;
components never reach back into the application.

### Tests

```bash
cargo test                          # 36 tests
cargo test render -- --nocapture    # print every rendered frame
```

Render tests draw real frames through ratatui's `TestBackend`, so layout
regressions surface without a terminal.

### Build

```bash
cargo build            # Debug mode
cargo build --release  # Release build
cargo check            # Fast type-check (no binary output)
```

---

## License

MIT
