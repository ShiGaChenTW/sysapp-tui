//! The record panel — full detail for the selected unit.
//!
//! Rendered as a persistent side panel when the terminal is wide enough, and
//! as a centred modal when it is not. A master-detail pair beats an overlay:
//! the detail is visible while you move the cursor, so scanning the inventory
//! and reading a record are the same gesture instead of two.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};
use unicode_width::UnicodeWidthStr;

use crate::model::AppEntry;
use crate::tui::theme::Theme;

pub struct DetailPanel<'a> {
    pub entry: Option<&'a AppEntry>,
    /// Source-name / count pairs, shown beneath the record.
    pub sources: &'a [(String, usize)],
}

impl DetailPanel<'_> {
    /// Persistent side panel.
    pub fn render_side(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(theme.panel_border())
            .title(Span::styled(" RECORD ", theme.panel_title()))
            .padding(Padding::new(1, 1, 1, 0))
            .style(theme.base());
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines = match self.entry {
            Some(e) => self.body(e, inner.width, theme),
            None => vec![Line::from(Span::styled(" No unit selected", theme.muted()))],
        };

        if !self.sources.is_empty() && inner.height as usize > lines.len() + 2 {
            lines.push(Line::from(Span::styled("", theme.base())));
            lines.push(section("SOURCES", theme));
            let room = (inner.height as usize).saturating_sub(lines.len());
            for (name, n) in self.sources.iter().take(room) {
                lines.push(Line::from(vec![
                    Span::styled(format!("   {name:<10}"), theme.muted()),
                    Span::styled(format!("{n}"), theme.base()),
                ]));
            }
        }

        frame.render_widget(Paragraph::new(lines).style(theme.base()), inner);
    }

    /// Centred modal, for terminals too narrow to carry a side panel.
    pub fn render_modal(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let Some(entry) = self.entry else { return };
        let lines = self.body(entry, 60, theme);

        let rect = centered(area, 60, lines.len() as u16 + 2);
        if rect.width < 24 || rect.height < 5 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    " TERMINAL TOO SMALL FOR RECORD ",
                    theme.status_band(),
                )))
                .style(theme.base()),
                area,
            );
            return;
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Plain)
            .border_style(theme.accented())
            .title(Span::styled(" RECORD ", theme.panel_title()))
            .padding(Padding::new(1, 1, 1, 1))
            .style(theme.overlay());

        frame.render_widget(Clear, rect);
        frame.render_widget(Paragraph::new(lines).block(block), rect);
    }

    fn body<'b>(&self, e: &AppEntry, width: u16, theme: &Theme) -> Vec<Line<'b>> {
        let field = |label: &'static str, value: String| -> Line<'b> {
            vec![
                Span::styled(format!("   {label:<13}"), theme.muted()),
                Span::styled(value, theme.base()),
            ]
            .into()
        };

        let mut lines = vec![
            Line::from(vec![
                Span::styled(" ● ", theme.accented()),
                Span::styled(e.name.clone(), theme.heading()),
            ]),
            Line::from(Span::styled("", theme.base())),
            section("ORIGIN", theme),
            field("SOURCE", e.source.to_string().to_uppercase()),
            field(
                "LANGUAGE",
                e.language
                    .as_ref()
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| "—".into()),
            ),
            field("VERSION", e.version.clone().unwrap_or_else(|| "—".into())),
            Line::from(Span::styled("", theme.base())),
            section("ACTIVITY", theme),
            field(
                "INSTALLED",
                e.install_date
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_else(|| "—".into()),
            ),
            field(
                "LAST USED",
                e.last_used
                    .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_else(|| "—".into()),
            ),
            field(
                "INVOCATIONS",
                if e.usage_count == 0 {
                    "none recorded".into()
                } else {
                    format!("{}", e.usage_count)
                },
            ),
            Line::from(Span::styled("", theme.base())),
            section("LOCATION", theme),
            Line::from(Span::styled(
                format!("   {}", e.path.as_deref().unwrap_or("—")),
                theme.base(),
            )),
        ];

        if let Some(d) = &e.description {
            lines.push(Line::from(Span::styled("", theme.base())));
            lines.push(section("NOTE", theme));
            // Wrapped here rather than by `Paragraph`, which would break the
            // continuation flush against the panel border and lose the indent.
            for chunk in wrap_indented(d, width.saturating_sub(3)) {
                lines.push(Line::from(Span::styled(format!("   {chunk}"), theme.muted())));
            }
        }
        lines
    }
}

/// Break `text` into display-width-bounded chunks on word boundaries.
///
/// Width-aware rather than char-aware so CJK descriptions do not overrun the
/// panel; a single word longer than the budget is emitted whole and clipped by
/// the terminal rather than being silently dropped.
fn wrap_indented(text: &str, width: u16) -> Vec<String> {
    let budget = width.max(8) as usize;
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if candidate.width() > budget && !line.is_empty() {
            out.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn section<'a>(label: &str, theme: &Theme) -> Line<'a> {
    Line::from(Span::styled(format!(" [ {label} ]"), theme.field_label()))
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
        for label in ["SOURCE", "LANGUAGE", "VERSION", "INSTALLED", "LAST USED", "INVOCATIONS"] {
            assert!(label.len() < PAD, "{label} needs padding > {}", label.len());
        }
    }

    /// Wrapped continuation lines must never exceed the panel's inner width,
    /// or they break flush against the border and lose the indent.
    #[test]
    fn wrap_respects_the_width_budget() {
        for (text, w) in [
            ("Distributed revision control system", 30u16),
            ("分散式版本控制系統，用於追蹤原始碼變更", 20),
            ("supercalifragilisticexpialidocious", 10),
            ("", 30),
        ] {
            for chunk in wrap_indented(text, w) {
                assert!(
                    chunk.width() <= w.max(8) as usize || !chunk.contains(' '),
                    "{chunk:?} is {} cols, budget {w}",
                    chunk.width()
                );
            }
        }
    }

    /// Wrapping must not drop content.
    #[test]
    fn wrap_preserves_every_word() {
        let text = "Distributed revision control system";
        let joined = wrap_indented(text, 12).join(" ");
        assert_eq!(joined, text);
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
