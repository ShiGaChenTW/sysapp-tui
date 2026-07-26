//! Cold-start progress screen.
//!
//! A first run with no cache spends ~90 seconds scanning, almost all of it
//! inside `brew info`. Previously that was a blank terminal with a few
//! `eprintln!` lines behind the alternate screen — indistinguishable from a
//! hang. This shows what is actually happening, per source, while it happens.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::theme::Theme;

const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Per-source state, as far as the UI is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceState {
    Pending,
    Done(usize),
    Skipped(String),
}

pub struct ScanningScreen<'a> {
    pub sources: &'a [(&'static str, SourceState)],
    pub tick: usize,
    /// Set once scanning finishes and enrichment begins.
    pub enriching: bool,
}

impl ScanningScreen<'_> {
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let spin = SPINNER[self.tick % SPINNER.len()];

        let mut lines = vec![
            Line::from(vec![
                Span::styled("SYSAPP", theme.heading()),
                Span::styled("·", theme.accented()),
                Span::styled("TUI", theme.heading()),
                Span::styled(" ®", theme.accented()),
            ]),
            Line::from(Span::styled(
                "━".repeat(area.width.min(72) as usize),
                theme.rule_style(),
            )),
            Line::from(vec![
                Span::styled("[ INITIAL INVENTORY ]", theme.accented()),
                Span::styled("  NO SNAPSHOT — FULL SCAN REQUIRED", theme.base()),
            ]),
            Line::from(Span::styled("", theme.base())),
        ];

        for (name, state) in self.sources {
            let (mark, text, style) = match state {
                SourceState::Pending => (spin, "SCANNING".to_string(), theme.accented()),
                SourceState::Done(n) => ("▪", format!("{n} UNITS"), theme.base()),
                SourceState::Skipped(why) => {
                    ("✗", format!("SKIPPED — {why}"), theme.muted())
                }
            };
            let mut spans = vec![
                Span::styled(format!(" {mark} "), style),
                Span::styled(format!("{:<10}", name.to_uppercase()), theme.heading()),
                Span::styled(text, style),
            ];
            // brew is the reason a cold scan takes a minute and a half; saying
            // so stops it reading as a hang.
            if *name == "brew" && matches!(state, SourceState::Pending) {
                spans.push(Span::styled("  (slowest source, ~40s)", theme.muted()));
            }
            lines.push(Line::from(spans));
        }

        lines.push(Line::from(Span::styled("", theme.base())));
        lines.push(if self.enriching {
            Line::from(vec![
                Span::styled(format!(" {spin} "), theme.accented()),
                Span::styled("ENRICHING", theme.heading()),
                Span::styled("  detecting languages, reading usage history", theme.muted()),
            ])
        } else {
            Line::from(Span::styled(
                " Results are cached — the next launch opens instantly.",
                theme.muted(),
            ))
        });

        // Top-left rather than centred: the list grows downward as sources
        // report, and a centred block would jitter on every update.
        let [body] = Layout::vertical([Constraint::Min(1)]).areas(area);
        frame.render_widget(Paragraph::new(lines).style(theme.base()), body);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_index_wraps_safely() {
        for tick in [0usize, 9, 10, 999, usize::MAX] {
            let _ = SPINNER[tick % SPINNER.len()];
        }
    }
}
