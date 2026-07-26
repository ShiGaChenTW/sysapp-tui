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

pub struct StatusBar {
    pub mode: Mode,
    pub position: Option<usize>,
    pub total: usize,
}

impl StatusBar {
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let position = match self.position {
            Some(i) if self.total > 0 => format!(" {}/{} ", i + 1, self.total),
            _ => " —/0 ".to_string(),
        };

        let left = Line::from(vec![
            Span::styled(format!(" {} ", self.mode.label()), theme.status_band()),
            Span::styled(position, theme.status_band_idle()),
            Span::styled(
                "  j/k MOVE · / FILTER · Enter RECORD · ? KEYS · q QUIT",
                theme.muted(),
            ),
        ]);

        frame.render_widget(Paragraph::new(left).style(theme.base()), area);
    }
}
