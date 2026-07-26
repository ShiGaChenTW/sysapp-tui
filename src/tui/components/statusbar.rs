//! Footer band: current mode, cursor position, and the five essential keys.
//!
//! Tier one of the help system. The mode label is always present so the
//! interface can never be modally ambiguous.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::message::Mode;
use crate::tui::theme::Theme;

/// Braille dots — the default modern spinner (tui-design §5).
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct StatusBar<'a> {
    pub mode: Mode,
    pub position: Option<usize>,
    pub total: usize,
    pub refreshing: bool,
    pub tick: usize,
    pub notice: Option<&'a str>,
}

impl StatusBar<'_> {
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let position = match self.position {
            Some(i) if self.total > 0 => format!(" {}/{} ", i + 1, self.total),
            _ => " —/0 ".to_string(),
        };

        let mut spans = vec![
            Span::styled(format!(" {} ", self.mode.label()), theme.status_band()),
            Span::styled(position, theme.status_band_idle()),
        ];

        // Precedence: an in-flight rescan outranks a finished one's notice,
        // which outranks the static key hints.
        if self.refreshing {
            spans.push(Span::styled(
                format!("  {} RESCANNING", SPINNER[self.tick % SPINNER.len()]),
                theme.accented(),
            ));
            spans.push(Span::styled(
                "  (keys still live)",
                theme.muted(),
            ));
        } else if let Some(notice) = self.notice {
            spans.push(Span::styled(format!("  {notice}"), theme.accented()));
        } else {
            spans.push(Span::styled(
                "  j/k MOVE · / FILTER · Enter RECORD · r RESCAN · ? KEYS · q QUIT",
                theme.muted(),
            ));
        }

        frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.base()), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spinner index must never panic regardless of how long a rescan runs.
    #[test]
    fn spinner_index_wraps_safely() {
        for tick in [0usize, 9, 10, 999, usize::MAX] {
            let _ = SPINNER[tick % SPINNER.len()];
        }
    }
}
