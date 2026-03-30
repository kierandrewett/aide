use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

pub enum Action {
    // Aide-reserved actions
    NextTab,
    PrevTab,
    NewInstance,
    ProjectPicker,
    CloseInstance,
    TogglePanel,
    Exit,
    // Picker-only actions (only used when picker/dialog is open)
    ScrollUp,
    ScrollDown,
    Confirm,
    Cancel,
    PickerChar(char),
    PickerBackspace,
    // Forward to tmux
    ForwardKey(KeyEvent),
    None,
}

/// Poll for a key event with the given timeout. Returns an Action.
pub fn poll_action(timeout: Duration, picker_mode: bool) -> Action {
    if event::poll(timeout).unwrap_or(false) {
        if let Ok(Event::Key(key)) = event::read() {
            return map_key(key, picker_mode);
        }
    }
    Action::None
}

fn map_key(key: KeyEvent, picker_mode: bool) -> Action {
    // Aide-reserved Ctrl bindings — always intercepted
    match key {
        KeyEvent {
            code: KeyCode::Char('t'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => return Action::NewInstance,
        KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => return Action::ProjectPicker,
        KeyEvent {
            code: KeyCode::Char('w'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => return Action::CloseInstance,
        KeyEvent {
            code: KeyCode::Char('g'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => return Action::TogglePanel,
        KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => return Action::Exit,
        // Tab switching
        KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            ..
        } => return Action::NextTab,
        KeyEvent {
            code: KeyCode::BackTab,
            ..
        } => return Action::PrevTab,
        _ => {}
    }

    // When picker/dialog is open, handle input for the picker
    if picker_mode {
        return match key {
            KeyEvent {
                code: KeyCode::Enter,
                ..
            } => Action::Confirm,
            KeyEvent {
                code: KeyCode::Esc, ..
            } => Action::Cancel,
            KeyEvent {
                code: KeyCode::Up, ..
            } => Action::ScrollUp,
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => Action::ScrollDown,
            KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE | KeyModifiers::SHIFT,
                ..
            } => Action::PickerChar(c),
            KeyEvent {
                code: KeyCode::Backspace,
                ..
            } => Action::PickerBackspace,
            _ => Action::None,
        };
    }

    // Everything else gets forwarded to tmux
    Action::ForwardKey(key)
}
