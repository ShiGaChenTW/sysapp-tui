//! Footer: bracketed key hints, plus transient state.
//!
//! Tier one of the three-tier help system — the handful of keys worth carrying
//! permanently. Unbordered and dim so it reads as chrome, not content.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::message::Mode;
use crate::tui::theme::Theme;

/// Braille dots — the default modern spinner (tui-design §5).
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

const KEYS: &[(&str, &str)] = &[
    ("j/k", "MOVE"),
    ("/", "FILTER"),
    ("p", "NOISE"),
    ("s", "IDLE"),
    ("1-6", "SORT"),
    ("r", "RESCAN"),
    ("?", "KEYS"),
    ("q", "QUIT"),
];

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
        // Precedence: an in-flight rescan outranks a finished one's notice,
        // which outranks the static key hints.
        let mut spans = Vec::new();

        if self.refreshing {
            spans.push(Span::styled(
                format!(" {} RESCANNING", SPINNER[self.tick % SPINNER.len()]),
                theme.accented(),
            ));
            spans.push(Span::styled("  keys still live", theme.muted()));
        } else if let Some(notice) = self.notice {
            spans.push(Span::styled(format!(" {notice}"), theme.accented()));
        } else {
            for (key, label) in KEYS {
                spans.push(Span::styled(format!(" [{key}]"), theme.accented()));
                spans.push(Span::styled(format!(" {label} "), theme.muted()));
            }
        }

        frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.base()), area);

        // Position sits hard right, always — it is the one piece of state that
        // must survive a notice taking over the left side.
        if self.total > 0 {
            let pos = self.position.map(|i| i + 1).unwrap_or(0);
            let mode = self.mode.label();
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("{mode}  {pos}/{} ", self.total),
                    theme.muted(),
                )))
                .style(theme.base())
                .right_aligned(),
                area,
            );
        }
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

    /// Every advertised key must actually be bound, or the footer lies.
    #[test]
    fn footer_keys_are_bound() {
        use crate::tui::keymap::translate;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        for (key, _) in KEYS {
            let ch = match *key {
                "j/k" => 'j',
                "1-6" => '1',
                k => k.chars().next().unwrap(),
            };
            let ev = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE);
            assert!(
                translate(Mode::Browse, ev).is_some(),
                "footer advertises {key:?} but {ch:?} is unbound"
            );
        }
    }
}
