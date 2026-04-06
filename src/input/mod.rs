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
    CommandPaletteReverse,
    CloseInstance,
    TogglePanel,
    ToggleFileBrowser,
    ToggleFileView,
    Exit,
    // Picker/dialog actions
    ScrollUp(u16, u16),    // (x, y) mouse position
    ScrollDown(u16, u16),  // (x, y) mouse position
    ScrollLeft(u16, u16),  // (x, y) mouse position
    ScrollRight(u16, u16), // (x, y) mouse position
    ScrollToTop,
    ScrollToBottom,
    Confirm,
    Cancel,
    PickerChar(char),
    PickerBackspace,
    // Forward to PTY
    ForwardChars(String),
    ForwardSpecial(String),
    ForwardCtrl(char),
    /// Bracketed paste — large text that should be wrapped in paste brackets
    Paste(String),
    CopySelection,
    EscapeKey,
    MouseClick(u16, u16),
    MouseDrag(u16, u16),
    MouseRelease(u16, u16),
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
                // Bracketed paste — send as paste action for proper handling
                if !text.is_empty() {
                    actions.push(Action::Paste(text));
                }
            }
            Ok(Event::Resize(..)) => {
                // Terminal resized — continue draining. The main loop
                // detects the new dimensions via ratatui layout on the
                // next draw and resizes the PTY accordingly.
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
            modifiers,
            ..
        } if modifiers.contains(KeyModifiers::CONTROL)
            && modifiers.contains(KeyModifiers::SHIFT) =>
        {
            return Action::CommandPaletteReverse
        }
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
            } => Action::ScrollUp(0, 0),
            KeyEvent {
                code: KeyCode::Down,
                ..
            } => Action::ScrollDown(0, 0),
            KeyEvent {
                code: KeyCode::Left,
                ..
            } => Action::ScrollLeft(0, 0),
            KeyEvent {
                code: KeyCode::Right,
                ..
            } => Action::ScrollRight(0, 0),
            KeyEvent {
                code: KeyCode::Tab, ..
            } => Action::ScrollDown(0, 0),
            KeyEvent {
                code: KeyCode::BackTab,
                ..
            } => Action::ScrollUp(0, 0),
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
        // Ctrl+Shift+C = copy selection (crossterm may report 'c' or 'C')
        (KeyCode::Char('c' | 'C'), mods)
            if mods.contains(KeyModifiers::CONTROL) && mods.contains(KeyModifiers::SHIFT) =>
        {
            Action::CopySelection
        }
        (KeyCode::Char(c), mods) if mods.contains(KeyModifiers::CONTROL) => Action::ForwardCtrl(*c),
        (KeyCode::Char(c), mods) if mods.contains(KeyModifiers::ALT) => {
            Action::ForwardSpecial(format!("A-{}", c))
        }
        // Shift+Enter sends newline
        (KeyCode::Enter, mods) if mods.contains(KeyModifiers::SHIFT) => {
            Action::ForwardSpecial("S-Enter".into())
        }
        (KeyCode::Char(c), _) => Action::ForwardChars(c.to_string()),
        (KeyCode::Enter, _) => Action::ForwardSpecial("Enter".into()),
        (KeyCode::Backspace, mods) if mods.contains(KeyModifiers::CONTROL) => {
            Action::ForwardSpecial("C-BSpace".into())
        }
        (KeyCode::Backspace, mods) if mods.contains(KeyModifiers::ALT) => {
            Action::ForwardSpecial("A-BSpace".into())
        }
        (KeyCode::Backspace, _) => Action::ForwardSpecial("BSpace".into()),
        (KeyCode::Esc, _) => Action::EscapeKey,
        // Ctrl+Arrow
        (KeyCode::Up, mods) if mods.contains(KeyModifiers::CONTROL) => {
            Action::ForwardSpecial("C-Up".into())
        }
        (KeyCode::Down, mods) if mods.contains(KeyModifiers::CONTROL) => {
            Action::ForwardSpecial("C-Down".into())
        }
        (KeyCode::Left, mods) if mods.contains(KeyModifiers::CONTROL) => {
            Action::ForwardSpecial("C-Left".into())
        }
        (KeyCode::Right, mods) if mods.contains(KeyModifiers::CONTROL) => {
            Action::ForwardSpecial("C-Right".into())
        }
        // Shift+Arrow
        (KeyCode::Up, mods) if mods.contains(KeyModifiers::SHIFT) => {
            Action::ForwardSpecial("S-Up".into())
        }
        (KeyCode::Down, mods) if mods.contains(KeyModifiers::SHIFT) => {
            Action::ForwardSpecial("S-Down".into())
        }
        (KeyCode::Left, mods) if mods.contains(KeyModifiers::SHIFT) => {
            Action::ForwardSpecial("S-Left".into())
        }
        (KeyCode::Right, mods) if mods.contains(KeyModifiers::SHIFT) => {
            Action::ForwardSpecial("S-Right".into())
        }
        // Alt+Arrow
        (KeyCode::Up, mods) if mods.contains(KeyModifiers::ALT) => {
            Action::ForwardSpecial("A-Up".into())
        }
        (KeyCode::Down, mods) if mods.contains(KeyModifiers::ALT) => {
            Action::ForwardSpecial("A-Down".into())
        }
        (KeyCode::Left, mods) if mods.contains(KeyModifiers::ALT) => {
            Action::ForwardSpecial("A-Left".into())
        }
        (KeyCode::Right, mods) if mods.contains(KeyModifiers::ALT) => {
            Action::ForwardSpecial("A-Right".into())
        }
        (KeyCode::Up, _) => Action::ForwardSpecial("Up".into()),
        (KeyCode::Down, _) => Action::ForwardSpecial("Down".into()),
        (KeyCode::Left, _) => Action::ForwardSpecial("Left".into()),
        (KeyCode::Right, _) => Action::ForwardSpecial("Right".into()),
        // Home / End with modifiers
        (KeyCode::Home, mods) if mods.contains(KeyModifiers::CONTROL) => {
            Action::ForwardSpecial("C-Home".into())
        }
        (KeyCode::End, mods) if mods.contains(KeyModifiers::CONTROL) => {
            Action::ForwardSpecial("C-End".into())
        }
        (KeyCode::Home, mods) if mods.contains(KeyModifiers::SHIFT) => {
            Action::ForwardSpecial("S-Home".into())
        }
        (KeyCode::End, mods) if mods.contains(KeyModifiers::SHIFT) => {
            Action::ForwardSpecial("S-End".into())
        }
        (KeyCode::Home, _) => Action::ForwardSpecial("Home".into()),
        (KeyCode::End, _) => Action::ForwardSpecial("End".into()),
        (KeyCode::PageUp, mods) if mods.contains(KeyModifiers::CONTROL) => Action::ScrollToTop,
        (KeyCode::PageDown, mods) if mods.contains(KeyModifiers::CONTROL) => Action::ScrollToBottom,
        (KeyCode::PageUp, _) => Action::ScrollUp(0, 0),
        (KeyCode::PageDown, _) => Action::ScrollDown(0, 0),
        // Delete / Insert with modifiers
        (KeyCode::Delete, mods) if mods.contains(KeyModifiers::CONTROL) => {
            Action::ForwardSpecial("C-DC".into())
        }
        (KeyCode::Delete, mods) if mods.contains(KeyModifiers::SHIFT) => {
            Action::ForwardSpecial("S-DC".into())
        }
        (KeyCode::Delete, _) => Action::ForwardSpecial("DC".into()),
        (KeyCode::Insert, mods) if mods.contains(KeyModifiers::CONTROL) => {
            Action::ForwardSpecial("C-IC".into())
        }
        (KeyCode::Insert, mods) if mods.contains(KeyModifiers::SHIFT) => {
            Action::ForwardSpecial("S-IC".into())
        }
        (KeyCode::Insert, _) => Action::ForwardSpecial("IC".into()),
        (KeyCode::F(n), _) => Action::ForwardSpecial(format!("F{}", n)),
        _ => Action::None,
    }
}

fn map_mouse(mouse: MouseEvent) -> Option<Action> {
    match mouse.kind {
        MouseEventKind::ScrollUp => Some(Action::ScrollUp(mouse.column, mouse.row)),
        MouseEventKind::ScrollDown => Some(Action::ScrollDown(mouse.column, mouse.row)),
        MouseEventKind::ScrollLeft => Some(Action::ScrollLeft(mouse.column, mouse.row)),
        MouseEventKind::ScrollRight => Some(Action::ScrollRight(mouse.column, mouse.row)),
        MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
            Some(Action::MouseClick(mouse.column, mouse.row))
        }
        MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
            Some(Action::MouseDrag(mouse.column, mouse.row))
        }
        MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
            Some(Action::MouseRelease(mouse.column, mouse.row))
        }
        _ => None,
    }
}
