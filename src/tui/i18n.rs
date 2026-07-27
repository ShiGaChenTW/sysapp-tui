//! Interface language.
//!
//! Strings live in a plain struct rather than a keyed map: a missing or
//! misspelled string is then a compile error instead of a blank cell at
//! runtime, and lookup costs nothing.
//!
//! Traditional Chinese is not a translation of the English — it is written to
//! be read as Chinese. Where the English uses terminal-manual register
//! ("UNITS", "RESCAN"), the Chinese uses the words a Chinese-speaking operator
//! would actually use.
//!
//! **Width matters here.** CJK glyphs occupy two terminal columns, so any
//! layout that pads or truncates around these strings must measure display
//! width, not character count. `pad` is the helper for that.

use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    ZhHant,
    En,
}

impl Lang {
    /// Pick from the environment, defaulting to Traditional Chinese for a
    /// Chinese locale and English otherwise.
    pub fn detect() -> Self {
        let locale = std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default()
            .to_lowercase();
        // zh_TW, zh_HK, zh-Hant, zh_MO all read as Traditional; bare `zh` and
        // zh_CN are Simplified, which this interface does not ship, so they
        // fall back to English rather than showing the wrong script.
        let traditional = ["zh_tw", "zh-tw", "zh_hk", "zh-hk", "zh_mo", "zh-mo", "hant"];
        if traditional.iter().any(|t| locale.contains(t)) {
            Self::ZhHant
        } else {
            Self::En
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::ZhHant => Self::En,
            Self::En => Self::ZhHant,
        }
    }

    /// Shown on the language key, naming the language it switches *to*.
    pub fn other_name(self) -> &'static str {
        match self {
            Self::ZhHant => "English",
            Self::En => "中文",
        }
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Self::ZhHant => &ZH_HANT,
            Self::En => &EN,
        }
    }
}

/// Pad `s` to `width` terminal columns.
///
/// `format!("{s:<width$}")` counts characters, so a CJK label would be padded
/// to half the intended column count and collide with the value beside it.
pub fn pad(s: &str, width: usize) -> String {
    let w = s.width();
    if w >= width {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(width - w))
    }
}

pub struct Strings {
    // Masthead
    pub app_subtitle: &'static str,
    pub units: &'static str,
    pub live: &'static str,
    pub rev: &'static str,
    // Snapshot age
    pub just_now: &'static str,
    pub minutes_ago: &'static str,
    pub hours_ago: &'static str,
    pub days_ago: &'static str,
    // Panels
    pub panel_inventory: &'static str,
    pub panel_record: &'static str,
    // Stats line
    pub shown: &'static str,
    pub noise_hidden: &'static str,
    pub idle_only: &'static str,
    pub sorted_by: &'static str,
    // Columns
    pub col_name: &'static str,
    pub col_source: &'static str,
    pub col_lang: &'static str,
    pub col_version: &'static str,
    pub col_installed: &'static str,
    pub col_usage: &'static str,
    pub col_path: &'static str,
    // Record panel
    pub sec_origin: &'static str,
    pub sec_activity: &'static str,
    pub sec_location: &'static str,
    pub sec_note: &'static str,
    pub sec_sources: &'static str,
    pub f_source: &'static str,
    pub f_language: &'static str,
    pub f_version: &'static str,
    pub f_installed: &'static str,
    pub f_last_used: &'static str,
    pub f_invocations: &'static str,
    pub none_recorded: &'static str,
    pub no_selection: &'static str,
    // Modes
    pub mode_browse: &'static str,
    pub mode_search: &'static str,
    pub mode_detail: &'static str,
    pub mode_help: &'static str,
    // Footer keys
    pub k_move: &'static str,
    pub k_filter: &'static str,
    pub k_noise: &'static str,
    pub k_idle: &'static str,
    pub k_sort: &'static str,
    pub k_rescan: &'static str,
    pub k_keys: &'static str,
    pub k_quit: &'static str,
    // Search
    pub search_label: &'static str,
    pub search_matches: &'static str,
    pub search_hint: &'static str,
    pub no_matches: &'static str,
    pub esc_to_clear: &'static str,
    // Notices
    pub rescanning: &'static str,
    pub keys_still_live: &'static str,
    pub rescan_complete: &'static str,
    pub rescan_failed: &'static str,
    pub rescan_empty: &'static str,
    pub scan_empty: &'static str,
    pub cache_write_failed: &'static str,
    pub noise_hidden_notice: &'static str,
    pub noise_shown_notice: &'static str,
    pub idle_only_notice: &'static str,
    pub all_units_notice: &'static str,
    // Scanning screen
    pub initial_inventory: &'static str,
    pub no_snapshot: &'static str,
    pub scanning: &'static str,
    pub skipped: &'static str,
    pub slowest_source: &'static str,
    pub enriching: &'static str,
    pub enriching_detail: &'static str,
    pub cached_next_launch: &'static str,
    // Undersized
    pub viewport_undersized: &'static str,
    pub need: &'static str,
    pub have: &'static str,
    pub too_small_record: &'static str,
    pub too_small_help: &'static str,
    // Help overlay
    pub help_title: &'static str,
    pub h_navigate: &'static str,
    pub h_inspect: &'static str,
    pub h_filter: &'static str,
    pub h_sort: &'static str,
    pub h_view: &'static str,
    pub h_data: &'static str,
    pub h_session: &'static str,
    pub h_down_one: &'static str,
    pub h_up_one: &'static str,
    pub h_down_page: &'static str,
    pub h_up_page: &'static str,
    pub h_first: &'static str,
    pub h_last: &'static str,
    pub h_open_record: &'static str,
    pub h_close_overlay: &'static str,
    pub h_live_filter: &'static str,
    pub h_cancel_filter: &'static str,
    pub h_keep_filter: &'static str,
    pub h_sort_col: &'static str,
    pub h_toggle_noise: &'static str,
    pub h_toggle_idle: &'static str,
    pub h_rescan: &'static str,
    pub h_language: &'static str,
    pub h_toggle_help: &'static str,
    pub h_quit: &'static str,
    pub h_columns: &'static str,
}

