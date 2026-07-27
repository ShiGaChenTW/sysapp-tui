//! Masthead: a single solid band across the top.
//!
//! One filled bar carrying identity on the left and headline figures on the
//! right. Everything else — filters, sort state, source density — moved into
//! the panels below, where each sits beside the data it describes. A header
//! that restates the whole application state competes with the grid.

use chrono::{DateTime, Local};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::cache;
use crate::tui::i18n::Lang;
use crate::tui::theme::Theme;

/// Two blank rows inside the band, above and below the text, so the title
/// sits in a field rather than being crushed against the panels.
const BAND_PAD: u16 = 2;

/// The filled band (padding + text + padding) plus one blank row separating
/// it from the panels below.
pub const HEIGHT: u16 = BAND_PAD * 2 + 1 + 1;

pub struct HeaderBar {
    pub total: usize,
    pub generated_at: Option<DateTime<Local>>,
    pub lang: Lang,
}

impl HeaderBar {
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        let band_height = (BAND_PAD * 2 + 1).min(area.height);
        let band = Rect { height: band_height, ..area };

        // Fill the whole band before writing into it. The Paragraphs below
        // paint only the glyphs they carry, and an unpainted gap shows the
        // user's terminal background straight through what should be a solid
        // field — including the padding rows, which are nothing but fill.
        frame.render_widget(Paragraph::new("").style(theme.masthead()), band);

        // The text row sits centred in the field.
        let text_row = Rect {
            y: band.y + band_height.saturating_sub(1) / 2,
            height: 1,
            ..band
        };

        let s = self.lang.strings();
        let left = Line::from(vec![
            Span::styled("  SYSAPP·TUI ®", theme.masthead()),
            Span::styled("   ", theme.masthead()),
            Span::styled(s.app_subtitle, theme.masthead()),
        ]);

        // Staleness is never hidden: a restored snapshot says how old it is.
        let freshness = match self.generated_at {
            Some(at) => cache::age_label(at, self.lang),
            None => s.live.to_string(),
        };
        let right = Line::from(Span::styled(
            format!(
                "{} {} · {} · {} {}  ",
                self.total,
                s.units,
                freshness,
                s.rev,
                env!("CARGO_PKG_VERSION")
            ),
            theme.masthead(),
        ));

        frame.render_widget(Paragraph::new(left).style(theme.masthead()), text_row);
        frame.render_widget(
            Paragraph::new(right).style(theme.masthead()).right_aligned(),
            text_row,
        );
    }
}
