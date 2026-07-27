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
        Mode::Category => category(key),
        Mode::Confirm => confirm(key),
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
        KeyCode::Char('L') => Some(Message::ToggleLanguage),
        KeyCode::Char('c') => Some(Message::CategoryFilterCycle),
        // Bare `Char('C')`, not `Char('c') + SHIFT`: crossterm already resolves
        // the shift into the capital, and matching on the modifier as well
        // makes the binding depend on how the terminal reports it.
        KeyCode::Char('C') => Some(Message::CategoryOpen),
        // Enter now acts on the row rather than describing it: in a list
        // interface it reads as "do the thing to this line", and opening a
        // record was always the secondary action. The record keeps `i` and
        // gains Tab.
        KeyCode::Char('i') | KeyCode::Tab => Some(Message::DetailOpen),
        KeyCode::Enter => Some(Message::ExecRequest),
        // L2: '1'..'9' pick the sort column. '0' is intentionally unbound.
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

/// Text entry for the category being assigned.
///
/// Up/Down are deliberately unbound, unlike `search`: that filter is
/// incremental so the user picks a row while typing, whereas this labels the
/// row that was already selected. Moving the cursor mid-edit would write the
/// name onto a different unit than the one the user pressed `C` on.
fn category(key: KeyEvent) -> Option<Message> {
    match key.code {
        KeyCode::Esc => Some(Message::CategoryCancel),
        KeyCode::Enter => Some(Message::CategoryCommit),
        KeyCode::Backspace => Some(Message::CategoryPop),
        KeyCode::Char(c) if !c.is_control() => Some(Message::CategoryPush(c)),
        _ => None,
    }
}

/// The launch confirmation, and the reason it is a mode rather than a prompt.
///
/// Only `y` proceeds; **every** other key cancels, including keys that mean
/// something in every other mode. A `j` held down a moment too long, or an
/// `r` aimed at the grid, must not be able to answer a question about running
/// a program. Defaulting the unrecognised key to "cancel" rather than to
/// "ignore" also means the mode can never be sat in by accident.
fn confirm(key: KeyEvent) -> Option<Message> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => Some(Message::ExecConfirm),
        _ => Some(Message::ExecCancel),
    }
}

fn detail(key: KeyEvent) -> Option<Message> {
    match key.code {
        // Enter is gone from here: it runs things now. Tab joins `i` so the
        // key that opens the record also closes it.
        KeyCode::Esc | KeyCode::Char('i') | KeyCode::Tab => Some(Message::DetailClose),
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
            Some(Message::SortBy(Column::Category))
        ));
        assert!(matches!(
            translate(Mode::Browse, press('8')),
            Some(Message::SortBy(Column::UiKind))
        ));
        assert!(matches!(
            translate(Mode::Browse, press('9')),
            Some(Message::SortBy(Column::LastUsed))
        ));
        assert_eq!(Column::from_index(9), None);
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
        for m in [
            Mode::Browse,
            Mode::Search,
            Mode::Category,
            Mode::Confirm,
            Mode::Detail,
            Mode::Help,
        ] {
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

    /// `c` and `C` act while browsing and are literal text while assigning —
    /// the same modal-safety property `q_quits_browsing_but_types_in_search`
    /// pins for the search box.
    #[test]
    fn category_keys_act_while_browsing_and_type_while_assigning() {
        assert!(matches!(
            translate(Mode::Browse, press('c')),
            Some(Message::CategoryFilterCycle)
        ));
        assert!(matches!(
            translate(Mode::Browse, press('C')),
            Some(Message::CategoryOpen)
        ));
        for ch in ['c', 'C'] {
            assert!(matches!(
                translate(Mode::Category, press(ch)),
                Some(Message::CategoryPush(got)) if got == ch
            ));
        }
    }

    /// The confirmation is a focus trap: `y` is the only key that proceeds,
    /// and keys that navigate or act everywhere else must cancel rather than
    /// leak through to the grid or satisfy the prompt.
    #[test]
    fn only_y_confirms_and_every_other_key_cancels() {
        for ch in ['y', 'Y'] {
            assert!(matches!(
                translate(Mode::Confirm, press(ch)),
                Some(Message::ExecConfirm)
            ));
        }
        for ch in ['n', 'N', 'j', 'k', 'q', 'r', 'C', 'c', '/', '1', ' '] {
            assert!(
                matches!(translate(Mode::Confirm, press(ch)), Some(Message::ExecCancel)),
                "{ch:?} must cancel, not proceed or fall through"
            );
        }
        for code in [KeyCode::Esc, KeyCode::Enter, KeyCode::Down, KeyCode::Tab] {
            let ev = KeyEvent::new(code, KeyModifiers::NONE);
            assert!(
                matches!(translate(Mode::Confirm, ev), Some(Message::ExecCancel)),
                "{code:?} must cancel"
            );
        }
    }

    /// Enter acts on the row; the record moved to `i` and Tab.
    #[test]
    fn enter_runs_and_the_record_belongs_to_i_and_tab() {
        assert!(matches!(
            translate(Mode::Browse, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Message::ExecRequest)
        ));
        assert!(matches!(
            translate(Mode::Browse, press('i')),
            Some(Message::DetailOpen)
        ));
        assert!(matches!(
            translate(Mode::Browse, KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            Some(Message::DetailOpen)
        ));
        // And Enter no longer closes the record, or it would run the unit the
        // moment the user tried to dismiss the panel.
        assert!(!matches!(
            translate(Mode::Detail, KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Message::DetailClose)
        ));
    }

    #[test]
    fn key_release_is_ignored() {
        let mut k = press('q');
        k.kind = KeyEventKind::Release;
        assert!(translate(Mode::Browse, k).is_none());
    }
}