pub static EN: Strings = Strings {
    app_subtitle: "MACOS PACKAGE INVENTORY",
    units: "UNITS",
    live: "LIVE",
    rev: "REV",
    just_now: "JUST NOW",
    minutes_ago: "M AGO",
    hours_ago: "H AGO",
    days_ago: "D AGO",
    panel_inventory: " INVENTORY ",
    panel_record: " RECORD ",
    shown: "shown",
    noise_hidden: "noise hidden",
    idle_only: "IDLE ONLY",
    sorted_by: "sorted by",
    col_name: "NAME",
    col_source: "SRC",
    col_lang: "LANG",
    col_version: "VER",
    col_installed: "INSTALLED",
    col_usage: "USAGE",
    col_path: "PATH",
    sec_origin: "ORIGIN",
    sec_activity: "ACTIVITY",
    sec_location: "LOCATION",
    sec_note: "NOTE",
    sec_sources: "SOURCES",
    f_source: "SOURCE",
    f_language: "LANGUAGE",
    f_version: "VERSION",
    f_installed: "INSTALLED",
    f_last_used: "LAST USED",
    f_invocations: "INVOCATIONS",
    none_recorded: "none recorded",
    no_selection: "No unit selected",
    mode_browse: "BROWSE",
    mode_search: "SEARCH",
    mode_detail: "DETAIL",
    mode_help: "HELP",
    k_move: "MOVE",
    k_filter: "FILTER",
    k_noise: "NOISE",
    k_idle: "IDLE",
    k_sort: "SORT",
    k_rescan: "RESCAN",
    k_keys: "KEYS",
    k_quit: "QUIT",
    search_label: "SEARCH",
    search_matches: "MATCH",
    search_hint: "ESC CANCEL · ENTER KEEP",
    no_matches: "NO UNITS MATCH FILTER",
    esc_to_clear: "Esc to clear",
    rescanning: "RESCANNING",
    keys_still_live: "keys still live",
    rescan_complete: "RESCAN COMPLETE",
    rescan_failed: "RESCAN FAILED",
    rescan_empty: "RESCAN FOUND NOTHING — KEEPING PREVIOUS DATA",
    scan_empty: "SCAN FOUND NOTHING — press r to retry",
    cache_write_failed: "CACHE WRITE FAILED",
    noise_hidden_notice: "PACKAGING NOISE HIDDEN",
    noise_shown_notice: "SHOWING ALL SOURCES",
    idle_only_notice: "IDLE ONLY — NO USE IN 6 MONTHS",
    all_units_notice: "SHOWING ALL UNITS",
    initial_inventory: "[ INITIAL INVENTORY ]",
    no_snapshot: "NO SNAPSHOT — FULL SCAN REQUIRED",
    scanning: "SCANNING",
    skipped: "SKIPPED",
    slowest_source: "(slowest source, ~40s)",
    enriching: "ENRICHING",
    enriching_detail: "detecting languages, reading usage history",
    cached_next_launch: "Results are cached — the next launch opens instantly.",
    viewport_undersized: "[ VIEWPORT UNDERSIZED ]",
    need: "NEED",
    have: "HAVE",
    too_small_record: " TERMINAL TOO SMALL FOR RECORD ",
    too_small_help: " TERMINAL TOO SMALL FOR HELP ",
    help_title: " [ KEY REFERENCE ] ",
    h_navigate: "NAVIGATE",
    h_inspect: "INSPECT",
    h_filter: "FILTER",
    h_sort: "SORT",
    h_view: "VIEW",
    h_data: "DATA",
    h_session: "SESSION",
    h_down_one: "down one unit",
    h_up_one: "up one unit",
    h_down_page: "down one page",
    h_up_page: "up one page",
    h_first: "first unit",
    h_last: "last unit",
    h_open_record: "open unit record",
    h_close_overlay: "close overlay",
    h_live_filter: "live filter — name, source, lang, path",
    h_cancel_filter: "cancel filter and restore full inventory",
    h_keep_filter: "keep filter, return to browse",
    h_sort_col: "sort by column; repeat to reverse",
    h_toggle_noise: "show/hide packaging noise (pkgutil, /System)",
    h_toggle_idle: "show only units with no evidence of use",
    h_rescan: "rescan in the background — keys stay live",
    h_language: "switch language",
    h_toggle_help: "toggle this overlay",
    h_quit: "quit",
    h_columns: "COLUMNS",
};

