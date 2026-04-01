use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind,
};
use std::time::Duration;

pub enum Action {
    // Aide-reserved actions
    NextTab,
    PrevTab,
    NewInstance,
    CommandPalette,
    CloseInstance,
    TogglePanel,
    ToggleFileBrowser,
    ToggleFileView,
    Exit,
    // Picker/dialog actions
    ScrollUp,
    ScrollDown,
    Confirm,
    Cancel,
    PickerChar(char),
    PickerBackspace,
    // Forward to PTY
    ForwardChars(String),
    ForwardSpecial(String),
    ForwardCtrl(char),
    EscapeKey,
    MouseClick(u16, u16),
    None,
}

/// Drain all pending events, returning a list of actions.
pub fn drain_actions(timeout: Duration, picker_mode: bool) -> Vec<Action> {
    let mut actions = Vec::new();

    if !event::poll(timeout).unwrap_or(false) {
        return actions;
    }

    loop {
        if !event::poll(Duration::ZERO).unwrap_or(false) {
            break;
        }
        match event::read() {
            Ok(Event::Key(key)) => {
                let action = map_key(key, picker_mode);
                match &action {
                    Action::None => {}
                    _ => actions.push(action),
                }
            }
            Ok(Event::Mouse(mouse)) => {
                if let Some(action) = map_mouse(mouse) {
                    actions.push(action);
                }
            }
            Ok(Event::Paste(text)) => {
                // Bracketed paste — forward as raw chars
                if !text.is_empty() {
                    actions.push(Action::ForwardChars(text));
                }
            }
            _ => break,
        }
    }

    coalesce_chars(actions)
}

fn coalesce_chars(actions: Vec<Action>) -> Vec<Action> {
    let mut result: Vec<Action> = Vec::with_capacity(actions.len());
    let mut char_buf = String::new();

    for action in actions {
        match action {
            Action::ForwardChars(s) => char_buf.push_str(&s),
            other => {
                if !char_buf.is_empty() {
                    result.push(Action::ForwardChars(std::mem::take(&mut char_buf)));
                }
                result.push(other);
            }
        }
    }
    if !char_buf.is_empty() {
        result.push(Action::ForwardChars(char_buf));
    }
    result
}

fn map_key(key: KeyEvent, picker_mode: bool) -> Action {
    if key.kind != KeyEventKind::Press {
        return Action::None;
    }

    // Aide-reserved bindings
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
        } => return Action::CommandPalette,
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
            code: KeyCode::Char('b'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => return Action::ToggleFileBrowser,
        KeyEvent {
            code: KeyCode::Char('f'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => return Action::ToggleFileView,
        KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::CONTROL,
            ..
        } => return Action::Exit,
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

    // Forward to PTY
    match (&key.code, key.modifiers) {
        (KeyCode::Char(c), mods) if mods.contains(KeyModifiers::CONTROL) => {
            Action::ForwardCtrl(*c)
        }
        // Shift+Enter sends newline
        (KeyCode::Enter, mods) if mods.contains(KeyModifiers::SHIFT) => {
            Action::ForwardSpecial("S-Enter".into())
        }
        (KeyCode::Char(c), _) => Action::ForwardChars(c.to_string()),
        (KeyCode::Enter, _) => Action::ForwardSpecial("Enter".into()),
        (KeyCode::Backspace, _) => Action::ForwardSpecial("BSpace".into()),
        (KeyCode::Esc, _) => Action::EscapeKey,
        (KeyCode::Up, _) => Action::ForwardSpecial("Up".into()),
        (KeyCode::Down, _) => Action::ForwardSpecial("Down".into()),
        (KeyCode::Left, _) => Action::ForwardSpecial("Left".into()),
        (KeyCode::Right, _) => Action::ForwardSpecial("Right".into()),
        (KeyCode::Home, _) => Action::ForwardSpecial("Home".into()),
        (KeyCode::End, _) => Action::ForwardSpecial("End".into()),
        (KeyCode::PageUp, _) => Action::ScrollUp,
        (KeyCode::PageDown, _) => Action::ScrollDown,
        (KeyCode::Delete, _) => Action::ForwardSpecial("DC".into()),
        (KeyCode::Insert, _) => Action::ForwardSpecial("IC".into()),
        (KeyCode::F(n), _) => Action::ForwardSpecial(format!("F{}", n)),
        _ => Action::None,
    }
}

fn map_mouse(mouse: MouseEvent) -> Option<Action> {
    match mouse.kind {
        MouseEventKind::ScrollUp => Some(Action::ScrollUp),
        MouseEventKind::ScrollDown => Some(Action::ScrollDown),
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            Some(Action::MouseClick(mouse.column, mouse.row))
        }
        _ => None,
    }
}
