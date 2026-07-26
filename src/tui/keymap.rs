//! Key → Message translation, layered per tui-design §3.
//!
//! - **L0 universal**: arrows, Enter, Esc, q — always advertised in the footer
//! - **L1 vim**: hjkl, /, ?, g/G — also advertised
//! - **L2 actions**: digits select the sort column — shown in the `?` overlay
//!
//! `Ctrl+C` is deliberately *not* rebound; it belongs to the terminal. It is
//! honoured here only as an additional quit path, matching the pre-existing
//! behaviour users already rely on.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use super::message::{Column, Message, Mode};

/// Translate a key press into a message, or `None` if the key is inert
/// in the current mode.
pub fn translate(mode: Mode, key: KeyEvent) -> Option<Message> {
    // Key *releases* and repeats arrive on some platforms; only act on press.
    if key.kind != KeyEventKind::Press {
        return None;
    }

    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Some(Message::Quit);
    }

    match mode {
        Mode::Browse => browse(key),
        Mode::Search => search(key),
        Mode::Detail => detail(key),
        Mode::Help => help(key),
    }
}

fn browse(key: KeyEvent) -> Option<Message> {
    // A page is a conservative fixed stride; the viewport clamps it anyway.
    const PAGE: i32 = 10;

    match key.code {
        KeyCode::Char('q') => Some(Message::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Message::Move(1)),
        KeyCode::Char('k') | KeyCode::Up => Some(Message::Move(-1)),
        KeyCode::PageDown | KeyCode::Char('d') => Some(Message::Move(PAGE)),
        KeyCode::PageUp | KeyCode::Char('u') => Some(Message::Move(-PAGE)),
        KeyCode::Char('g') | KeyCode::Home => Some(Message::JumpTop),
        KeyCode::Char('G') | KeyCode::End => Some(Message::JumpBottom),
        KeyCode::Char('/') => Some(Message::SearchOpen),
        KeyCode::Char('?') => Some(Message::HelpToggle),
        KeyCode::Char('r') => Some(Message::RefreshStart),
        KeyCode::Char('p') => Some(Message::ToggleNoise),
        KeyCode::Char('s') => Some(Message::ToggleIdleOnly),
        KeyCode::Enter | KeyCode::Char('i') => Some(Message::DetailOpen),
        // L2: '1'..'7' pick the sort column. '0' is intentionally unbound.
        KeyCode::Char(c @ '1'..='9') => {
            Column::from_index(c as usize - '1' as usize).map(Message::SortBy)
        }
        _ => None,
    }
}

fn search(key: KeyEvent) -> Option<Message> {
    match key.code {
        KeyCode::Esc => Some(Message::SearchCancel),
        KeyCode::Enter => Some(Message::SearchCommit),
        KeyCode::Backspace => Some(Message::SearchPop),
        KeyCode::Down => Some(Message::Move(1)),
        KeyCode::Up => Some(Message::Move(-1)),
        // Control chars would otherwise land in the query as garbage.
        KeyCode::Char(c) if !c.is_control() => Some(Message::SearchPush(c)),
        _ => None,
    }
}

fn detail(key: KeyEvent) -> Option<Message> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('i') | KeyCode::Enter => Some(Message::DetailClose),
        KeyCode::Char('q') => Some(Message::Quit),
        KeyCode::Char('?') => Some(Message::HelpToggle),
        KeyCode::Char('r') => Some(Message::RefreshStart),
        _ => None,
    }
}

fn help(key: KeyEvent) -> Option<Message> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Enter => Some(Message::HelpToggle),
        KeyCode::Char('q') => Some(Message::Quit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    #[test]
    fn digits_map_to_columns_in_range_only() {
        assert!(matches!(
            translate(Mode::Browse, press('1')),
            Some(Message::SortBy(Column::Name))
        ));
        assert!(matches!(
            translate(Mode::Browse, press('7')),
            Some(Message::SortBy(Column::Path))
        ));
        // Only seven columns exist — '8' must not panic or wrap around.
        assert!(translate(Mode::Browse, press('8')).is_none());
    }

    /// The same physical key means different things per mode. This is the
    /// property that makes modal input safe.
    #[test]
    fn q_quits_browsing_but_types_in_search() {
        assert!(matches!(
            translate(Mode::Browse, press('q')),
            Some(Message::Quit)
        ));
        assert!(matches!(
            translate(Mode::Search, press('q')),
            Some(Message::SearchPush('q'))
        ));
    }

    #[test]
    fn ctrl_c_always_quits() {
        let k = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        for m in [Mode::Browse, Mode::Search, Mode::Detail, Mode::Help] {
            assert!(matches!(translate(m, k), Some(Message::Quit)), "mode {m:?}");
        }
    }

    #[test]
    fn r_refreshes_while_browsing_but_types_in_search() {
        assert!(matches!(
            translate(Mode::Browse, press('r')),
            Some(Message::RefreshStart)
        ));
        assert!(matches!(
            translate(Mode::Search, press('r')),
            Some(Message::SearchPush('r'))
        ));
    }

    #[test]
    fn key_release_is_ignored() {
        let mut k = press('q');
        k.kind = KeyEventKind::Release;
        assert!(translate(Mode::Browse, k).is_none());
    }
}