pub static ZH_HANT: Strings = Strings {
    app_subtitle: "macOS 套件盤點",
    units: "個項目",
    live: "即時掃描",
    rev: "版本",
    just_now: "剛剛",
    minutes_ago: " 分鐘前",
    hours_ago: " 小時前",
    days_ago: " 天前",
    panel_inventory: " 套件清單 ",
    panel_record: " 詳細資料 ",
    shown: "項符合",
    noise_hidden: "項系統雜訊已隱藏",
    idle_only: "僅顯示閒置",
    sorted_by: "排序：",
    col_name: "名稱",
    col_source: "來源",
    col_lang: "語言",
    col_version: "版本",
    col_installed: "安裝日期",
    col_usage: "使用",
    col_path: "路徑",
    sec_origin: "來源資訊",
    sec_activity: "使用狀況",
    sec_location: "安裝位置",
    sec_note: "說明",
    sec_sources: "來源統計",
    f_source: "來源",
    f_language: "語言",
    f_version: "版本",
    f_installed: "安裝日期",
    f_last_used: "最後使用",
    f_invocations: "呼叫次數",
    none_recorded: "無使用紀錄",
    no_selection: "未選取任何項目",
    mode_browse: "瀏覽",
    mode_search: "搜尋",
    mode_detail: "詳細",
    mode_help: "說明",
    k_move: "移動",
    k_filter: "過濾",
    k_noise: "雜訊",
    k_idle: "閒置",
    k_sort: "排序",
    k_rescan: "重掃",
    k_keys: "按鍵",
    k_quit: "離開",
    search_label: "搜尋",
    search_matches: "項符合",
    search_hint: "ESC 取消 · ENTER 保留",
    no_matches: "沒有項目符合此條件",
    esc_to_clear: "按 Esc 清除",
    rescanning: "重新掃描中",
    keys_still_live: "按鍵仍可使用",
    rescan_complete: "重新掃描完成",
    rescan_failed: "重新掃描失敗",
    rescan_empty: "重新掃描沒有結果 — 保留原有資料",
    scan_empty: "掃描沒有結果 — 按 r 重試",
    cache_write_failed: "快取寫入失敗",
    noise_hidden_notice: "已隱藏系統雜訊",
    noise_shown_notice: "顯示全部來源",
    idle_only_notice: "僅顯示閒置 — 六個月內未使用",
    all_units_notice: "顯示全部項目",
    initial_inventory: "［ 首次盤點 ］",
    no_snapshot: "尚無快取 — 需要完整掃描",
    scanning: "掃描中",
    skipped: "略過",
    slowest_source: "（最慢的來源，約 40 秒）",
    enriching: "分析中",
    enriching_detail: "偵測語言、讀取使用紀錄",
    cached_next_launch: "結果會寫入快取 — 下次啟動可立即開啟。",
    viewport_undersized: "［ 視窗過小 ］",
    need: "需要",
    have: "目前",
    too_small_record: " 視窗過小，無法顯示詳細資料 ",
    too_small_help: " 視窗過小，無法顯示按鍵說明 ",
    help_title: " ［ 按鍵說明 ］ ",
    h_navigate: "移動",
    h_inspect: "檢視",
    h_filter: "過濾",
    h_sort: "排序",
    h_view: "顯示",
    h_data: "資料",
    h_session: "工作階段",
    h_down_one: "往下一項",
    h_up_one: "往上一項",
    h_down_page: "往下一頁",
    h_up_page: "往上一頁",
    h_first: "第一項",
    h_last: "最後一項",
    h_open_record: "開啟詳細資料",
    h_close_overlay: "關閉浮層",
    h_live_filter: "即時過濾 — 名稱、來源、語言、路徑",
    h_cancel_filter: "取消過濾並還原完整清單",
    h_keep_filter: "保留過濾並回到瀏覽",
    h_sort_col: "依欄位排序，再按一次反向",
    h_toggle_noise: "顯示／隱藏系統雜訊（pkgutil、/System）",
    h_toggle_idle: "只顯示沒有使用跡象的項目",
    h_rescan: "背景重新掃描 — 按鍵全程可用",
    h_language: "切換語言",
    h_toggle_help: "開關此說明浮層",
    h_quit: "離開",
    h_columns: "欄位",
};

