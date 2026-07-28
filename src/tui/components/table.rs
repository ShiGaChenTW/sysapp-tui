//! The data grid — the component that actually is the interface.
//!
//! Owns cursor and sort state. `view` takes `&self` (tears keeps the model
//! immutable during render), while ratatui's `TableState` needs `&mut` to
//! compute its scroll offset, so the state lives behind a `RefCell`.

use std::cell::RefCell;
use std::cmp::Ordering;

use ratatui::Frame;
use ratatui::text::{Line, Span};
use ratatui::layout::{Constraint as C, Layout, Rect};
use ratatui::widgets::{Block, BorderType, Borders, Cell, Padding, Paragraph, Row, Table, TableState};

use unicode_width::UnicodeWidthStr;
use std::collections::HashSet;

use crate::model::AppEntry;
use crate::tui::i18n::{self, Lang};
use crate::tui::message::Column;
use crate::tui::theme::Theme;

const COMPACT_COLUMNS: &[Column] = &[
    Column::Name,
    Column::Source,
    Column::UiKind,
    Column::Installed,
    Column::Usage,
];
const MEDIUM_COLUMNS: &[Column] = &[
    Column::Name,
    Column::Source,
    Column::UiKind,
    Column::Installed,
    Column::Usage,
    Column::Category,
    Column::LastUsed,
];
const FULL_COLUMNS: &[Column] = &[
    Column::Name,
    Column::Source,
    Column::UiKind,
    Column::Installed,
    Column::Usage,
    Column::Category,
    Column::LastUsed,
    Column::Lang,
    Column::Version,
];

pub(crate) const MEDIUM_MIN_CONTENT_WIDTH: u16 = 74;
pub(crate) const FULL_MIN_CONTENT_WIDTH: u16 = 96;

pub struct DataGrid {
    state: RefCell<TableState>,
    pub sort_col: Column,
    pub sort_asc: bool,
}

impl Default for DataGrid {
    fn default() -> Self {
        Self {
            state: RefCell::new(TableState::default().with_selected(Some(0))),
            sort_col: Column::Name,
            sort_asc: true,
        }
    }
}

impl DataGrid {
    pub fn selected(&self) -> Option<usize> {
        self.state.borrow().selected()
    }

    pub fn select(&mut self, index: Option<usize>) {
        self.state.get_mut().select(index);
    }

    /// Move the cursor by a signed delta, clamped to `len`.
    ///
    /// Clamping rather than wrapping: wrapping from the last row to the first
    /// on a long list is disorienting when the user is holding `j`.
    pub fn move_by(&mut self, delta: i32, len: usize) {
        if len == 0 {
            self.select(None);
            return;
        }
        let cur = self.selected().unwrap_or(0) as i64;
        let max = len as i64 - 1;
        let next = (cur + delta as i64).clamp(0, max);
        self.select(Some(next as usize));
    }

    /// Select a column; re-selecting the active column reverses direction.
    ///
    /// A newly selected column starts in whichever direction is useful for it
    /// (see [`Column::default_ascending`]), not blindly ascending.
    pub fn toggle_sort(&mut self, col: Column) {
        if self.sort_col == col {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_col = col;
            self.sort_asc = col.default_ascending();
        }
    }

