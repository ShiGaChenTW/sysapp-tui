//! Full record for one unit, as a centred modal overlay.
//!
//! Modal overlays are focus traps: while this is up, the grid receives no
//! keys. `Clear` is rendered first so the grid does not bleed through.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use crate::model::AppEntry;
use crate::tui::theme::Theme;

pub struct DetailPanel<'a> {
    pub entry: &'a AppEntry,
}

impl DetailPanel<'_> {
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let e = self.entry;

        let field = |label: &'static str, value: String| -> Line<'static> {
            Line::from(vec![
                // Width must exceed the longest label ("INVOCATIONS", 11) or
                // the value collides with it.
                Span::styled(format!(" {label:<13}"), theme.muted()),
                Span::styled(value, theme.base()),
            ])
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled(" >>> ", theme.accented()),
                Span::styled(e.name.to_uppercase(), theme.heading()),
            ]),
            Line::from(Span::styled("", theme.base())),
            field("SOURCE", e.source.to_string().to_uppercase()),
            field(
                "LANGUAGE",
                e.language
                    .as_ref()
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "—".into()),
            ),
            field("VERSION", e.version.clone().unwrap_or_else(|| "—".into())),
            field(
                "INSTALLED",
                e.install_date
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "—".into()),
            ),
            field(
                "LAST USED",
                e.last_used
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "—".into()),
            ),
            field("INVOCATIONS", format!("{}", e.usage_count)),
            field("PATH", e.path.clone().unwrap_or_else(|| "—".into())),
        ];

        if let Some(d) = &e.description {
            lines.push(field("NOTE", d.clone()));
        }

        lines.push(Line::from(Span::styled("", theme.base())));
        lines.push(Line::from(Span::styled(
            " ESC / i  RETURN     q  QUIT",
            theme.muted(),
        )));

        let inner_h = lines.len() as u16;
        let rect = centered(area, 68, inner_h + 2);

        // If the terminal cannot fit the overlay, say so instead of drawing a
        // clipped box that looks like a rendering bug.
        if rect.width < 24 || rect.height < 5 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " TERMINAL TOO SMALL FOR DETAIL ",
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
            .title(Span::styled(" [ UNIT RECORD ] ", theme.status_band()))
            .style(theme.overlay());

        frame.render_widget(Clear, rect);
        frame.render_widget(Paragraph::new(lines).block(block), rect);
    }
}

/// Centre a `w`x`h` rect inside `area`, shrinking to fit rather than
/// overflowing when the terminal is small.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let width = w.min(area.width);
    let height = h.min(area.height);
    Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The value column is padded past the longest label; if a longer label is
    /// ever added, this fails instead of rendering "INVOCATIONS96".
    #[test]
    fn label_padding_clears_the_longest_label() {
        const PAD: usize = 13;
        for label in ["SOURCE", "LANGUAGE", "VERSION", "INSTALLED", "LAST USED",
                      "INVOCATIONS", "PATH", "NOTE"] {
            assert!(label.len() < PAD, "{label} needs padding > {}", label.len());
        }
    }

    /// The overlay must always stay inside its parent — a rect that escapes
    /// `area` panics inside ratatui's buffer.
    #[test]
    fn centered_never_escapes_parent() {
        let parent = Rect { x: 3, y: 2, width: 40, height: 12 };
        for (w, h) in [(10u16, 4u16), (68, 20), (1, 1), (999, 999)] {
            let r = centered(parent, w, h);
            assert!(r.x >= parent.x && r.y >= parent.y, "{r:?}");
            assert!(r.x + r.width <= parent.x + parent.width, "{r:?}");
            assert!(r.y + r.height <= parent.y + parent.height, "{r:?}");
        }
    }
}