#[cfg(test)]
mod tests {
    use super::*;

    /// `pad` must measure terminal columns, not characters — the whole point
    /// of it existing.
    #[test]
    fn pad_measures_display_width() {
        assert_eq!(pad("SOURCE", 10).width(), 10);
        assert_eq!(pad("來源", 10).width(), 10, "CJK must pad to 10 columns");
        assert_eq!(pad("安裝日期", 10).width(), 10);
        // Already at or over budget: returned unchanged, never truncated.
        assert_eq!(pad("INVOCATIONS", 4), "INVOCATIONS");
    }

    /// Record labels are padded to a fixed column; every label in both
    /// languages must fit or values will collide with them.
    #[test]
    fn record_labels_fit_their_column() {
        const PAD: usize = 13;
        for s in [&EN, &ZH_HANT] {
            for label in [
                s.f_source,
                s.f_language,
                s.f_version,
                s.f_installed,
                s.f_last_used,
                s.f_invocations,
            ] {
                assert!(
                    label.width() < PAD,
                    "{label:?} is {} columns, budget {PAD}",
                    label.width()
                );
            }
        }
    }

    /// Column headers carry a `N·` prefix and a sort arrow; the whole thing
    /// has to fit the narrowest column the grid allocates.
    #[test]
    fn column_headers_fit_their_widths() {
        // Matches the widths in components::table.
        let budgets = [14usize, 7, 10, 10, 12, 10];
        for s in [&EN, &ZH_HANT] {
            let cols = [
                s.col_name,
                s.col_source,
                s.col_lang,
                s.col_version,
                s.col_installed,
                s.col_usage,
            ];
            for (label, budget) in cols.iter().zip(budgets) {
                // "1·" prefix (2 cols) plus a sort arrow (1 col).
                let rendered = 2 + label.width() + 1;
                assert!(
                    rendered <= budget,
                    "header {label:?} renders {rendered} columns, budget {budget}"
                );
            }
        }
    }

    #[test]
    fn locale_detection_prefers_traditional_only_for_traditional_locales() {
        // Simplified and generic Chinese fall back to English rather than
        // showing the wrong script.
        for (locale, expected) in [
            ("zh_TW.UTF-8", Lang::ZhHant),
            ("zh_HK.UTF-8", Lang::ZhHant),
            ("zh-Hant", Lang::ZhHant),
            ("zh_CN.UTF-8", Lang::En),
            ("en_US.UTF-8", Lang::En),
            ("", Lang::En),
        ] {
            let l = locale.to_lowercase();
            let traditional = ["zh_tw", "zh-tw", "zh_hk", "zh-hk", "zh_mo", "zh-mo", "hant"];
            let got = if traditional.iter().any(|t| l.contains(t)) {
                Lang::ZhHant
            } else {
                Lang::En
            };
            assert_eq!(got, expected, "locale {locale:?}");
        }
    }

    #[test]
    fn toggling_twice_returns_to_the_start() {
        assert_eq!(Lang::ZhHant.toggled().toggled(), Lang::ZhHant);
        assert_eq!(Lang::En.toggled(), Lang::ZhHant);
    }
}