    /// Draw the grid inside a titled panel.
    ///
    /// `stats` is the filter/sort summary shown as the panel's first line —
    /// it belongs next to the data it describes rather than in the masthead.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        frame: &mut Frame,
        area: Rect,
        entries: &[AppEntry],
        rows: &[usize],
        starred: &HashSet<String>,
        stats: Line<'_>,
        lang: Lang,
        theme: &Theme,
    ) {
        // Paint the panel interior before anything else. Cell styles cover
        // cells only; column gaps and short rows would otherwise expose the
        // terminal background as vertical stripes.
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(theme.panel_border())
            .title(Span::styled(lang.strings().panel_inventory, theme.panel_title()))
            .padding(Padding::new(1, 1, 1, 0))
            .style(theme.base());
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if inner.height < 2 {
            return;
        }

        let [stats_area, _gap, grid] = Layout::vertical([
            C::Length(1),
            C::Length(1),
            C::Min(1),
        ])
        .areas(inner);

        frame.render_widget(Paragraph::new(stats).style(theme.base()), stats_area);

        if rows.is_empty() {
            let t = lang.strings();
            let msg = Line::from(vec![
                Span::styled(" >>> ", theme.accented()),
                Span::styled(t.no_matches, theme.heading()),
                Span::styled(format!("   {}", t.esc_to_clear), theme.muted()),
            ]);
            frame.render_widget(Paragraph::new(msg).style(theme.base()), grid);
            return;
        }

        // Nine columns never fit at once, and ratatui answers an over-budget
        // width by silently clipping the rightmost headers rather than
        // reporting anything — which shipped twice as `5·INSTALLE`. So the set
        // is chosen by width instead of fixed.
        //
        // Tiering reads the full table width handed to ratatui, not a post-gap
        // remainder, which keeps the decision non-circular: the gap count
        // depends on the column count, which is what we are choosing. Each tier
        // costs `1 highlight + (n - 1) gaps + Σ width(col)`, with NAME
        // contributing its `Min`. Measured, those close at 53 / 74 / 96
        // columns. The spec asked for 70 as the middle floor; the fixed widths
        // overrun it by four once the headers are wide enough to survive CJK
        // labels, so the threshold moved rather than the headers being clipped
        // to fit — `visible_column_tiers_close_at_their_narrowest_widths` is
        // the executable form of that budget.
        let columns = visible_columns(grid.width);
        let widths = columns
            .iter()
            .map(|col| match col {
                Column::Name => C::Min(width(*col)),
                _ => C::Length(width(*col)),
            })
            .collect::<Vec<_>>();

        let header = Row::new(
            columns
                .iter()
                .map(|col| {
                    let active = *col == self.sort_col;
                    let logical = Column::ALL
                        .iter()
                        .position(|candidate| candidate == col)
                        .expect("visible columns stay in Column::ALL");
                    // The digit hint stays on every column, active or not —
                    // dropping it from the sorted column hides the binding the
                    // user is most likely to press again.
                    let text = if active {
                        format!(
                            "{}·{}{}",
                            logical + 1,
                            col.label(lang),
                            if self.sort_asc { "▲" } else { "▼" }
                        )
                    } else {
                        format!("{}·{}", logical + 1, col.label(lang))
                    };
                    Cell::from(text).style(if active {
                        theme.column_header_active()
                    } else {
                        theme.column_header()
                    })
                })
                .collect::<Vec<_>>(),
        )
        .style(theme.base());

        let body: Vec<Row> = rows
            .iter()
            .map(|&i| self.row(&entries[i], starred.contains(&entries[i].name), columns, lang, theme))
            .collect();

        let table = Table::new(body, widths)
            .header(header)
            .style(theme.base())
            .row_highlight_style(theme.selection())
            .highlight_symbol(Span::styled("▌", theme.accented()));

        let mut state = self.state.borrow_mut();
        frame.render_stateful_widget(table, grid, &mut state);
    }

    fn row<'a>(&self, e: &'a AppEntry, starred: bool, columns: &[Column], lang: Lang, theme: &Theme) -> Row<'a> {
        let install = e
            .install_date
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "—".into());
        let last_used = e
            .last_used
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "—".into());
        let language = e
            .language
            .as_ref()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "—".into());
        let interface = e
            .ui_kind
            .map(|value| value.to_string())
            .unwrap_or_else(|| "—".into());
        let category = e
            .category
            .as_ref()
            .map(|value| i18n::category_label(value, lang).into_owned())
            .unwrap_or_else(|| "—".into());

        // USAGE is the only place the terminal green appears in the whole
        // interface (industrial-brutalist-ui §4: one element, no more).
        let (usage, usage_style) = if e.usage_count > 0 {
            (
                format!("{} {}x", meter(e.usage_count), e.usage_count),
                theme.meter_style(),
            )
        } else {
            ("—".into(), theme.muted())
        };

        Row::new(
            columns
                .iter()
                .map(|col| match col {
                    Column::Name => Cell::from(format!(
                        "{}{}",
                        if starred { "★ " } else { "  " },
                        truncate(&e.name, 28)
                    ))
                    .style(if starred { theme.accented() } else { theme.base() }),
                    Column::Source => {
                        Cell::from(truncate(&e.source.to_string().to_uppercase(), width(*col) as usize))
                            .style(theme.muted())
                    }
                    Column::UiKind => {
                        Cell::from(truncate(&interface, width(*col) as usize)).style(theme.muted())
                    }
                    Column::Installed => Cell::from(install.clone()).style(theme.muted()),
                    Column::Usage => Cell::from(usage.clone()).style(usage_style),
                    Column::Category => {
                        Cell::from(truncate(&category, width(*col) as usize)).style(theme.muted())
                    }
                    Column::LastUsed => Cell::from(last_used.clone()).style(theme.muted()),
                    Column::Lang => {
                        Cell::from(truncate(&language, width(*col) as usize)).style(theme.muted())
                    }
                    Column::Version => Cell::from(
                        truncate(e.version.as_deref().unwrap_or("—"), width(*col) as usize),
                    )
                    .style(theme.muted()),
                })
                .collect::<Vec<_>>(),
        )
        .style(theme.base())
    }
}

