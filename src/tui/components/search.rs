//! Live filter input.
//!
//! Owns the query string and the matching rule. Filtering is incremental —
//! results narrow on every keystroke (tui-design §3, search & filtering).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::model::AppEntry;
use crate::tui::i18n::Lang;
use crate::tui::theme::Theme;

#[derive(Debug, Default)]
pub struct SearchBox {
    query: String,
    /// Lowercased query, kept alongside so matching does not re-allocate per row.
    needle: String,
}

impl SearchBox {
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn push(&mut self, c: char) {
        self.query.push(c);
        self.sync();
    }

    pub fn pop(&mut self) {
        self.query.pop();
        self.sync();
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.needle.clear();
    }

    /// Replace the query wholesale, as when a filter is restored across a
    /// suspend. Goes through `sync` like every other mutation so the
    /// lowercased needle can never drift out of step with the query.
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_string();
        self.sync();
    }

    fn sync(&mut self) {
        self.needle = self.query.to_lowercase();
    }

    /// Substring match over name, source, language and path.
    ///
    /// Deliberately not fuzzy: this list is dominated by exact tool names the
    /// user already knows, and fuzzy matching on ~1000 short names produces
    /// more noise than signal.
    pub fn matches(&self, e: &AppEntry) -> bool {
        if self.needle.is_empty() {
            return true;
        }
        let n = &self.needle;
        e.name.to_lowercase().contains(n)
            || e.source.to_string().to_lowercase().contains(n)
            || e
                .language
                .as_ref()
                .is_some_and(|l| l.to_string().to_lowercase().contains(n))
            || e.path.as_deref().is_some_and(|p| p.to_lowercase().contains(n))
    }

    /// Footer line while the search mode holds focus.
    pub fn render(&self, frame: &mut Frame, area: Rect, hits: usize, lang: Lang, theme: &Theme) {
        let t = lang.strings();
        let line = Line::from(vec![
            Span::styled(format!(" {} ", t.search_label), theme.masthead()),
            Span::styled(" /", theme.accented()),
            Span::styled(&self.query, theme.heading()),
            Span::styled("█", theme.accented()),
            Span::styled(format!("  {hits} {}", t.search_matches), theme.base()),
            Span::styled(format!("   {}", t.search_hint), theme.muted()),
        ]);
        frame.render_widget(Paragraph::new(line).style(theme.base()), area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Language, Source};

    fn entry() -> AppEntry {
        AppEntry {
            name: "RipGrep".into(),
            version: None,
            source: Source::Homebrew,
            language: Some(Language::Rust),
            install_date: None,
            last_used: None,
            usage_count: 0,
            path: Some("/opt/homebrew/bin/rg".into()),
            description: None,
            ui_kind: None,
            category: None,
        }
    }

    #[test]
    fn empty_query_matches_everything() {
        assert!(SearchBox::default().matches(&entry()));
    }

    #[test]
    fn matching_is_case_insensitive_across_fields() {
        let mut s = SearchBox::default();
        for q in ["ripgrep", "RIPGREP", "brew", "rust", "homebrew/bin"] {
            s.clear();
            q.chars().for_each(|c| s.push(c));
            assert!(s.matches(&entry()), "query {q:?} should match");
        }
    }

    #[test]
    fn non_matching_query_rejects() {
        let mut s = SearchBox::default();
        "zzzz".chars().for_each(|c| s.push(c));
        assert!(!s.matches(&entry()));
    }

    /// pop() must keep the lowercase needle in sync, or the filter goes stale.
    #[test]
    fn pop_resyncs_needle() {
        let mut s = SearchBox::default();
        "RGx".chars().for_each(|c| s.push(c));
        assert!(!s.matches(&entry()));
        s.pop();
        assert!(s.matches(&entry()), "after pop, 'RG' should match again");
    }
}
