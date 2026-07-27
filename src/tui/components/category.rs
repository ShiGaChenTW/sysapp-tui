//! Category assignment input.
//!
//! This mirrors the search footer rather than opening another modal because
//! the selected row already says which unit is being labelled; spending the
//! whole viewport on a one-line edit would hide the list the user is curating.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::tui::i18n::Lang;
use crate::tui::theme::Theme;

#[derive(Debug, Default)]
pub struct CategoryInput {
    input: String,
}

impl CategoryInput {
    pub fn input(&self) -> &str {
        &self.input
    }

    pub fn push(&mut self, c: char) {
        self.input.push(c);
    }

    pub fn pop(&mut self) {
        self.input.pop();
    }

    pub fn clear(&mut self) {
        self.input.clear();
    }

    /// Footer line while category assignment holds focus.
    pub fn render(&self, frame: &mut Frame, area: Rect, lang: Lang, theme: &Theme) {
        let t = lang.strings();
        let line = Line::from(vec![
            Span::styled(format!(" {} ", t.category_label), theme.masthead()),
            Span::styled(" >", theme.accented()),
            Span::styled(self.input(), theme.heading()),
            Span::styled("█", theme.accented()),
            Span::styled(format!("   {}", t.category_hint), theme.muted()),
        ]);
        frame.render_widget(Paragraph::new(line).style(theme.base()), area);
    }
}

