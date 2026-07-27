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

use crate::model::AppEntry;
use crate::tui::i18n::Lang;
use crate::tui::message::Column;
use crate::tui::theme::Theme;

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

        // PATH is dropped from the grid: it is long, almost always elided, and
        // shown in full in the record panel. Its screen budget buys legible
        // columns for the fields that fit.
        // Budget at the narrowest side-by-side layout (116 cols):
        //   116 - 4 outer margin - 1 panel gap - 36 record = 75 grid
        //   75 - 2 border - 2 padding = 71 inner
        //   71 - 1 highlight symbol - 5 inter-column gaps = 65 content
        // The fixed columns take 49, leaving 16 for NAME. Exceed the budget
        // and ratatui silently clips the rightmost headers instead of
        // reporting anything — `columns_fit_at_minimum_width` guards it.
        let widths = [
            C::Min(14),      // NAME
            C::Length(7),    // SRC        — "PKGUTIL"
            C::Length(10),   // LANG       — "JavaScript"
            C::Length(10),   // VER
            C::Length(12),   // INSTALLED  — "5·INSTALLED▲" with the sort arrow
            C::Length(10),   // USAGE      — the "2026-07-20" fallback value
        ];

        let header = Row::new(
            Column::ALL
                .iter()
                .take(6)
                .enumerate()
                .map(|(i, col)| {
                    let active = *col == self.sort_col;
                    // The digit hint stays on every column, active or not —
                    // dropping it from the sorted column hides the binding the
                    // user is most likely to press again.
                    let text = if active {
                        format!(
                            "{}·{}{}",
                            i + 1,
                            col.label(lang),
                            if self.sort_asc { "▲" } else { "▼" }
                        )
                    } else {
                        format!("{}·{}", i + 1, col.label(lang))
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
            .map(|&i| self.row(&entries[i], theme))
            .collect();

        let table = Table::new(body, widths)
            .header(header)
            .style(theme.base())
            .row_highlight_style(theme.selection())
            .highlight_symbol(Span::styled("▌", theme.accented()));

        let mut state = self.state.borrow_mut();
        frame.render_stateful_widget(table, grid, &mut state);
    }

    fn row<'a>(&self, e: &'a AppEntry, theme: &Theme) -> Row<'a> {
        let lang = e
            .language
            .as_ref()
            .map(|l| l.to_string())
            .unwrap_or_else(|| "—".into());
        let install = e
            .install_date
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "—".into());

        // USAGE is the only place the terminal green appears in the whole
        // interface (industrial-brutalist-ui §4: one element, no more).
        let (usage, usage_style) = if e.usage_count > 0 {
            (
                format!("{} {}x", meter(e.usage_count), e.usage_count),
                theme.meter_style(),
            )
        } else if let Some(d) = e.last_used {
            (d.format("%Y-%m-%d").to_string(), theme.muted())
        } else {
            ("—".into(), theme.muted())
        };

        Row::new(vec![
            Cell::from(truncate(&e.name, 30)).style(theme.base()),
            Cell::from(e.source.to_string().to_uppercase()).style(theme.muted()),
            Cell::from(lang).style(theme.muted()),
            Cell::from(truncate(e.version.as_deref().unwrap_or("—"), 11)).style(theme.muted()),
            Cell::from(install).style(theme.muted()),
            Cell::from(usage).style(usage_style),
        ])
        .style(theme.base())
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
fn truncate(text: &str, max: usize) -> String {
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
        Column::Lang => lang_key(a).cmp(&lang_key(b)),
        Column::Version => a
            .version
            .as_deref()
            .unwrap_or("")
            .cmp(b.version.as_deref().unwrap_or("")),
        Column::Installed => a.install_date.cmp(&b.install_date),
        Column::Usage => a.usage_count.cmp(&b.usage_count),
        Column::Path => a
            .path
            .as_deref()
            .unwrap_or("")
            .cmp(b.path.as_deref().unwrap_or("")),
    }
}

fn lang_key(e: &AppEntry) -> String {
    e.language.as_ref().map(|l| l.to_string()).unwrap_or_default()
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
}
