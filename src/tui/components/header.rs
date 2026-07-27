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
use crate::tui::theme::Theme;

/// Band plus the blank line separating it from the panels.
pub const HEIGHT: u16 = 2;

pub struct HeaderBar<'a> {
    pub total: usize,
    pub generated_at: Option<DateTime<Local>>,
    pub title: &'a str,
}

impl HeaderBar<'_> {
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if area.height == 0 {
            return;
        }
        let band = Rect { height: 1, ..area };

        // Fill the band before writing into it. The Paragraphs below paint only
        // the glyphs they carry, and an unpainted gap shows the user's terminal
        // background straight through the middle of what should be a solid bar.
        frame.render_widget(Paragraph::new("").style(theme.status_band()), band);

        let left = Line::from(vec![
            Span::styled(" SYSAPP·TUI ®", theme.status_band()),
            Span::styled("   ", theme.status_band()),
            Span::styled(self.title, theme.status_band()),
        ]);

        // Staleness is never hidden: a restored snapshot says how old it is.
        let freshness = match self.generated_at {
            Some(at) => cache::age_label(at),
            None => "LIVE".to_string(),
        };
        let right = Line::from(Span::styled(
            format!(
                "{} UNITS · {} · REV {} ",
                self.total,
                freshness,
                env!("CARGO_PKG_VERSION")
            ),
            theme.status_band(),
        ));

        frame.render_widget(Paragraph::new(left).style(theme.status_band()), band);
        frame.render_widget(
            Paragraph::new(right)
                .style(theme.status_band())
                .right_aligned(),
            band,
        );
    }
}
