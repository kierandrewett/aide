mod app;
mod config;
mod filebrowser;
mod git;
mod input;
mod protocol;
mod pty_backend;
mod sessions;
mod ui;

use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::DisableMouseCapture;
use crossterm::event::EnableMouseCapture;
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

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal);

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
    let mut app = App::new(config);
    app.init()?;

    let size = terminal.size()?;
    app.show_right_panel = size.width >= 100;
    app.is_narrow = size.width < 100;

    let tick_rate = Duration::from_millis(16);
    let mut last_git_status_refresh = Instant::now();
    let mut last_git_log_refresh = Instant::now();
    let mut last_output_refresh = Instant::now();
    let mut last_resize = (0u16, 0u16);

    let git_status_interval = Duration::from_secs(2);
    let git_log_interval = Duration::from_secs(3);
    let output_interval = Duration::from_millis(16);
    let mut last_bg_check = Instant::now();
    let bg_check_interval = Duration::from_secs(2);

    loop {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        // Resize PTY to match our viewport if changed
        if (app.output_width, app.output_height) != last_resize
            && app.output_width > 0
            && app.output_height > 0
        {
            let _ = app
                .session_manager
                .resize_active(app.output_width, app.output_height);
            last_resize = (app.output_width, app.output_height);
        }

        // Track terminal size
        let current_size = terminal.size().unwrap_or_default();
        app.is_narrow = current_size.width < 100;

        // Auto-show git panel on wide screens when viewing a session
        if current_size.width >= 100 && !app.is_on_welcome() && !app.show_right_panel {
            app.show_right_panel = true;
            last_resize = (0, 0);
        }

        let now = Instant::now();
        if now.duration_since(last_output_refresh) >= output_interval {
            refresh_output(&mut app);
            last_output_refresh = now;
        }

        if now.duration_since(last_git_status_refresh) >= git_status_interval {
            refresh_git_status(&mut app);
            last_git_status_refresh = now;
        }
        if now.duration_since(last_git_log_refresh) >= git_log_interval {
            refresh_git_log(&mut app);
            last_git_log_refresh = now;
        }

        if now.duration_since(last_bg_check) >= bg_check_interval {
            check_background_notifications(&mut app);
            last_bg_check = now;
        }

        let picker_mode = app.show_picker || app.show_close_confirm || app.show_command_palette;
        let actions = input::drain_actions(tick_rate, picker_mode);

        let mut did_forward = false;

        for action in actions {
            if app.show_close_confirm {
                match action {
                    Action::PickerChar('y') | Action::PickerChar('Y') => {
                        app.show_close_confirm = false;
                        let idx = app.session_manager.active_index;
                        app.session_manager.close_session(idx)?;
                        if app.session_manager.sessions.is_empty() {
                            app.should_quit = true;
                        } else {
                            app.refresh_data();
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
                        app.command_palette_confirm()?;
                        last_resize = (0, 0);
                    }
                    Action::Cancel => app.close_command_palette(),
                    Action::PickerChar(c) => {
                        app.command_palette_filter.push(c);
                        app.command_palette_selected = 0;
                    }
                    Action::PickerBackspace => {
                        app.command_palette_filter.pop();
                        app.command_palette_selected = 0;
                    }
                    Action::ScrollDown | Action::NextTab => app.command_palette_move_down(),
                    Action::ScrollUp | Action::PrevTab => app.command_palette_move_up(),
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
                        let next = (cur + 1) % total;
                        app.session_manager.active_index = next;
                    }
                    clear_active_notification(&mut app);
                    app.scroll_offset = 0;
                    app.follow_mode = true;
                    app.git_log_limit = 100;
                    app.git_log_scroll = 0;
                    app.git_status_scroll = 0;
                    app.refresh_data();
                    last_resize = (0, 0);
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
                        let prev = if cur == 0 { total - 1 } else { cur - 1 };
                        app.session_manager.active_index = prev;
                    }
                    clear_active_notification(&mut app);
                    app.scroll_offset = 0;
                    app.follow_mode = true;
                    app.git_log_limit = 100;
                    app.git_log_scroll = 0;
                    app.git_status_scroll = 0;
                    app.refresh_data();
                    last_resize = (0, 0);
                }
                Action::NewInstance => {
                    app.show_welcome = true;
                    app.session_manager.active_index = app.session_manager.sessions.len();
                }
                Action::CommandPalette => app.open_command_palette(),
                Action::ToggleFileBrowser => {
                    app.show_file_browser = !app.show_file_browser;
                    if app.show_file_browser {
                        app.focus = app::FocusPanel::FileBrowser;
                    } else {
                        app.focus = app::FocusPanel::Output;
                    }
                    last_resize = (0, 0);
                }
                Action::ToggleFileView => {
                    if app.viewing_file.is_some() {
                        if app.is_narrow {
                            app.show_file_view = !app.show_file_view;
                        } else {
                            app.close_file();
                        }
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
                    if app.show_right_panel && app.focus == app::FocusPanel::GitPanel {
                        app.show_right_panel = false;
                        app.focus = app::FocusPanel::Output;
                    } else if app.show_right_panel {
                        app.focus = app::FocusPanel::GitPanel;
                    } else {
                        app.show_right_panel = true;
                        app.focus = app::FocusPanel::GitPanel;
                    }
                    app.git_status_scroll = 0;
                    app.git_log_scroll = 0;
                    last_resize = (0, 0);
                }
                Action::ForwardChars(chars) => {
                    if app.focus == app::FocusPanel::FileBrowser && app.show_file_browser {
                        // Don't forward typing to PTY when file browser focused
                    } else if app.session_manager.active_session().is_some() {
                        let _ = app.session_manager.write_input(chars.as_bytes());
                        did_forward = true;
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
                                        app.focus = app::FocusPanel::Output;
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
                                            if let Some(e) = app.file_browser.entries.get(app.file_browser.selected) {
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
                    } else {
                        // Map special key names to actual escape sequences
                        let seq = special_key_sequence(&key);
                        if app.session_manager.active_session().is_some() {
                            let _ = app.session_manager.write_input(seq.as_bytes());
                            did_forward = true;
                            app.last_input_time = Some(Instant::now());
                            app.follow_mode = true;
                        }
                    }
                }
                Action::ForwardCtrl(c) => {
                    if app.session_manager.active_session().is_some() {
                        // Ctrl key = char & 0x1f
                        let ctrl_byte = (c as u8) & 0x1f;
                        let _ = app.session_manager.write_input(&[ctrl_byte]);
                        did_forward = true;
                        app.last_input_time = Some(Instant::now());
                        app.follow_mode = true;
                    }
                }
                Action::Paste(text) => {
                    if app.session_manager.active_session().is_some() {
                        // Send bracketed paste: \x1b[200~ ... \x1b[201~
                        // This tells the terminal/shell this is pasted text, not typed
                        let _ = app.session_manager.write_input(b"\x1b[200~");

                        // Chunk large pastes to avoid overwhelming the PTY
                        let bytes = text.as_bytes();
                        let chunk_size = 4096;
                        for chunk in bytes.chunks(chunk_size) {
                            let _ = app.session_manager.write_input(chunk);
                        }

                        let _ = app.session_manager.write_input(b"\x1b[201~");
                        did_forward = true;
                        app.last_input_time = Some(Instant::now());
                        app.follow_mode = true;
                    }
                }
                Action::EscapeKey => {
                    let size = terminal.size().unwrap_or_default();
                    if app.focus == app::FocusPanel::FileBrowser {
                        if app.is_narrow {
                            app.show_file_browser = false;
                        }
                        app.focus = app::FocusPanel::Output;
                        last_resize = (0, 0);
                    } else if app.viewing_file.is_some() {
                        app.close_file();
                        last_resize = (0, 0);
                    } else if app.show_right_panel && size.width < 100 {
                        app.show_right_panel = false;
                        app.focus = app::FocusPanel::Output;
                        last_resize = (0, 0);
                    } else if app.session_manager.active_session().is_some() {
                        let _ = app.session_manager.write_input(b"\x1b");
                        did_forward = true;
                        app.last_input_time = Some(Instant::now());
                    }
                }
                Action::ScrollUp => {
                    let reversed = app.is_narrow;
                    let delta: i16 = if reversed { -1 } else { 1 };

                    if app.focus == app::FocusPanel::FileBrowser && app.show_file_browser {
                        if delta > 0 {
                            app.file_browser.move_up();
                        } else {
                            app.file_browser.move_down();
                        }
                        // Keep selected in view
                        let sel = app.file_browser.selected as u16;
                        if sel < app.file_browser.scroll_offset {
                            app.file_browser.scroll_offset = sel;
                        }
                    } else if app.focus == app::FocusPanel::GitPanel && app.show_right_panel {
                        if delta > 0 {
                            app.git_status_scroll = app.git_status_scroll.saturating_add(1);
                            app.git_log_scroll = app.git_log_scroll.saturating_add(1);
                        } else {
                            app.git_status_scroll = app.git_status_scroll.saturating_sub(1);
                            app.git_log_scroll = app.git_log_scroll.saturating_sub(1);
                        }
                        let log_lines = app.git_log.lines().count() as u16;
                        if app.git_log_has_more && app.git_log_scroll + 20 >= log_lines {
                            app.git_log_limit += 200;
                            refresh_git_log(&mut app);
                        }
                    } else {
                        if delta > 0 {
                            app.follow_mode = false;
                            app.scroll_offset = app.scroll_offset.saturating_add(1);
                        } else {
                            app.scroll_offset = app.scroll_offset.saturating_sub(1);
                            if app.scroll_offset == 0 {
                                app.follow_mode = true;
                            }
                        }
                    }
                }
                Action::ScrollDown => {
                    let reversed = app.is_narrow;
                    let delta: i16 = if reversed { 1 } else { -1 };

                    if app.focus == app::FocusPanel::FileBrowser && app.show_file_browser {
                        if delta > 0 {
                            app.file_browser.move_down();
                        } else {
                            app.file_browser.move_up();
                        }
                        let sel = app.file_browser.selected as u16;
                        let visible = app.file_browser_area.height.saturating_sub(2);
                        if sel >= app.file_browser.scroll_offset + visible {
                            app.file_browser.scroll_offset = sel.saturating_sub(visible) + 1;
                        }
                    } else if app.focus == app::FocusPanel::GitPanel && app.show_right_panel {
                        if delta < 0 {
                            app.git_status_scroll = app.git_status_scroll.saturating_add(1);
                            app.git_log_scroll = app.git_log_scroll.saturating_add(1);
                        } else {
                            app.git_status_scroll = app.git_status_scroll.saturating_sub(1);
                            app.git_log_scroll = app.git_log_scroll.saturating_sub(1);
                        }
                    } else {
                        if delta < 0 {
                            app.follow_mode = false;
                            app.scroll_offset = app.scroll_offset.saturating_add(1);
                        } else {
                            app.scroll_offset = app.scroll_offset.saturating_sub(1);
                            if app.scroll_offset == 0 {
                                app.follow_mode = true;
                            }
                        }
                    }
                }
                Action::MouseClick(mx, my) => {
                    let tab_area = app.tab_bar_area;
                    if my >= tab_area.y && my < tab_area.y + tab_area.height {
                        for &(x_start, x_end, tab_idx) in &app.tab_click_zones {
                            if mx >= x_start && mx < x_end {
                                let total_tabs = app.session_manager.sessions.len()
                                    + if app.show_welcome
                                        || app.session_manager.sessions.is_empty()
                                    {
                                        1
                                    } else {
                                        0
                                    };
                                if tab_idx < total_tabs {
                                    if tab_idx >= app.session_manager.sessions.len() {
                                        app.show_welcome = true;
                                    }
                                    app.session_manager.active_index = tab_idx;
                                    clear_active_notification(&mut app);
                                    app.scroll_offset = 0;
                                    app.follow_mode = true;
                                    app.git_log_limit = 100;
                                    app.git_log_scroll = 0;
                                    app.git_status_scroll = 0;
                                    app.refresh_data();
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
                        // Click to select entry
                        let click_row = (my - app.file_browser_area.y).saturating_sub(1) as usize;
                        let idx = click_row + app.file_browser.scroll_offset as usize;
                        if idx < app.file_browser.entries.len() {
                            app.file_browser.selected = idx;
                        }
                    } else if app.output_area.width > 0
                        && my >= app.output_area.y
                        && my < app.output_area.y + app.output_area.height
                        && mx >= app.output_area.x
                        && mx < app.output_area.x + app.output_area.width
                    {
                        app.focus = app::FocusPanel::Output;
                    } else if app.git_panel_area.width > 0
                        && my >= app.git_panel_area.y
                        && my < app.git_panel_area.y + app.git_panel_area.height
                        && mx >= app.git_panel_area.x
                        && mx < app.git_panel_area.x + app.git_panel_area.width
                    {
                        app.focus = app::FocusPanel::GitPanel;
                    }
                }
                Action::None => {}
                _ => {}
            }
        }

        if did_forward {
            refresh_output(&mut app);
            last_output_refresh = Instant::now();
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Map special key names to terminal escape sequences.
fn special_key_sequence(key: &str) -> String {
    match key {
        "Enter" => "\r".to_string(),
        "BSpace" => "\x7f".to_string(),
        "Up" => "\x1b[A".to_string(),
        "Down" => "\x1b[B".to_string(),
        "Right" => "\x1b[C".to_string(),
        "Left" => "\x1b[D".to_string(),
        "Home" => "\x1b[H".to_string(),
        "End" => "\x1b[F".to_string(),
        "DC" => "\x1b[3~".to_string(),
        "IC" => "\x1b[2~".to_string(),
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
        _ => String::new(),
    }
}

fn refresh_output(app: &mut App) {
    if app.session_manager.active_session().is_some() {
        if let Ok(output) = app.session_manager.read_output() {
            app.claude_output = output;
        }
    }
}

fn refresh_git_status(app: &mut App) {
    if let Some(session) = app.session_manager.active_session() {
        let dir = session.directory.clone();
        if !dir.is_empty() {
            if let Ok(status) = git::status_short(&dir) {
                app.git_status = status.clone();
                // Update file browser git indicators
                app.file_browser.update_git_status(&status);
            }
            if let Ok(branch) = git::current_branch(&dir) {
                app.git_branch = branch;
            }
            app.git_upstream = git::upstream_counts(&dir);
            app.git_diff_stats = git::diff_stats(&dir);
            app.git_remote_branch = git::remote_tracking_branch(&dir).unwrap_or_default();
            app.git_file_stats = git::file_diff_stats(&dir);
        }
    }
}

fn refresh_git_log(app: &mut App) {
    if let Some(session) = app.session_manager.active_session() {
        let dir = session.directory.clone();
        if !dir.is_empty() {
            if let Ok(log) = git::log_oneline(&dir, app.git_log_limit) {
                let line_count = log.lines().count();
                app.git_log_has_more = line_count >= app.git_log_limit;
                app.git_log = log;
            }
        }
    }
}

fn check_background_notifications(app: &mut App) {
    let active = app.session_manager.active_index;
    let count = app.session_manager.sessions.len();
    for i in 0..count {
        if i == active {
            if let Ok(output) = app.session_manager.read_output_for(i) {
                let trimmed_len = output.trim().len();
                if let Some(s) = app.session_manager.sessions.get_mut(i) {
                    s.last_output_len = trimmed_len;
                }
            }
            continue;
        }

        if let Ok(output) = app.session_manager.read_output_for(i) {
            let trimmed_len = output.trim().len();
            if let Some(s) = app.session_manager.sessions.get_mut(i) {
                if trimmed_len > s.last_output_len + 50 {
                    s.has_notification = true;
                }
                s.last_output_len = trimmed_len;
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
