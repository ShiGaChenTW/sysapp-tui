//! Footer: bracketed key hints, plus transient state.
//!
//! Tier one of the three-tier help system — the handful of keys worth carrying
//! permanently. Unbordered and dim so it reads as chrome, not content.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::i18n::Lang;
use crate::tui::message::Mode;
use crate::tui::theme::Theme;

/// Braille dots — the default modern spinner (tui-design §5).
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Key hints, built per language. The key itself never translates — `j` is
/// `j` on every keyboard — only what it does.
fn keys(lang: Lang) -> [(&'static str, &'static str); 9] {
    let t = lang.strings();
    [
        ("j/k", t.k_move),
        ("/", t.k_filter),
        ("p", t.k_noise),
        ("s", t.k_idle),
        ("1-6", t.k_sort),
        ("r", t.k_rescan),
        ("L", lang.other_name()),
        ("?", t.k_keys),
        ("q", t.k_quit),
    ]
}

pub struct StatusBar<'a> {
    pub mode: Mode,
    pub position: Option<usize>,
    pub total: usize,
    pub refreshing: bool,
    pub tick: usize,
    pub notice: Option<&'a str>,
    pub lang: Lang,
}

impl StatusBar<'_> {
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Precedence: an in-flight rescan outranks a finished one's notice,
        // which outranks the static key hints.
        let mut spans = Vec::new();

        let t = self.lang.strings();
        if self.refreshing {
            spans.push(Span::styled(
                format!(" {} {}", SPINNER[self.tick % SPINNER.len()], t.rescanning),
                theme.accented(),
            ));
            spans.push(Span::styled(format!("  {}", t.keys_still_live), theme.muted()));
        } else if let Some(notice) = self.notice {
            spans.push(Span::styled(format!(" {notice}"), theme.accented()));
        } else {
            for (key, label) in keys(self.lang) {
                spans.push(Span::styled(format!(" [{key}]"), theme.accented()));
                spans.push(Span::styled(format!(" {label} "), theme.muted()));
            }
        }

        frame.render_widget(Paragraph::new(Line::from(spans)).style(theme.base()), area);

        // Position sits hard right, always — it is the one piece of state that
        // must survive a notice taking over the left side.
        if self.total > 0 {
            let pos = self.position.map(|i| i + 1).unwrap_or(0);
            let mode = self.mode.label(self.lang);
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
        for (key, _) in keys(Lang::ZhHant) {
            let ch = match key {
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