/// The visible slice must come from the raw table width, before any per-column
/// budgeting is subtracted, or the tier decision becomes self-referential.
pub(crate) fn visible_columns(content_width: u16) -> &'static [Column] {
    if content_width >= FULL_MIN_CONTENT_WIDTH {
        FULL_COLUMNS
    } else if content_width >= MEDIUM_MIN_CONTENT_WIDTH {
        MEDIUM_COLUMNS
    } else {
        COMPACT_COLUMNS
    }
}

pub(crate) fn width(col: Column) -> u16 {
    match col {
        Column::Name => 14,
        Column::Source => 7,
        Column::UiKind => 7,
        Column::Installed => 12,
        Column::Usage => 8,
        // 13 columns, not 7: the built-in category names are words, and the
        // longest ("Communication") is 13. At 7 almost every row rendered as
        // `Develo…` — still unambiguous by prefix, but a column that is
        // ellipsised on nearly every row is noise where a label should be.
        Column::Category => 13,
        Column::LastUsed => 12,
        Column::Lang => 10,
        Column::Version => 10,
    }
}

/// A four-step sparkline bucket for invocation counts. Log-ish thresholds
/// because shell history counts are heavily long-tailed.
fn meter(count: u32) -> &'static str {
    match count {
        0 => "▁",
        1..=9 => "▂",
        10..=49 => "▄",
        50..=199 => "▆",
        _ => "█",
    }
}

