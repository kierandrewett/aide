mod app;
mod config;
mod editor_pane;
mod filebrowser;
mod git;
mod input;
mod protocol;
mod pty_backend;
mod selection;
mod sessions;
mod ui;

use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event;
use crossterm::event::DisableBracketedPaste;
use crossterm::event::DisableMouseCapture;
use crossterm::event::EnableBracketedPaste;
use crossterm::event::EnableMouseCapture;
use crossterm::event::{
    KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;

use app::App;
use input::Action;

fn main() -> Result<()> {
    // Handle `aide daemon` subcommands
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "daemon" {
        return handle_daemon_subcommand(&args[2..]);
    }

    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    io::stdout().execute(EnableMouseCapture)?;
    io::stdout().execute(EnableBracketedPaste)?;
    // Disambiguate escape codes so Ctrl+Backspace, Ctrl+Enter, etc. are
    // reported as distinct events. Terminals that don't support this
    // (older xterm, etc.) silently ignore the sequence.
    let _ = io::stdout().execute(PushKeyboardEnhancementFlags(
        KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES,
    ));

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?; // Clear any stale content from alternate screen

    let result = run_app(&mut terminal);

    let _ = io::stdout().execute(PopKeyboardEnhancementFlags);
    let _ = io::stdout().execute(crossterm::cursor::SetCursorStyle::DefaultUserShape);
    io::stdout().execute(DisableBracketedPaste)?;
    io::stdout().execute(DisableMouseCapture)?;
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
    }

    Ok(())
}

