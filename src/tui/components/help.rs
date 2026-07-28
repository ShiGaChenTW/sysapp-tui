//! The `?` overlay — tier two of the three-tier help system.
//!
//! The footer carries the five essential keys; this carries everything the
//! footer cannot. Anything beyond this lives in the README.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::tui::components::detail::centered;
use crate::tui::i18n::{self, Lang};
use crate::tui::message::Column;
use crate::tui::theme::Theme;

/// `(keys, description)` grouped under section headers. A `None` key marks
/// the row as a section title. Built per language rather than declared as a
/// const so the descriptions can be translated.
fn bindings(lang: Lang) -> Vec<(Option<&'static str>, &'static str)> {
    let t = lang.strings();
    let (star, recent) = match lang {
        Lang::En => ("star/unstar; starred units stay on top", "cycle installs: all / 7 days / 30 days"),
        Lang::ZhHant => ("設定／移除星號；星號項目固定置頂", "循環安裝日期：全部／近 7 天／近 30 天"),
    };
    vec![
        (None, t.h_navigate),
        (Some("j / ↓"), t.h_down_one),
        (Some("k / ↑"), t.h_up_one),
        (Some("d / PgDn"), t.h_down_page),
        (Some("u / PgUp"), t.h_up_page),
        (Some("g / Home"), t.h_first),
        (Some("G / End"), t.h_last),
        (None, t.h_run_section),
        (Some("Enter"), t.h_run),
        (Some("y / N"), t.h_confirm),
        (None, t.h_inspect),
        (Some("i / Tab"), t.h_open_record),
        (Some("Esc"), t.h_close_overlay),
        (None, t.h_filter),
        (Some("/"), t.h_live_filter),
        (Some("Esc"), t.h_cancel_filter),
        (Some("Enter"), t.h_keep_filter),
        (None, t.h_sort),
        (Some("1 … 9"), t.h_sort_col),
        (None, t.h_view),
        (Some("p"), t.h_toggle_noise),
        (Some("s"), t.h_toggle_idle),
        (Some("c"), t.h_cat_filter),
        (Some("C"), t.h_cat_set),
        (Some("*"), star),
        (Some("n"), recent),
        (None, t.h_data),
        (Some("r"), t.h_rescan),
        (None, t.h_session),
        (Some("L"), t.h_language),
        (Some("?"), t.h_toggle_help),
        (Some("q / Ctrl-C"), t.h_quit),
    ]
}

pub struct HelpOverlay {
    pub lang: Lang,
}

impl HelpOverlay {
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let t = self.lang.strings();
        let entries = bindings(self.lang);
        let mut lines: Vec<Line> = Vec::with_capacity(entries.len() + 3);
        let safe_height = area.height.saturating_sub(4);
        let spacious_height = entries.len() as u16
            + entries.iter().filter(|(key, _)| key.is_none()).count() as u16
            + 5;
        let spacious = spacious_height <= safe_height;
        let overlay_base = theme.base().bg(theme.bg_overlay);
        let overlay_muted = theme.muted().bg(theme.bg_overlay);
        let overlay_heading = theme.heading().bg(theme.bg_overlay);
        let overlay_accent = theme.accented().bg(theme.bg_overlay);

        for (key, desc) in &entries {
            match key {
                None => {
                    if spacious {
                        lines.push(Line::from(Span::styled("", overlay_base)));
                    }
                    lines.push(Line::from(vec![
                        Span::styled(" ▌", overlay_accent),
                        Span::styled(desc.to_string(), overlay_heading),
                    ]));
                }
                Some(k) => lines.push(Line::from(vec![
                    Span::styled(format!("   {k:<12}"), overlay_accent),
                    Span::styled(desc.to_string(), overlay_base),
                ])),
            }
        }

        if spacious {
            lines.push(Line::from(Span::styled("", overlay_base)));
        }
        // Nine columns overrun the 62-column overlay on one line, so they wrap.
        // The digit comes from the enumeration of `Column::ALL` itself, which is
        // the same index `keymap` feeds to `from_index` — deriving it any other
        // way lets the overlay advertise a key that sorts by something else.
        let numbered: Vec<String> = Column::ALL
            .iter()
            .enumerate()
            .map(|(i, c)| format!("{}·{}", i + 1, c.label(self.lang)))
            .collect();
        for (row, chunk) in numbered.chunks(3).enumerate() {
            lines.push(Line::from(vec![
                // Padded by display width: `{:<12}` counts characters, so the
                // CJK label would indent half as far as the English one.
                Span::styled(
                    format!("   {}", i18n::pad(if row == 0 { t.h_columns } else { "" }, 12)),
                    overlay_muted,
                ),
                Span::styled(chunk.join("  "), overlay_base),
            ]));
        }

        // Keep the modal visibly detached from the terminal edges. Previously
        // the full-height help touched the top and bottom borders, making it
        // read as a broken full-screen repaint rather than an overlay.
        let content_fits_inset =
            lines.len() as u16 + 2 <= area.height.saturating_sub(4) && area.width >= 28;
        let safe_area = if content_fits_inset {
            Rect {
                x: area.x.saturating_add(2),
                y: area.y.saturating_add(2),
                width: area.width.saturating_sub(4),
                height: area.height.saturating_sub(4),
            }
        } else {
            area
        };
        let rect = centered(safe_area, 62, lines.len() as u16 + 2);
        if rect.width < 24 || rect.height < 5 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    t.too_small_help,
                    theme.masthead(),
                )))
                .style(overlay_base),
                area,
            );
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(theme.accented())
            .title(Span::styled(t.help_title, theme.masthead()))
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
        assert!(matches!(
            translate(Mode::Browse, press('r')),
            Some(Message::RefreshStart)
        ));
    }
}