/// Width-aware truncation, where `max` is terminal *columns*, not characters.
///
/// Counting characters overflows the column for CJK names — "系統設定" is four
/// chars but eight columns wide. Byte slicing would be worse still: it panics
/// mid-codepoint.
pub(crate) fn truncate(text: &str, max: usize) -> String {
    if text.width() <= max {
        return text.to_string();
    }
    // Reserve one column for the ellipsis.
    let budget = max.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = ch.to_string().width();
        if used + w > budget {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

/// Column comparator, shared with the model so sort order and rendering can
/// never disagree about what a column means.
pub fn compare(a: &AppEntry, b: &AppEntry, col: Column) -> Ordering {
    match col {
        Column::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        Column::Source => a.source.to_string().cmp(&b.source.to_string()),
        Column::UiKind => ui_kind_key(a).cmp(&ui_kind_key(b)),
        Column::Lang => lang_key(a).cmp(&lang_key(b)),
        Column::Version => a
            .version
            .as_deref()
            .unwrap_or("")
            .cmp(b.version.as_deref().unwrap_or("")),
        Column::Installed => a.install_date.cmp(&b.install_date),
        Column::Usage => a.usage_count.cmp(&b.usage_count),
        Column::Category => category_key(a).cmp(category_key(b)),
        Column::LastUsed => a.last_used.cmp(&b.last_used),
    }
}

fn lang_key(e: &AppEntry) -> String {
    e.language.as_ref().map(|l| l.to_string()).unwrap_or_default()
}

fn ui_kind_key(e: &AppEntry) -> String {
    e.ui_kind.map(|kind| kind.to_string()).unwrap_or_default()
}

/// Sorts on the untranslated key rather than the displayed label: `compare` has
/// no language, and a sort order that reshuffles when the user presses `L`
/// would make the ▲/▼ marker describe an order the column no longer has.
fn category_key(e: &AppEntry) -> &str {
    e.category.as_ref().map(|c| c.key()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_never_exceeds_its_column_budget() {
        for (text, max) in [
            ("日本語のアプリ名です", 10usize),
            ("系統設定", 5),
            ("Visual Studio Code", 8),
            ("短", 1),
            ("mixed中文and英文name", 12),
        ] {
            let out = truncate(text, max);
            assert!(
                out.width() <= max,
                "{text:?} -> {out:?} is {} cols, budget {max}",
                out.width()
            );
        }
    }

    #[test]
    fn truncate_leaves_fitting_text_alone() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("exactly10!", 10), "exactly10!");
        assert_eq!(truncate("系統設定", 8), "系統設定");
    }

    /// A CJK name must be cut on a column boundary, not a character count.
    #[test]
    fn truncate_is_width_aware_not_char_aware() {
        // 5 chars = 10 columns; budget 7 leaves 6 for content = 3 glyphs.
        assert_eq!(truncate("日本語のア", 7), "日本語…");
    }

    #[test]
    fn move_by_clamps_and_never_wraps() {
        let mut g = DataGrid::default();
        g.move_by(-5, 10);
        assert_eq!(g.selected(), Some(0));
        g.move_by(100, 10);
        assert_eq!(g.selected(), Some(9));
    }

    #[test]
    fn empty_list_clears_selection() {
        let mut g = DataGrid::default();
        g.move_by(1, 0);
        assert_eq!(g.selected(), None);
    }

    #[test]
    fn toggle_sort_flips_only_on_same_column() {
        let mut g = DataGrid::default();
        assert!(g.sort_asc);
        g.toggle_sort(Column::Name); // same column → flip
        assert!(!g.sort_asc);
        // A new column adopts its own default direction — usage starts
        // descending so the most-used unit is on top.
        g.toggle_sort(Column::Usage);
        assert!(!g.sort_asc);
        assert_eq!(g.sort_col, Column::Usage);
        g.toggle_sort(Column::Usage); // same column → flip
        assert!(g.sort_asc);
    }

    /// Narrowest grid the application can produce: `MIN_WIDTH` 60, less the
    /// 2-column margin each side, the shadow column, and the panel's 2 borders
    /// plus 2 padding. Nothing selects a tier below this, so it is the compact
    /// tier's real floor — asserting that set against its own width, as the
    /// first draft did, is a tautology that guards nothing.
    const COMPACT_MIN_CONTENT_WIDTH: u16 = 51;

    /// The `▌` cursor marker ratatui reserves ahead of the first column.
    const HIGHLIGHT_WIDTH: u16 = 1;

    /// A name column narrower than this is useless, so it is the floor NAME's
    /// `Min` constraint has to be left after every fixed column is paid for.
    const NAME_MIN_VISIBLE: u16 = 8;

    /// The budget the old code carried as a hand-computed comment, made
    /// executable. Overrun it and ratatui silently clips the rightmost headers
    /// instead of reporting anything — the failure this whole tiering exists to
    /// prevent. NAME is the one flexible column, so the guard is that the fixed
    /// columns, the inter-column gaps and the cursor marker still leave it room.
    #[test]
    fn visible_column_tiers_close_at_their_narrowest_widths() {
        for (floor, columns) in [
            (COMPACT_MIN_CONTENT_WIDTH, COMPACT_COLUMNS),
            (MEDIUM_MIN_CONTENT_WIDTH, MEDIUM_COLUMNS),
            (FULL_MIN_CONTENT_WIDTH, FULL_COLUMNS),
        ] {
            assert_eq!(visible_columns(floor), columns, "width {floor} picks the wrong tier");

            let fixed: u16 = columns
                .iter()
                .filter(|col| **col != Column::Name)
                .map(|col| width(*col))
                .sum();
            let overhead = HIGHLIGHT_WIDTH + columns.len().saturating_sub(1) as u16;
            assert!(
                fixed + overhead + NAME_MIN_VISIBLE <= floor,
                "{columns:?} need {} fixed + {NAME_MIN_VISIBLE} for NAME, budget {floor}",
                fixed + overhead
            );
        }
    }

    /// Widening the terminal must never take a column away, and every visible
    /// column must be one the digit keys can actually reach.
    #[test]
    fn widening_never_hides_a_column() {
        let mut previous: &[Column] = &[];

        for content_width in 0..=140 {
            let columns = visible_columns(content_width);
            assert!(!columns.is_empty());
            assert!(
                columns.len() >= previous.len(),
                "width {content_width} hid columns"
            );
            // Each tier extends the narrower one rather than swapping columns
            // around, so a resize never relocates a column the user was reading.
            assert!(
                previous.iter().all(|col| columns.contains(col)),
                "width {content_width} dropped a column the narrower tier showed"
            );
            for col in columns {
                assert!(
                    Column::ALL.contains(col),
                    "{col:?} is not reachable from a digit key"
                );
            }
            previous = columns;
        }
    }
}