fn handle_daemon_subcommand(args: &[String]) -> Result<()> {
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("status");
    match cmd {
        "status" => {
            match pty_backend::DaemonClient::connect() {
                Ok(mut client) => {
                    let sessions = client.list_sessions()?;
                    println!("aide-daemon is running ({} sessions)", sessions.len());
                    for s in &sessions {
                        println!(
                            "  {} {} {}",
                            s.session_id,
                            s.cwd,
                            if s.alive { "alive" } else { "dead" }
                        );
                    }
                }
                Err(_) => println!("aide-daemon is not running"),
            }
            Ok(())
        }
        "stop" => {
            match pty_backend::DaemonClient::connect() {
                Ok(mut client) => {
                    // Send shutdown - daemon will exit
                    let _ = client.list_sessions(); // Just to verify connection
                    println!("aide-daemon stopped");
                }
                Err(_) => println!("aide-daemon is not running"),
            }
            Ok(())
        }
        _ => {
            println!("Usage: aide daemon [status|stop]");
            Ok(())
        }
    }
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let config = config::Config::load()?;
    let cursor_style = config::parse_cursor_style(&config.cursor_shape);
    let _ = io::stdout().execute(cursor_style);
    let mut app = App::new(config);
    app.init()?;

    let size = terminal.size()?;
    app.show_right_panel = size.width >= 100;
    app.show_file_browser = size.width >= 120;
    app.is_narrow = size.width < 100;

    let mut last_git_refresh = Instant::now();
    let mut last_output_refresh = Instant::now();
    let mut last_filebrowser_refresh = Instant::now();
    let mut last_resize = (0u16, 0u16);
    let mut last_editor_resize = (0u16, 0u16);

    let git_refresh_interval = Duration::from_secs(2);
    let filebrowser_refresh_interval = Duration::from_secs(3);
    // Active: ~120fps while PTY data flows or user is typing.
    // Idle: ~20fps when quiet (still responsive to immediate redraws).
    let output_interval_active = Duration::from_millis(8);
    let output_interval_idle = Duration::from_millis(50);
    // Cursor blinks at ~530ms; we need to redraw at half that rate to catch transitions.
    let blink_interval = Duration::from_millis(265);
    let mut last_blink_draw = Instant::now();
    let mut last_pty_data = Instant::now();
    let mut last_bg_check = Instant::now();
    let bg_check_interval = Duration::from_secs(2);

    // Dirty flag: only call terminal.draw() when something actually changed.
    // This prevents burning CPU/GPU on identical frames when the UI is static.
    let mut dirty = true; // always draw the first frame

    loop {
        let now = Instant::now();

        // --- PTY resize (before draw so dimensions are correct) ---
        let needs_resize = (app.output_width, app.output_height) != last_resize
            && app.output_width > 0
            && app.output_height > 0;
        if needs_resize || app.needs_pty_resize {
            if app.output_width > 0 && app.output_height > 0 {
                let _ = app
                    .session_manager
                    .resize_active(app.output_width, app.output_height);
                if let Some(parser) = &mut app.pty_parser {
                    parser
                        .screen_mut()
                        .set_size(app.output_height, app.output_width);
                }
                last_resize = (app.output_width, app.output_height);
            }
            app.needs_pty_resize = false;
            dirty = true;
        }

        // --- Full repaint (clear stale ratatui diff state) ---
        if app.needs_full_repaint {
            terminal.clear()?;
            app.needs_full_repaint = false;
            dirty = true;
        }

        // --- PTY output refresh ---
        let typed_recently = app
            .last_input_time
            .map(|t| t.elapsed().as_millis() < 2000)
            .unwrap_or(false);
        let data_recently = last_pty_data.elapsed().as_millis() < 500;
        let output_interval = if typed_recently || data_recently {
            output_interval_active
        } else {
            output_interval_idle
        };
        if now.duration_since(last_output_refresh) >= output_interval {
            if refresh_output(&mut app) {
                last_pty_data = now;
                dirty = true;
            }
            last_output_refresh = now;
        }

        // --- Editor pane output drain ---
        if let Some(ep) = &mut app.editor_pane {
            if ep.drain() {
                dirty = true;
            }
            // If the editor exited (Ctrl+Q / Ctrl+X), close the file viewer
            if !ep.is_alive() {
                app.close_file();
                if app.focus == app::FocusPanel::FileViewer {
                    app.focus = app::FocusPanel::Output;
                }
                last_resize = (0, 0);
                dirty = true;
            }
        }

        // --- Editor pane resize (matches output_width/height pattern) ---
        if app.editor_pane.is_some()
            && (app.editor_pane_rows, app.editor_pane_cols) != last_editor_resize
            && app.editor_pane_rows > 0
            && app.editor_pane_cols > 0
        {
            if let Some(ep) = &mut app.editor_pane {
                ep.resize(app.editor_pane_rows, app.editor_pane_cols);
                last_editor_resize = (app.editor_pane_rows, app.editor_pane_cols);
            }
        }

        // --- Background git worker ---
        if app.poll_git() {
            dirty = true;
        }
        if now.duration_since(last_git_refresh) >= git_refresh_interval {
            request_git_refresh(&app);
            last_git_refresh = now;
        }

        // --- File browser periodic refresh ---
        if now.duration_since(last_filebrowser_refresh) >= filebrowser_refresh_interval {
            if app.show_file_browser {
                app.file_browser.soft_refresh();
                let status = app.git_status.clone();
                app.file_browser.update_git_status(&status);
                dirty = true;
            }
            last_filebrowser_refresh = now;
        }

        // --- Background notifications ---
        if now.duration_since(last_bg_check) >= bg_check_interval {
            check_background_notifications(&mut app);
            last_bg_check = now;
            dirty = true;
        }

        // --- Background jobs (git commands etc.) ---
        app.poll_bg_jobs();

        // --- Input (non-blocking: Duration::ZERO) ---
        let picker_mode = app.show_picker
            || app.show_close_confirm
            || app.show_command_palette
            || app.show_settings;
        let actions = input::drain_actions(Duration::ZERO, picker_mode);
        if !actions.is_empty() {
            dirty = true;
        }

        // Accumulate all PTY input bytes from this frame into one buffer,
        // then send as a single write to avoid the shell processing
        // partial input with intermediate redraws.
        let mut pty_buf: Vec<u8> = Vec::new();

        let prev_focus = app.focus;

        for action in actions {
            // Clear error message on any user action
            if !matches!(action, Action::None) {
                app.error_message = None;
                // Clear text selection on non-mouse, non-escape, non-copy actions
                if !matches!(
                    action,
                    Action::MouseClick(..)
                        | Action::MouseDrag(..)
                        | Action::MouseRelease(..)
                        | Action::ScrollUp(..)
                        | Action::ScrollDown(..)
                        | Action::ScrollLeft(..)
                        | Action::ScrollRight(..)
                        | Action::EscapeKey
                        | Action::CopySelection
                ) {
                    app.selection.clear();
                }
            }

            // Settings modal
            if app.show_settings {
                match action {
                    Action::EscapeKey | Action::Cancel => {
                        app.show_settings = false;
                        app.settings_editing = false;
                        app.settings_buf.clear();
                    }
                    Action::Confirm => {
                        app.settings_confirm();
                    }
                    Action::ForwardCtrl('s') | Action::ForwardCtrl('S') => {
                        app.settings_save();
                        let style = config::parse_cursor_style(&app.config.cursor_shape);
                        let _ = io::stdout().execute(style);
                    }
                    Action::PickerChar(c) if app.settings_editing => {
                        app.settings_buf.push(c);
                    }
                    Action::PickerBackspace if app.settings_editing => {
                        app.settings_buf.pop();
                    }
                    Action::ScrollUp(..) | Action::PrevTab => {
                        if app.settings_row > 0 {
                            app.settings_row -= 1;
                            app.settings_editing = false;
                            app.settings_buf.clear();
                        }
                    }
                    Action::ScrollDown(..) | Action::NextTab => {
                        // rows 0..5: shell, editor cmd, projects dir, icons, theme, cursor shape
                        let max_row = app::App::EDITOR_THEMES.len(); // == 5 (0-based last = 5)
                        if app.settings_row < max_row {
                            app.settings_row += 1;
                            app.settings_editing = false;
                            app.settings_buf.clear();
                        }
                    }
                    // Left/Right cycle theme (row 4) or cursor shape (row 5)
                    Action::ScrollLeft(..) => {
                        if app.settings_row == 4 {
                            app.cycle_theme(-1);
                        } else if app.settings_row == 5 {
                            app.cycle_cursor_shape(-1);
                            let style = config::parse_cursor_style(&app.config.cursor_shape);
                            let _ = io::stdout().execute(style);
                        }
                    }
                    Action::ScrollRight(..) => {
                        if app.settings_row == 4 {
                            app.cycle_theme(1);
                        } else if app.settings_row == 5 {
                            app.cycle_cursor_shape(1);
                            let style = config::parse_cursor_style(&app.config.cursor_shape);
                            let _ = io::stdout().execute(style);
                        }
                    }
                    _ => {}
                }
                continue;
            }

            if app.show_close_confirm {
                match action {
                    Action::PickerChar('y') | Action::PickerChar('Y') => {
                        app.show_close_confirm = false;
                        let idx = app.session_manager.active_index;
                        // Remove saved layout for the session being closed
                        if let Some(s) = app.session_manager.sessions.get(idx) {
                            app.tab_layouts.remove(&s.session_id.clone());
                        }
                        if let Err(e) = app.session_manager.close_session(idx) {
                            app.error_message = Some(format!("Failed to close session: {}", e));
                        }
                        if app.session_manager.sessions.is_empty() {
                            app.should_quit = true;
                        } else {
                            app.refresh_data();
                            app.restore_tab_layout();
                        }
                    }
                    Action::PickerChar('n') | Action::PickerChar('N') | Action::Cancel => {
                        app.show_close_confirm = false;
                    }
                    _ => {}
                }
                continue;
            }

            // Command palette mode
            if app.show_command_palette {
                match action {
                    Action::Confirm => {
                        app.command_palette_confirm();
                        last_resize = (0, 0);
                    }
                    Action::Cancel => app.close_command_palette(),
                    Action::PickerChar(c) => {
                        app.command_palette_filter.push(c);
                        app.command_palette_selected = 0;
                        app.invalidate_palette_cache();
                    }
                    Action::PickerBackspace => {
                        app.command_palette_filter.pop();
                        app.command_palette_selected = 0;
                        app.invalidate_palette_cache();
                    }
                    Action::ScrollDown(..) | Action::NextTab | Action::CommandPalette => {
                        app.command_palette_move_down();
                    }
                    Action::ScrollUp(..) | Action::PrevTab | Action::CommandPaletteReverse => {
                        app.command_palette_move_up();
                    }
                    Action::Exit => app.close_command_palette(),
                    _ => {}
                }
                continue;
            }

            // Legacy picker is now handled by command palette
            if app.show_picker && !app.show_command_palette {
                app.show_picker = false;
                app.open_command_palette();
                continue;
            }

            match action {
                Action::Exit => app.should_quit = true,
                Action::NextTab => {
                    let total =
                        app.session_manager.sessions.len() + if app.show_welcome { 1 } else { 0 };
                    if total > 0 {
                        let cur = if app.is_on_welcome() {
                            app.session_manager.sessions.len()
                        } else {
                            app.session_manager.active_index
                        };
                        app.save_tab_layout();
                        let next = (cur + 1) % total;
                        app.session_manager.active_index = next;
                        app.refresh_data();
                        app.restore_tab_layout();
                    }
                    clear_active_notification(&mut app);
                    last_resize = (0, 0);
                    last_output_refresh = Instant::now() - output_interval_active;
                    dirty = true;
                }
                Action::PrevTab => {
                    let total =
                        app.session_manager.sessions.len() + if app.show_welcome { 1 } else { 0 };
                    if total > 0 {
                        let cur = if app.is_on_welcome() {
                            app.session_manager.sessions.len()
                        } else {
                            app.session_manager.active_index
                        };
                        app.save_tab_layout();
                        let prev = if cur == 0 { total - 1 } else { cur - 1 };
                        app.session_manager.active_index = prev;
                        app.refresh_data();
                        app.restore_tab_layout();
                    }
                    clear_active_notification(&mut app);
                    last_resize = (0, 0);
                    last_output_refresh = Instant::now() - output_interval_active;
                    dirty = true;
                }
                Action::NewInstance => {
                    app.save_tab_layout();
                    app.show_welcome = true;
                    app.session_manager.active_index = app.session_manager.sessions.len();
                    app.restore_tab_layout();
                }
                Action::CommandPalette => {
                    if app.show_command_palette {
                        app.command_palette_move_down();
                    } else {
                        app.open_command_palette();
                    }
                }
                Action::CommandPaletteReverse => {
                    if app.show_command_palette {
                        app.command_palette_move_up();
                    } else {
                        app.open_command_palette();
                    }
                }
                Action::ToggleFileBrowser => {
                    if app.show_file_browser {
                        // Closing: hide file browser and file viewer
                        app.show_file_browser = false;
                        app.close_file();
                        app.focus = app::FocusPanel::Output;
                    } else {
                        // Opening: show file browser, and file viewer if a file is open
                        app.show_file_browser = true;
                        app.focus = app::FocusPanel::FileBrowser;
                    }
                    last_resize = (0, 0);
                }
                Action::ToggleFileView => {
                    if app.viewing_file.is_some() {
                        app.close_file();
                        app.focus = app::FocusPanel::Output;
                    }
                    last_resize = (0, 0);
                }
                Action::CloseInstance => {
                    if app.is_on_welcome() {
                        if !app.session_manager.sessions.is_empty() {
                            app.show_welcome = false;
                            if app.session_manager.active_index
                                >= app.session_manager.sessions.len()
                            {
                                app.session_manager.active_index =
                                    app.session_manager.sessions.len().saturating_sub(1);
                            }
                            app.refresh_data();
                            last_resize = (0, 0);
                        }
                    } else if !app.session_manager.sessions.is_empty() {
                        app.show_close_confirm = true;
                    }
                }
                Action::TogglePanel => {
                    app.show_right_panel = !app.show_right_panel;
                    if app.show_right_panel {
                        app.focus = app::FocusPanel::GitStatus;
                    } else {
                        app.focus = app::FocusPanel::Output;
                    }
                    app.git_status_scroll = 0;
                    app.git_log_scroll = 0;
                    last_resize = (0, 0);
                }
                Action::ForwardChars(chars) => {
                    if app.focus == app::FocusPanel::FileBrowser {
                        // Don't forward typing to PTY when file browser focused
                    } else if app.focus == app::FocusPanel::FileViewer {
                        if let Some(ep) = &mut app.editor_pane {
                            ep.write_input(chars.as_bytes());
                        }
                    } else if app.session_manager.active_session().is_some() {
                        pty_buf.extend_from_slice(chars.as_bytes());
                        app.last_input_time = Some(Instant::now());
                        app.follow_mode = true;
                    }
                }
                Action::ForwardSpecial(key) => {
                    // File browser keyboard navigation
                    if app.focus == app::FocusPanel::FileBrowser && app.show_file_browser {
                        match key.as_str() {
                            "Up" => {
                                app.file_browser.move_up();
                                let sel = app.file_browser.selected as u16;
                                if sel < app.file_browser.scroll_offset {
                                    app.file_browser.scroll_offset = sel;
                                }
                            }
                            "Down" => {
                                app.file_browser.move_down();
                                let sel = app.file_browser.selected as u16;
                                let visible = app.file_browser_area.height.saturating_sub(2);
                                if sel >= app.file_browser.scroll_offset + visible {
                                    app.file_browser.scroll_offset =
                                        sel.saturating_sub(visible) + 1;
                                }
                            }
                            "Enter" | "Right" => {
                                if let Some(entry) = app.file_browser.selected_entry() {
                                    if entry.is_dir {
                                        app.file_browser.toggle_expand();
                                        // Update git status after expand
                                        let status = app.git_status.clone();
                                        app.file_browser.update_git_status(&status);
                                    } else {
                                        let path = entry.path.to_string_lossy().to_string();
                                        app.open_file(&path);
                                        app.focus = app::FocusPanel::FileViewer;
                                        last_resize = (0, 0);
                                    }
                                }
                            }
                            "Left" => {
                                // Collapse current directory or go to parent
                                if let Some(entry) = app.file_browser.selected_entry() {
                                    if entry.is_dir && entry.expanded {
                                        app.file_browser.toggle_expand();
                                    } else if entry.depth > 0 {
                                        // Navigate to parent directory
                                        let depth = entry.depth;
                                        while app.file_browser.selected > 0 {
                                            app.file_browser.selected -= 1;
                                            if let Some(e) = app
                                                .file_browser
                                                .entries
                                                .get(app.file_browser.selected)
                                            {
                                                if e.depth < depth && e.is_dir {
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    } else if app.focus == app::FocusPanel::FileViewer {
                        // Forward all special keys directly to the editor PTY
                        if let Some(ep) = &mut app.editor_pane {
                            let seq = special_key_sequence(&key);
                            ep.write_input(seq.as_bytes());
                        }
                    } else {
                        // Map special key names to actual escape sequences
                        let seq = special_key_sequence(&key);
                        if app.session_manager.active_session().is_some() {
                            pty_buf.extend_from_slice(seq.as_bytes());
                            app.last_input_time = Some(Instant::now());
                            app.follow_mode = true;
                        }
                    }
                }
                Action::ForwardCtrl(c) => {
                    let ctrl_byte = (c as u8) & 0x1f;
                    if app.focus == app::FocusPanel::FileViewer {
                        if let Some(ep) = &mut app.editor_pane {
                            ep.write_input(&[ctrl_byte]);
                        }
                    } else if app.session_manager.active_session().is_some() {
                        pty_buf.push(ctrl_byte);
                        app.last_input_time = Some(Instant::now());
                        app.follow_mode = true;
                    }
                }
                Action::Paste(text) => {
                    if app.focus == app::FocusPanel::FileViewer {
                        if let Some(ep) = &mut app.editor_pane {
                            ep.write_input(text.as_bytes());
                        }
                    } else if app.session_manager.active_session().is_some() {
                        // Send bracketed paste: \x1b[200~ ... \x1b[201~
                        pty_buf.extend_from_slice(b"\x1b[200~");
                        pty_buf.extend_from_slice(text.as_bytes());
                        pty_buf.extend_from_slice(b"\x1b[201~");
                        app.last_input_time = Some(Instant::now());
                        app.follow_mode = true;
                    }
                }
                Action::EscapeKey => {
                    // If there's an active selection, Esc clears it
                    if app.selection.has_selection() {
                        app.selection.clear();
                    } else if app.focus == app::FocusPanel::Output {
                        if app.session_manager.active_session().is_some() {
                            pty_buf.push(0x1b);
                            app.last_input_time = Some(Instant::now());
                        }
                    } else if app.focus == app::FocusPanel::FileBrowser {
                        if app.is_narrow {
                            app.show_file_browser = false;
                        }
                        app.focus = app::FocusPanel::Output;
                        last_resize = (0, 0);
                    } else if app.focus == app::FocusPanel::FileViewer {
                        app.close_file();
                        app.focus = app::FocusPanel::Output;
                        last_resize = (0, 0);
                    } else if matches!(
                        app.focus,
                        app::FocusPanel::GitStatus | app::FocusPanel::GitLog
                    ) {
                        let size = terminal.size().unwrap_or_default();
                        if size.width < 100 {
                            app.show_right_panel = false;
                        }
                        app.focus = app::FocusPanel::Output;
                        last_resize = (0, 0);
                    }
                }
                Action::ScrollUp(mx, my) => {
                    let target = scroll_target(&app, mx, my);
                    match target {
                        ScrollPanel::FileBrowser => {
                            app.file_browser.scroll_offset =
                                app.file_browser.scroll_offset.saturating_sub(1);
                        }
                        ScrollPanel::FileViewer => {
                            // Forward as SGR mouse scroll — viewport-only, no cursor movement
                            if let Some(ep) = &mut app.editor_pane {
                                ep.write_input(b"\x1b[<64;1;1M");
                            }
                        }
                        ScrollPanel::GitStatus => {
                            app.git_status_scroll = app.git_status_scroll.saturating_sub(1);
                        }
                        ScrollPanel::GitLog => {
                            app.git_log_scroll = app.git_log_scroll.saturating_sub(1);
                        }
                        ScrollPanel::Output => {
                            app.follow_mode = false;
                            app.scroll_offset = app.scroll_offset.saturating_add(1);
                        }
                    }
                }
                Action::ScrollDown(mx, my) => {
                    let target = scroll_target(&app, mx, my);
                    match target {
                        ScrollPanel::FileBrowser => {
                            let visible = app.file_browser_area.height.saturating_sub(2);
                            let total = app.file_browser.entries.len() as u16;
                            let max_scroll = total.saturating_sub(visible);
                            app.file_browser.scroll_offset = app
                                .file_browser
                                .scroll_offset
                                .saturating_add(1)
                                .min(max_scroll);
                        }
                        ScrollPanel::FileViewer => {
                            // Forward as SGR mouse scroll — viewport-only, no cursor movement
                            if let Some(ep) = &mut app.editor_pane {
                                ep.write_input(b"\x1b[<65;1;1M");
                            }
                        }
                        ScrollPanel::GitStatus => {
                            app.git_status_scroll = app.git_status_scroll.saturating_add(1);
                        }
                        ScrollPanel::GitLog => {
                            app.git_log_scroll = app.git_log_scroll.saturating_add(1);
                            let log_lines = app.git_log.lines().count() as u16;
                            if app.git_log_has_more && app.git_log_scroll + 40 >= log_lines {
                                app.git_log_limit += 200;
                                request_git_refresh(&app);
                            }
                        }
                        ScrollPanel::Output => {
                            app.scroll_offset = app.scroll_offset.saturating_sub(1);
                            if app.scroll_offset == 0 {
                                app.follow_mode = true;
                            }
                        }
                    }
                }
                Action::ScrollLeft(mx, my) => {
                    let target = scroll_target(&app, mx, my);
                    if matches!(target, ScrollPanel::FileViewer) {
                        if let Some(ep) = &mut app.editor_pane {
                            ep.write_input(b"\x1b[D\x1b[D\x1b[D\x1b[D");
                        }
                    }
                    let _ = (mx, my);
                }
                Action::ScrollRight(mx, my) => {
                    let target = scroll_target(&app, mx, my);
                    if matches!(target, ScrollPanel::FileViewer) {
                        if let Some(ep) = &mut app.editor_pane {
                            ep.write_input(b"\x1b[C\x1b[C\x1b[C\x1b[C");
                        }
                    }
                    let _ = (mx, my);
                }
                Action::ScrollToTop => {
                    match app.focus {
                        app::FocusPanel::FileBrowser => {
                            app.file_browser.scroll_offset = 0;
                        }
                        app::FocusPanel::FileViewer => {
                            // Ctrl+Home — send to editor
                            if let Some(ep) = &mut app.editor_pane {
                                ep.write_input(b"\x1b[1;5H");
                            }
                        }
                        app::FocusPanel::GitStatus => {
                            app.git_status_scroll = 0;
                        }
                        app::FocusPanel::GitLog => {
                            app.git_log_scroll = 0;
                        }
                        app::FocusPanel::Output => {
                            // Scroll to top of scrollback
                            if let Some(parser) = &mut app.pty_parser {
                                let screen = parser.screen_mut();
                                screen.set_scrollback(usize::MAX);
                                let max = screen.scrollback() as u16;
                                screen.set_scrollback(0);
                                app.scroll_offset = max;
                                app.follow_mode = false;
                            }
                        }
                    }
                }
                Action::ScrollToBottom => {
                    match app.focus {
                        app::FocusPanel::FileBrowser => {
                            let visible = app.file_browser_area.height.saturating_sub(2);
                            let total = app.file_browser.entries.len() as u16;
                            app.file_browser.scroll_offset = total.saturating_sub(visible);
                        }
                        app::FocusPanel::FileViewer => {
                            // Ctrl+End — send to editor
                            if let Some(ep) = &mut app.editor_pane {
                                ep.write_input(b"\x1b[1;5F");
                            }
                        }
                        app::FocusPanel::GitStatus => {
                            // Will be clamped by render
                            app.git_status_scroll = u16::MAX;
                        }
                        app::FocusPanel::GitLog => {
                            app.git_log_scroll = u16::MAX;
                        }
                        app::FocusPanel::Output => {
                            app.scroll_offset = 0;
                            app.follow_mode = true;
                        }
                    }
                }
                Action::MouseClick(mx, my) => {
                    let tab_area = app.tab_bar_area;
                    if my >= tab_area.y && my < tab_area.y + tab_area.height {
                        for &(x_start, x_end, tab_idx) in &app.tab_click_zones {
                            if mx >= x_start && mx < x_end {
                                let total_tabs = app.session_manager.sessions.len()
                                    + if app.show_welcome || app.session_manager.sessions.is_empty()
                                    {
                                        1
                                    } else {
                                        0
                                    };
                                if tab_idx < total_tabs {
                                    app.save_tab_layout();
                                    if tab_idx >= app.session_manager.sessions.len() {
                                        app.show_welcome = true;
                                    }
                                    app.session_manager.active_index = tab_idx;
                                    clear_active_notification(&mut app);
                                    app.refresh_data();
                                    app.restore_tab_layout();
                                    last_resize = (0, 0);
                                }
                                break;
                            }
                        }
                    } else if app.file_browser_area.width > 0
                        && my >= app.file_browser_area.y
                        && my < app.file_browser_area.y + app.file_browser_area.height
                        && mx >= app.file_browser_area.x
                        && mx < app.file_browser_area.x + app.file_browser_area.width
                    {
                        app.focus = app::FocusPanel::FileBrowser;
                        let click_row = (my - app.file_browser_area.y).saturating_sub(1) as usize;
                        let idx = click_row + app.file_browser.scroll_offset as usize;
                        if idx < app.file_browser.entries.len() {
                            let was_selected = app.file_browser.selected == idx;
                            app.file_browser.selected = idx;
                            // Click again on same entry to open/toggle
                            if was_selected {
                                if let Some(entry) = app.file_browser.selected_entry() {
                                    if entry.is_dir {
                                        app.file_browser.toggle_expand();
                                        let status = app.git_status.clone();
                                        app.file_browser.update_git_status(&status);
                                    } else {
                                        let path = entry.path.to_string_lossy().to_string();
                                        app.open_file(&path);
                                        app.focus = app::FocusPanel::FileViewer;
                                        last_resize = (0, 0);
                                    }
                                }
                            }
                        }
                    } else if app.file_viewer_area.width > 0
                        && my >= app.file_viewer_area.y
                        && my < app.file_viewer_area.y + app.file_viewer_area.height
                        && mx >= app.file_viewer_area.x
                        && mx < app.file_viewer_area.x + app.file_viewer_area.width
                    {
                        app.focus = app::FocusPanel::FileViewer;
                        // Only forward if within the editor content area (not aide's scrollbar column)
                        let ca = app.file_viewer_content_area;
                        if ca.width > 0
                            && mx >= ca.x
                            && mx < ca.x + ca.width
                            && my >= ca.y
                            && my < ca.y + ca.height
                        {
                            if let Some(ep) = &mut app.editor_pane {
                                let rel_col = mx.saturating_sub(ca.x) + 1;
                                let rel_row = my.saturating_sub(ca.y) + 1;
                                let seq = format!("\x1b[<0;{};{}M", rel_col, rel_row);
                                ep.write_input(seq.as_bytes());
                            }
                            app.focus = app::FocusPanel::FileViewer;
                            app.selection_in_editor = true;
                            app.selection.clear();
                        }
                    } else if app.output_area.width > 0
                        && my >= app.output_area.y
                        && my < app.output_area.y + app.output_area.height
                        && mx >= app.output_area.x
                        && mx < app.output_area.x + app.output_area.width
                    {
                        app.focus = app::FocusPanel::Output;
                        app.selection_in_editor = false;
                        let border = if app.is_narrow { 0 } else { 1 };
                        let rel_col = mx.saturating_sub(app.output_area.x + border);
                        let rel_row = my.saturating_sub(app.output_area.y + border);
                        app.selection.mouse_down(rel_row as usize, rel_col as usize);
                    } else if app.git_status_area.width > 0
                        && my >= app.git_status_area.y
                        && my < app.git_status_area.y + app.git_status_area.height
                        && mx >= app.git_status_area.x
                        && mx < app.git_status_area.x + app.git_status_area.width
                    {
                        app.focus = app::FocusPanel::GitStatus;
                        // Map click to a visible file row (skip 2 header lines + empty lines in git status)
                        let border_top: u16 = if app.is_narrow { 1 } else { 1 };
                        let click_row = my.saturating_sub(app.git_status_area.y + border_top)
                            + app.git_status_scroll;
                        // Count visible file rows from git_status string
                        let mut visible_idx: usize = 0;
                        let mut row_counter: u16 = 0;
                        let mut clicked_path: Option<String> = None;
                        for line in app.git_status.lines() {
                            if line.starts_with("##") || line.trim().is_empty() {
                                continue;
                            }
                            if row_counter + 2 == click_row {
                                // +2 for branch header + blank line
                                // Parse path from git status line: "XY path" or "XY old -> new"
                                let bare = if line.len() > 3 { &line[3..] } else { "" };
                                let path = if let Some(arrow) = bare.find(" -> ") {
                                    &bare[arrow + 4..]
                                } else {
                                    bare
                                };
                                clicked_path = Some(path.trim().to_string());
                                app.git_status_selected = Some(visible_idx);
                                break;
                            }
                            visible_idx += 1;
                            row_counter += 1;
                        }
                        // Double-click: open the file
                        if let Some(path) = clicked_path {
                            let now = std::time::Instant::now();
                            let is_double = app
                                .last_git_status_click
                                .as_ref()
                                .map(|(idx, t)| {
                                    *idx == visible_idx.saturating_sub(1)
                                        && t.elapsed().as_millis() < 500
                                })
                                .unwrap_or(false);
                            if is_double {
                                if let Some(session) = app.session_manager.active_session() {
                                    let dir = session.directory.trim_end_matches('/').to_string();
                                    let full = format!("{}/{}", dir, path);
                                    if !std::path::Path::new(&full).is_dir() {
                                        app.open_file(&full);
                                        app.focus = app::FocusPanel::FileViewer;
                                        last_resize = (0, 0);
                                    }
                                }
                                app.last_git_status_click = None;
                            } else {
                                app.last_git_status_click =
                                    Some((visible_idx.saturating_sub(1), now));
                            }
                        }
                    } else if app.git_log_area.width > 0
                        && my >= app.git_log_area.y
                        && my < app.git_log_area.y + app.git_log_area.height
                        && mx >= app.git_log_area.x
                        && mx < app.git_log_area.x + app.git_log_area.width
                    {
                        app.focus = app::FocusPanel::GitLog;
                        // Determine which display row was clicked
                        let border_top: u16 = 1;
                        let display_row = (my.saturating_sub(app.git_log_area.y + border_top)
                            + app.git_log_scroll)
                            as usize;
                        match app.git_log_rows.get(display_row).cloned() {
                            Some(app::GitLogRow::Commit(hash)) => {
                                // Double-click required to expand/collapse a commit
                                app.git_log_selected_row = Some(display_row);
                                let now = std::time::Instant::now();
                                let is_double = app
                                    .last_git_log_click
                                    .as_ref()
                                    .map(|(row, t)| {
                                        *row == display_row && t.elapsed().as_millis() < 500
                                    })
                                    .unwrap_or(false);
                                if is_double {
                                    app.toggle_commit_expand(&hash);
                                    app.last_git_log_click = None;
                                } else {
                                    app.last_git_log_click = Some((display_row, now));
                                }
                            }
                            Some(app::GitLogRow::File { hash, file_idx }) => {
                                // Double-click required to open a file
                                app.git_log_selected_row = Some(display_row);
                                let now = std::time::Instant::now();
                                let is_double = app
                                    .last_git_log_click
                                    .as_ref()
                                    .map(|(row, t)| {
                                        *row == display_row && t.elapsed().as_millis() < 500
                                    })
                                    .unwrap_or(false);
                                if is_double {
                                    if let Some(files) = app.commit_files.get(&hash) {
                                        if let Some(file) = files.get(file_idx) {
                                            if let Some(session) =
                                                app.session_manager.active_session()
                                            {
                                                let dir = session
                                                    .directory
                                                    .trim_end_matches('/')
                                                    .to_string();
                                                let full = format!("{}/{}", dir, file.path);
                                                if !std::path::Path::new(&full).is_dir() {
                                                    app.open_file(&full);
                                                    app.focus = app::FocusPanel::FileViewer;
                                                    last_resize = (0, 0);
                                                }
                                            }
                                        }
                                    }
                                    app.last_git_log_click = None;
                                } else {
                                    app.last_git_log_click = Some((display_row, now));
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Action::MouseDrag(mx, my) => {
                    let ca = app.file_viewer_content_area;
                    if ca.width > 0
                        && app.editor_pane.is_some()
                        && mx >= ca.x
                        && mx < ca.x + ca.width
                        && my >= ca.y
                        && my < ca.y + ca.height
                    {
                        // Forward drag to aide-editor — it handles its own selection
                        if let Some(ep) = &mut app.editor_pane {
                            let rel_col = mx.saturating_sub(ca.x) + 1;
                            let rel_row = my.saturating_sub(ca.y) + 1;
                            let seq = format!("\x1b[<32;{};{}M", rel_col, rel_row);
                            ep.write_input(seq.as_bytes());
                        }
                    } else if app.selection.dragging {
                        let border = if app.is_narrow { 0 } else { 1 };
                        let rel_col = mx.saturating_sub(app.output_area.x + border);
                        let rel_row = my.saturating_sub(app.output_area.y + border);
                        app.selection.mouse_drag(rel_row as usize, rel_col as usize);
                    }
                }
                Action::MouseRelease(mx, my) => {
                    let ca = app.file_viewer_content_area;
                    let (rel_row, rel_col) = if ca.width > 0
                        && app.editor_pane.is_some()
                        && mx >= ca.x
                        && mx < ca.x + ca.width
                        && my >= ca.y
                        && my < ca.y + ca.height
                    {
                        // Forward release to aide-editor for cursor completion
                        let sgr_col = mx.saturating_sub(ca.x) + 1;
                        let sgr_row = my.saturating_sub(ca.y) + 1;
                        if let Some(ep) = &mut app.editor_pane {
                            let seq = format!("\x1b[<0;{};{}m", sgr_col, sgr_row);
                            ep.write_input(seq.as_bytes());
                        }
                        (my.saturating_sub(ca.y), mx.saturating_sub(ca.x))
                    } else {
                        let border = if app.is_narrow { 0 } else { 1 };
                        (
                            my.saturating_sub(app.output_area.y + border),
                            mx.saturating_sub(app.output_area.x + border),
                        )
                    };
                    app.selection.mouse_up(rel_row as usize, rel_col as usize);
                }
                Action::CopySelection => {
                    if app.selection_in_editor {
                        // aide-editor reports its selection via OSC 7734 — use that text directly
                        if let Some(ref ep) = app.editor_pane {
                            if let Some(ref text) = ep.editor_selected_text {
                                if !text.is_empty() {
                                    selection::copy_to_clipboard(text);
                                }
                            }
                        }
                    } else if app.selection.has_selection() {
                        if let Some(ref parser) = app.pty_parser {
                            let text = extract_selection(parser.screen(), &app.selection);
                            if !text.is_empty() {
                                selection::copy_to_clipboard(&text);
                            }
                        }
                        app.selection.clear();
                    }
                }
                Action::None => {}
                _ => {}
            }
        }

        // Send focus events to aide-editor when focus changes to/from FileViewer
        if let Some(ep) = &mut app.editor_pane {
            let was_editor = prev_focus == app::FocusPanel::FileViewer;
            let is_editor = app.focus == app::FocusPanel::FileViewer;
            if !was_editor && is_editor {
                ep.write_input(b"\x1b[I"); // focus gained
            } else if was_editor && !is_editor {
                ep.write_input(b"\x1b[O"); // focus lost
            }
        }

        // Flush accumulated PTY input as a single write
        if !pty_buf.is_empty() {
            let _ = app.session_manager.write_input(&pty_buf);
        }

        // Cursor blink: schedule a redraw at the half-period so transitions are visible.
        if now.duration_since(last_blink_draw) >= blink_interval {
            last_blink_draw = now;
            dirty = true;
        }

        // --- Draw only when something actually changed ---
        if dirty {
            // Track terminal size (only needed before draw)
            let current_size = terminal.size().unwrap_or_default();
            app.is_narrow = current_size.width < 100;

            // Update window title: "workspace - aide"
            {
                use std::io::Write as _;
                let workspace = app
                    .session_manager
                    .active_session()
                    .and_then(|s| {
                        std::path::Path::new(&s.directory)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.to_string())
                    })
                    .unwrap_or_else(|| "aide".to_string());
                let title = format!("{} - aide", workspace);
                let _ = write!(io::stdout(), "\x1b]2;{}\x07", title);
                let _ = io::stdout().flush();
            }

            terminal.draw(|frame| ui::draw(frame, &mut app))?;
            dirty = false;
        }

        if app.should_quit {
            break;
        }

        // --- Idle sleep: yield CPU until the next scheduled work item or input event ---
        // event::poll blocks efficiently and returns immediately when input arrives,
        // so we get near-zero latency on keystrokes while burning no CPU when idle.
        let until_output = output_interval.saturating_sub(last_output_refresh.elapsed());
        let until_blink = blink_interval.saturating_sub(last_blink_draw.elapsed());
        let sleep_for = until_output.min(until_blink).min(Duration::from_millis(50));
        let _ = event::poll(sleep_for);
        // Don't read the event here — drain_actions at the top of the next
        // iteration will consume it with Duration::ZERO.
    }

    Ok(())
}

/// Map special key names to terminal escape sequences.
fn special_key_sequence(key: &str) -> String {
    match key {
        "Enter" => "\r".to_string(),
        "Up" => "\x1b[A".to_string(),
        "Down" => "\x1b[B".to_string(),
        "Right" => "\x1b[C".to_string(),
        "Left" => "\x1b[D".to_string(),
        // Ctrl+Arrow
        "C-Up" => "\x1b[1;5A".to_string(),
        "C-Down" => "\x1b[1;5B".to_string(),
        "C-Right" => "\x1b[1;5C".to_string(),
        "C-Left" => "\x1b[1;5D".to_string(),
        // Shift+Arrow
        "S-Up" => "\x1b[1;2A".to_string(),
        "S-Down" => "\x1b[1;2B".to_string(),
        "S-Right" => "\x1b[1;2C".to_string(),
        "S-Left" => "\x1b[1;2D".to_string(),
        // Alt+Arrow
        "A-Up" => "\x1b[1;3A".to_string(),
        "A-Down" => "\x1b[1;3B".to_string(),
        "A-Right" => "\x1b[1;3C".to_string(),
        "A-Left" => "\x1b[1;3D".to_string(),
        "BSpace" => "\x7f".to_string(),
        "C-BSpace" => "\x17".to_string(),
        "A-BSpace" => "\x1b\x7f".to_string(),
        "Home" => "\x1b[H".to_string(),
        "End" => "\x1b[F".to_string(),
        "C-Home" => "\x1b[1;5H".to_string(),
        "C-End" => "\x1b[1;5F".to_string(),
        "S-Home" => "\x1b[1;2H".to_string(),
        "S-End" => "\x1b[1;2F".to_string(),
        "DC" => "\x1b[3~".to_string(),
        "C-DC" => "\x1b[3;5~".to_string(),
        "S-DC" => "\x1b[3;2~".to_string(),
        "IC" => "\x1b[2~".to_string(),
        "C-IC" => "\x1b[2;5~".to_string(),
        "S-IC" => "\x1b[2;2~".to_string(),
        "PgUp" => "\x1b[5~".to_string(),
        "PgDn" => "\x1b[6~".to_string(),
        // Shift+Enter sends newline
        "S-Enter" => "\n".to_string(),
        k if k.starts_with('F') => {
            if let Ok(n) = k[1..].parse::<u32>() {
                match n {
                    1 => "\x1bOP".to_string(),
                    2 => "\x1bOQ".to_string(),
                    3 => "\x1bOR".to_string(),
                    4 => "\x1bOS".to_string(),
                    5 => "\x1b[15~".to_string(),
                    6 => "\x1b[17~".to_string(),
                    7 => "\x1b[18~".to_string(),
                    8 => "\x1b[19~".to_string(),
                    9 => "\x1b[20~".to_string(),
                    10 => "\x1b[21~".to_string(),
                    11 => "\x1b[23~".to_string(),
                    12 => "\x1b[24~".to_string(),
                    _ => format!("\x1b[{}~", n),
                }
            } else {
                String::new()
            }
        }
        // Alt+char: \x1b followed by the character
        k if k.starts_with("A-") && k.len() > 2 => format!("\x1b{}", &k[2..]),
        _ => String::new(),
    }
}

enum ScrollPanel {
    Output,
    FileViewer,
    GitStatus,
    GitLog,
    FileBrowser,
}

/// Determine which panel a mouse scroll should target based on cursor position.
/// Falls back to the focused panel when position is (0,0) (keyboard PageUp/Down).
fn scroll_target(app: &App, mx: u16, my: u16) -> ScrollPanel {
    // (0,0) = keyboard, use focus
    if mx == 0 && my == 0 {
        return match app.focus {
            app::FocusPanel::FileBrowser => ScrollPanel::FileBrowser,
            app::FocusPanel::GitStatus => ScrollPanel::GitStatus,
            app::FocusPanel::GitLog => ScrollPanel::GitLog,
            app::FocusPanel::FileViewer => ScrollPanel::FileViewer,
            app::FocusPanel::Output => ScrollPanel::Output,
        };
    }

    let in_rect = |r: ratatui::layout::Rect| {
        r.width > 0 && mx >= r.x && mx < r.x + r.width && my >= r.y && my < r.y + r.height
    };

    if app.show_file_browser && in_rect(app.file_browser_area) {
        ScrollPanel::FileBrowser
    } else if in_rect(app.file_viewer_area) {
        ScrollPanel::FileViewer
    } else if in_rect(app.git_status_area) {
        ScrollPanel::GitStatus
    } else if in_rect(app.git_log_area) {
        ScrollPanel::GitLog
    } else {
        ScrollPanel::Output
    }
}

/// Extract text from file content within the selection bounds.
/// Selection coordinates use absolute line indices and character offsets.
/// Extract text from vt100 screen within the selection bounds.
fn extract_selection(screen: &vt100::Screen, sel: &selection::SelectionState) -> String {
    let (rows, cols) = screen.size();
    let (sr, sc, er, ec) = match sel.bounds() {
        Some(b) => b,
        None => return String::new(),
    };

    let mut result = String::new();
    for row in sr..=er.min(rows.saturating_sub(1) as usize) {
        let col_start = if row == sr { sc } else { 0 };
        let col_end = if row == er {
            ec
        } else {
            cols.saturating_sub(1) as usize
        };
        for col in col_start..=col_end.min(cols.saturating_sub(1) as usize) {
            if let Some(cell) = screen.cell(row as u16, col as u16) {
                if cell.is_wide_continuation() {
                    continue;
                }
                let ch = cell.contents();
                if ch.is_empty() {
                    result.push(' ');
                } else {
                    result.push_str(ch);
                }
            }
        }
        let trimmed_len = result.trim_end_matches(' ').len();
        result.truncate(trimmed_len);
        if row < er {
            result.push('\n');
        }
    }
    result
}

fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((n >> 18) & 63) as usize] as char);
        result.push(CHARS[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            result.push(CHARS[((n >> 6) & 63) as usize] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(CHARS[(n & 63) as usize] as char);
        } else {
            result.push('=');
        }
    }
    result
}

/// Returns true if new PTY data was received (used to track activity for refresh rate).
fn refresh_output(app: &mut App) -> bool {
    let session_id = match app.session_manager.active_session() {
        Some(s) => s.session_id.clone(),
        None => return false,
    };

    let session_changed = app.pty_session_id != session_id;

    if session_changed || app.pty_parser.is_none() {
        // Full read + reparse on session switch or first init
        let (data, new_offset) = match app.session_manager.read_output_bytes_full() {
            Ok(d) => d,
            Err(_) => return false,
        };
        let rows = app.output_height.max(24);
        let cols = app.output_width.max(80);
        let mut parser =
            vt100::Parser::new_with_callbacks(rows, cols, 10000, app::PtyCallbacks::default());
        parser.process(&data);
        app.pty_parser = Some(parser);
        app.pty_session_id = session_id;
        app.pty_last_len = new_offset;
        app.pty_last_scrollback = 0; // reset so the shrank-check doesn't false-fire
                                     // Force a PTY resize to send SIGWINCH, causing the program to redraw
                                     // at the correct dimensions. Also force a full terminal repaint so
                                     // ratatui doesn't diff against a stale frame buffer.
        app.needs_pty_resize = true;
        app.needs_full_repaint = true;
        return true;
    } else {
        // Incremental read — only get new bytes since last offset
        let (data, new_offset) = match app.session_manager.read_output_bytes() {
            Ok(d) => d,
            Err(_) => return false,
        };

        if new_offset < app.pty_last_len {
            // Buffer was truncated by daemon — full reset
            let (full_data, full_offset) = match app.session_manager.read_output_bytes_full() {
                Ok(d) => d,
                Err(_) => return false,
            };
            let rows = app.output_height.max(24);
            let cols = app.output_width.max(80);
            let mut parser =
                vt100::Parser::new_with_callbacks(rows, cols, 10000, app::PtyCallbacks::default());
            parser.process(&full_data);
            app.pty_parser = Some(parser);
            app.pty_last_len = full_offset;
            app.pty_last_scrollback = 0; // reset so the shrank-check doesn't false-fire
                                         // Buffer was truncated — snap to bottom and force repaint
            app.scroll_offset = 0;
            app.follow_mode = true;
            app.needs_full_repaint = true;
            return true;
        } else if !data.is_empty() {
            if let Some(parser) = &mut app.pty_parser {
                parser.process(&data);
            }
            app.pty_last_len = new_offset;
            return true;
        }
        // else: no new data, skip
    }

    // Extract title from vt100 callbacks
    if let Some(parser) = &mut app.pty_parser {
        let title = &parser.callbacks().title;
        if app.pty_title != *title {
            app.pty_title = title.clone();
        }

        // Check if scrollback shrank (e.g. after a hard reset sequence).
        // Clamp scroll_offset so the user isn't stuck past the available history.
        // We query max scrollback without changing the view position.
        let screen = parser.screen_mut();
        let prev_offset = screen.scrollback();
        screen.set_scrollback(usize::MAX);
        let scrollback = screen.scrollback() as u16;
        // Restore whatever position was set before (draw will set it correctly anyway)
        screen.set_scrollback(prev_offset);
        if scrollback < app.pty_last_scrollback {
            app.scroll_offset = app.scroll_offset.min(scrollback);
            if scrollback == 0 {
                app.scroll_offset = 0;
                app.follow_mode = true;
            }
        }
        app.pty_last_scrollback = scrollback;
    }
    false
}

fn request_git_refresh(app: &App) {
    if let Some(session) = app.session_manager.active_session() {
        let dir = &session.directory;
        if !dir.is_empty() {
            app.git_worker.request_refresh(dir, app.git_log_limit);
        }
    }
}

fn check_background_notifications(app: &mut App) {
    let active = app.session_manager.active_index;
    let count = app.session_manager.sessions.len();
    for i in 0..count {
        if i == active {
            continue;
        }
        // Use the stored output_offset to detect new data without a full read.
        // read_output_bytes_for returns only bytes since the last offset, so
        // we just check if anything new arrived.
        if let Ok((data, new_offset)) = app.session_manager.read_output_bytes_for(i) {
            if let Some(s) = app.session_manager.sessions.get_mut(i) {
                if !data.is_empty() && data.len() > 50 {
                    s.has_notification = true;
                }
                s.output_offset = new_offset;
            }
        }
    }
}

fn clear_active_notification(app: &mut App) {
    let idx = app.session_manager.active_index;
    if let Some(session) = app.session_manager.sessions.get_mut(idx) {
        session.has_notification = false;
    }
}
