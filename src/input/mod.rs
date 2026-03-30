use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

pub enum Action {
    NextTab,
    PrevTab,
    NewInstance,
    ProjectPicker,
    CloseInstance,
    TogglePanel,
    Exit,
    ScrollUp,
    ScrollDown,
    Confirm,
    Cancel,
    Char(char),
    Backspace,
    None,
}

/// Poll for a key event with the given timeout. Returns an Action.
pub fn poll_action(timeout: Duration) -> Action {
    if event::poll(timeout).unwrap_or(false) {
        if let Ok(Event::Key(key)) = event::read() {
            return map_key(key);
        }
    }
    Action::None
}

fn map_key(key: KeyEvent) -> Action {
    match key {
        // Ctrl bindings
        KeyEvent {
            code: KeyCode::Char('t'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Action::NewInstance,
        KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Action::ProjectPicker,
        KeyEvent {
            code: KeyCode::Char('w'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Action::CloseInstance,
        KeyEvent {
            code: KeyCode::Char('g'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Action::TogglePanel,
        KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Action::Exit,
        KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => Action::Exit,

        // Tab navigation
        KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            ..
        } => Action::NextTab,
        KeyEvent {
            code: KeyCode::BackTab,
            ..
        } => Action::PrevTab,

        // Scrolling
        KeyEvent {
            code: KeyCode::Up, ..
        } => Action::ScrollUp,
        KeyEvent {
            code: KeyCode::Down,
            ..
        } => Action::ScrollDown,
        KeyEvent {
            code: KeyCode::PageUp,
            ..
        } => Action::ScrollUp,
        KeyEvent {
            code: KeyCode::PageDown,
            ..
        } => Action::ScrollDown,

        // Text input for picker
        KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            ..
        } => Action::Confirm,
        KeyEvent {
            code: KeyCode::Esc, ..
        } => Action::Cancel,
        KeyEvent {
            code: KeyCode::Char(c),
            modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
            ..
        } => Action::Char(c),
        KeyEvent {
            code: KeyCode::Backspace,
            ..
        } => Action::Backspace,

        _ => Action::None,
    }
}
