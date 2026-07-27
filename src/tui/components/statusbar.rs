//! Footer: bracketed key hints, plus transient state.
//!
//! Tier one of the three-tier help system — the handful of keys worth carrying
//! permanently. Unbordered and dim so it reads as chrome, not content.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use unicode_width::UnicodeWidthStr;

use crate::tui::i18n::Lang;
use crate::tui::message::Mode;
use crate::tui::theme::Theme;

/// Braille dots — the default modern spinner (tui-design §5).
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Key hints, built per language. The key itself never translates — `j` is
/// `j` on every keyboard — only what it does.
///
/// **Ordered by what has to survive a narrow terminal**, not by theme: the row
/// no longer fits at 120 columns and `render` drops from the tail. Movement,
/// filtering and the two escape hatches (`?`, `q`) come first because a user
/// who cannot read the rest still has to be able to leave.
fn keys(lang: Lang) -> [(&'static str, &'static str); 12] {
    let t = lang.strings();
    [
        ("j/k", t.k_move),
        ("Enter", t.k_run),
        ("/", t.k_filter),
        ("?", t.k_keys),
        ("q", t.k_quit),
        ("c", t.k_cat_filter),
        ("C", t.k_cat_set),
        ("p", t.k_noise),
        ("s", t.k_idle),
        ("1-9", t.k_sort),
        ("r", t.k_rescan),
        ("L", lang.other_name()),
    ]
}

pub struct StatusBar<'a> {
    pub mode: Mode,
    pub position: Option<usize>,
    pub total: usize,
    pub refreshing: bool,
    pub tick: usize,
    pub notice: Option<&'a str>,
    /// Name of the unit awaiting a `y`. Outranks everything else on this row:
    /// it is a question the user has to answer before anything else happens,
    /// and it lives here rather than in its own widget so the mode and
    /// position block on the right survives it like it survives a notice.
    pub confirm: Option<&'a str>,
    pub lang: Lang,
}

impl StatusBar<'_> {
    /// Columns the right-aligned `MODE  n/total` block will claim.
    ///
    /// The key hints have to stop short of it: it is drawn second, over the
    /// same row, so anything the hints put underneath is simply lost.
    fn position_width(&self) -> usize {
        if self.total == 0 {
            return 0;
        }
        let pos = self.position.map(|i| i + 1).unwrap_or(0);
        format!("{}  {pos}/{} ", self.mode.label(self.lang), self.total).width()
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Precedence: an in-flight rescan outranks a finished one's notice,
        // which outranks the static key hints.
        let mut spans = Vec::new();

        let t = self.lang.strings();
        if let Some(name) = self.confirm {
            // The name carries the band styling: it is the whole content of
            // the question, and a prompt that reads as chrome is a prompt
            // people answer without reading.
            spans.push(Span::styled(
                format!(" {} ", t.confirm_run_l.trim()),
                theme.masthead(),
            ));
            spans.push(Span::styled(format!(" {name}"), theme.heading()));
            spans.push(Span::styled(t.confirm_run_r, theme.accented()));
        } else if self.refreshing {
            spans.push(Span::styled(
                format!(" {} {}", SPINNER[self.tick % SPINNER.len()], t.rescanning),
                theme.accented(),
            ));
            spans.push(Span::styled(format!("  {}", t.keys_still_live), theme.muted()));
        } else if let Some(notice) = self.notice {
            spans.push(Span::styled(format!(" {notice}"), theme.accented()));
        } else {
            // Render only the hints that fit. Eleven of them overrun any
            // terminal this runs in, and ratatui clips the overflow silently —
            // so without this the tail hints would be half-drawn or gone with
            // nothing to say they existed, the same failure the column tiers
            // were rebuilt to avoid. Measured in display columns because the
            // Chinese labels are double-width and a character count fits one
            // language while clipping the other.
            let budget = usize::from(area.width).saturating_sub(self.position_width());
            let mut used = 0usize;
            for (key, label) in keys(self.lang) {
                let hint = format!(" [{key}] {label} ");
                let w = hint.width();
                if used + w > budget {
                    break;
                }
                used += w;
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

    /// Twelve hints do not fit any terminal this runs in, and ratatui clips
    /// the overflow without saying so. Every hint that renders must render
    /// whole, and the position block must survive in both languages — the
    /// Chinese labels are double-width, so a fit computed in characters would
    /// pass one language and clip the other.
    #[test]
    fn footer_drops_whole_hints_rather_than_clipping_them() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        use crate::tui::theme::Theme;

        for lang in [Lang::En, Lang::ZhHant] {
            for width in [80u16, 100, 120, 200] {
                let bar = StatusBar {
                    mode: Mode::Browse,
                    position: Some(41),
                    total: 909,
                    refreshing: false,
                    tick: 0,
                    notice: None,
                    confirm: None,
                    lang,
                };
                let mut term = Terminal::new(TestBackend::new(width, 1)).unwrap();
                term.draw(|f| bar.render(f, f.area(), &Theme::tactical()))
                    .unwrap();
                let buf = term.backend().buffer().clone();
                let row: String = (0..width).map(|x| buf[(x, 0)].symbol()).collect();

                // The position block is drawn last, over the hints, so its
                // survival is the thing the budget exists to protect.
                assert!(
                    row.contains("42/909"),
                    "{lang:?} at {width}: position block lost — {row:?}"
                );

                // Any hint that appears at all must appear complete: an
                // opening bracket with no closing one is a clipped hint.
                let opens = row.matches('[').count();
                let closes = row.matches(']').count();
                assert_eq!(
                    opens, closes,
                    "{lang:?} at {width}: a hint was cut mid-way — {row:?}"
                );
                assert!(opens > 0, "{lang:?} at {width}: no hints fit at all");
            }
        }
    }

    /// Every advertised key must actually be bound, or the footer lies.
    #[test]
    fn footer_keys_are_bound() {
        use crate::tui::keymap::translate;
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        for (key, _) in keys(Lang::ZhHant) {
            // A range advertises both ends, so both ends must translate —
            // checking only the first digit would let the footer claim 1-9
            // while the last columns had no binding at all. Named keys map to
            // their own `KeyCode`: deriving the probe from the first character
            // would test `E` and let a hint reading "Enter" pass while Enter
            // itself was unbound.
            let probes: Vec<KeyCode> = match key {
                "j/k" => vec![KeyCode::Char('j'), KeyCode::Char('k')],
                "1-9" => vec![KeyCode::Char('1'), KeyCode::Char('9')],
                "Enter" => vec![KeyCode::Enter],
                k => vec![KeyCode::Char(k.chars().next().unwrap())],
            };
            for code in probes {
                let ev = KeyEvent::new(code, KeyModifiers::NONE);
                assert!(
                    translate(Mode::Browse, ev).is_some(),
                    "footer advertises {key:?} but {code:?} is unbound"
                );
            }
        }
    }
}
