//! The `?` overlay — tier two of the three-tier help system.
//!
//! The footer carries the five essential keys; this carries everything the
//! footer cannot. Anything beyond this lives in the README.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::tui::components::detail::centered;
use crate::tui::message::Column;
use crate::tui::theme::Theme;

/// `(keys, description)` grouped under section headers. A `None` key marks
/// the row as a section title.
const BINDINGS: &[(Option<&str>, &str)] = &[
    (None, "NAVIGATE"),
    (Some("j / ↓"), "down one unit"),
    (Some("k / ↑"), "up one unit"),
    (Some("d / PgDn"), "down one page"),
    (Some("u / PgUp"), "up one page"),
    (Some("g / Home"), "first unit"),
    (Some("G / End"), "last unit"),
    (None, "INSPECT"),
    (Some("Enter / i"), "open unit record"),
    (Some("Esc"), "close overlay"),
    (None, "FILTER"),
    (Some("/"), "live filter — name, source, lang, path"),
    (Some("Esc"), "cancel filter and restore full inventory"),
    (Some("Enter"), "keep filter, return to browse"),
    (None, "SORT"),
    (Some("1 … 7"), "sort by column; repeat to reverse"),
    (None, "SESSION"),
    (Some("?"), "toggle this overlay"),
    (Some("q / Ctrl-C"), "quit"),
];

pub struct HelpOverlay;

impl HelpOverlay {
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let mut lines: Vec<Line> = Vec::with_capacity(BINDINGS.len() + 3);

        for (key, desc) in BINDINGS {
            match key {
                None => {
                    lines.push(Line::from(Span::styled("", theme.base())));
                    lines.push(Line::from(vec![
                        Span::styled(" ▌", theme.accented()),
                        Span::styled(desc.to_string(), theme.heading()),
                    ]));
                }
                Some(k) => lines.push(Line::from(vec![
                    Span::styled(format!("   {k:<12}"), theme.accented()),
                    Span::styled(desc.to_string(), theme.base()),
                ])),
            }
        }

        lines.push(Line::from(Span::styled("", theme.base())));
        lines.push(Line::from(vec![
            Span::styled("   COLUMNS  ", theme.muted()),
            Span::styled(
                Column::ALL
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("{}·{}", i + 1, c.label()))
                    .collect::<Vec<_>>()
                    .join("  "),
                theme.base(),
            ),
        ]));

        let rect = centered(area, 62, lines.len() as u16 + 2);
        if rect.width < 24 || rect.height < 5 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " TERMINAL TOO SMALL FOR HELP ",
                    theme.status_band(),
                )))
                .style(theme.base()),
                area,
            );
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(theme.accented())
            .title(Span::styled(" [ KEY REFERENCE ] ", theme.status_band()))
            .style(theme.overlay());

        frame.render_widget(Clear, rect);
        frame.render_widget(Paragraph::new(lines).block(block), rect);
    }
}

#[cfg(test)]
mod tests {
    /// Every key advertised in the overlay must actually be bound, or the
    /// help lies. Spot-checks the ones most likely to drift.
    #[test]
    fn advertised_keys_are_bound() {
        use crate::tui::keymap::translate;
        use crate::tui::message::{Message, Mode};
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

        let press = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);

        assert!(matches!(
            translate(Mode::Browse, press('d')),
            Some(Message::Move(n)) if n > 0
        ));
        assert!(matches!(
            translate(Mode::Browse, press('u')),
            Some(Message::Move(n)) if n < 0
        ));
        assert!(matches!(
            translate(Mode::Browse, press('?')),
            Some(Message::HelpToggle)
        ));
        assert!(matches!(
            translate(Mode::Browse, press('G')),
            Some(Message::JumpBottom)
        ));
    }
}
