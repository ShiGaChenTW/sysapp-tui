//! Structural header: identity plate, counters, and a source-density strip.
//!
//! Brutalist grammar per industrial-brutalist-ui §6 — uppercase throughout,
//! ASCII bracket framing, a registration mark used as a geometric element, and
//! a full-width rule segregating the operational unit from the data grid.

use chrono::{DateTime, Local};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::cache;
use crate::model::{AppEntry, Source};
use crate::tui::message::Column;
use crate::tui::theme::Theme;

/// Height this component needs. The caller reserves exactly this much.
pub const HEIGHT: u16 = 4;

pub struct HeaderBar<'a> {
    pub entries: &'a [AppEntry],
    pub shown: usize,
    pub sort_col: Column,
    pub sort_asc: bool,
    pub query: &'a str,
    pub generated_at: Option<DateTime<Local>>,
}

impl HeaderBar<'_> {
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height < HEIGHT {
            // Degrade rather than clip mid-glyph: identity plate only.
            self.render_plate(frame, area, theme);
            return;
        }

        let [plate, rule, counters, density] =
            Layout::vertical([Constraint::Length(1); 4]).areas(area);

        self.render_plate(frame, plate, theme);
        render_rule(frame, rule, theme);
        self.render_counters(frame, counters, theme);
        self.render_density(frame, density, theme);
    }

    /// `SYSAPP·TUI ®` left, revision metadata right.
    fn render_plate(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let left = Line::from(vec![
            Span::styled("SYSAPP", theme.heading()),
            Span::styled("·", theme.accented()),
            Span::styled("TUI", theme.heading()),
            Span::styled(" ®", theme.accented()),
        ]);
        // Staleness is never hidden: a cached inventory says how old it is,
        // a freshly scanned one says so explicitly.
        let freshness = match self.generated_at {
            Some(at) => format!("SNAPSHOT {}", cache::age_label(at)),
            None => "LIVE SCAN".to_string(),
        };
        let right = Line::from(vec![
            Span::styled(freshness, theme.accented()),
            Span::styled(
                format!("  REV {} / UNIT D-01 ", env!("CARGO_PKG_VERSION")),
                theme.muted(),
            ),
        ]);

        frame.render_widget(Paragraph::new(left).style(theme.base()), area);
        frame.render_widget(
            Paragraph::new(right)
                .style(theme.base())
                .right_aligned(),
            area,
        );
    }

    /// `[ INVENTORY ] 842  [ SHOWN ] 137  [ SORT ] NAME ▲  [ FILTER ] "py"`
    fn render_counters(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut spans = vec![
            Span::styled("[ INVENTORY ]", theme.accented()),
            Span::styled(format!(" {} ", self.entries.len()), theme.heading()),
            Span::styled("  [ SHOWN ]", theme.accented()),
            Span::styled(format!(" {} ", self.shown), theme.heading()),
            Span::styled("  [ SORT ]", theme.accented()),
            Span::styled(
                format!(
                    " {} {} ",
                    self.sort_col.label(),
                    if self.sort_asc { "▲" } else { "▼" }
                ),
                theme.heading(),
            ),
        ];

        if !self.query.is_empty() {
            spans.push(Span::styled("  [ FILTER ]", theme.accented()));
            spans.push(Span::styled(
                format!(" \"{}\" ", self.query.to_uppercase()),
                theme.heading(),
            ));
        }

        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(theme.base()),
            area,
        );
    }

    /// Horizontal bar chart of package counts per source — the densest useful
    /// summary of an inventory this shape (tui-design §5, block elements).
    fn render_density(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let counts = source_counts(self.entries);
        let Some(max) = counts.iter().map(|(_, n)| *n).max().filter(|n| *n > 0) else {
            return;
        };

        // Each source occupies a fixed-width cell so the strip stays a grid
        // rather than reflowing as counts change. The label field must clear
        // the longest source name ("PKGUTIL", 7) or the bar butts against it.
        const LABEL: usize = 8;
        const BAR: usize = 6;
        const COUNT: usize = 5;
        const GAP: usize = 2;
        const CELL: usize = LABEL + BAR + COUNT + GAP;

        let mut spans = Vec::new();
        let mut used = 0usize;
        for (source, n) in counts {
            if used + CELL > area.width as usize {
                break;
            }
            let filled = (n * BAR).div_ceil(max).min(BAR);
            spans.push(Span::styled(format!("{source:<LABEL$}"), theme.muted()));
            spans.push(Span::styled("█".repeat(filled), theme.accented()));
            spans.push(Span::styled(
                "░".repeat(BAR - filled),
                theme.rule_style(),
            ));
            spans.push(Span::styled(
                format!("{:<width$}", format!(" {n}"), width = COUNT + GAP),
                theme.base(),
            ));
            used += CELL;
        }

        frame.render_widget(
            Paragraph::new(Line::from(spans)).style(theme.base()),
            area,
        );
    }
}

fn render_rule(frame: &mut Frame, area: Rect, theme: &Theme) {
    // A full-width solid rule. Brutalist compartmentalization: zones are
    // separated by visible structure, not by whitespace.
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "━".repeat(area.width as usize),
            theme.rule_style(),
        )))
        .style(theme.base()),
        area,
    );
}

/// Counts per source, descending. Sources with zero entries are dropped.
fn source_counts(entries: &[AppEntry]) -> Vec<(String, usize)> {
    const ORDER: [Source; 9] = [
        Source::Homebrew,
        Source::HomebrewCask,
        Source::Applications,
        Source::Cargo,
        Source::Go,
        Source::Npm,
        Source::Pip,
        Source::Gem,
        Source::Pkgutil,
    ];

    let mut out: Vec<(String, usize)> = ORDER
        .iter()
        .map(|s| {
            let n = entries.iter().filter(|e| &e.source == s).count();
            (s.to_string().to_uppercase(), n)
        })
        .filter(|(_, n)| *n > 0)
        .collect();
    out.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(source: Source) -> AppEntry {
        AppEntry {
            name: "x".into(),
            version: None,
            source,
            language: None,
            install_date: None,
            last_used: None,
            usage_count: 0,
            path: None,
            description: None,
        }
    }

    #[test]
    fn counts_are_descending_and_drop_empties() {
        let entries = vec![
            entry(Source::Npm),
            entry(Source::Homebrew),
            entry(Source::Homebrew),
            entry(Source::Homebrew),
        ];
        let counts = source_counts(&entries);
        assert_eq!(counts, vec![("BREW".into(), 3), ("NPM".into(), 1)]);
    }

    /// The bar must never overflow its cell — a count equal to the max fills
    /// exactly BAR blocks, and div_ceil must not round past it.
    #[test]
    fn bar_width_is_bounded() {
        const BAR: usize = 6;
        for (n, max) in [(1usize, 1usize), (1, 1000), (999, 1000), (1000, 1000)] {
            let filled = (n * BAR).div_ceil(max).min(BAR);
            assert!(filled <= BAR, "n={n} max={max} filled={filled}");
            assert!(filled >= 1, "non-zero counts must show at least one block");
        }
    }
}
