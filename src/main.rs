mod app;
mod config;
mod git;
mod input;
mod sessions;
mod tmux;
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

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    let config = config::Config::load()?;
    let mut app = App::new(config);
    app.init()?;

    let size = terminal.size()?;
    app.show_right_panel = size.width >= 100;

    let tick_rate = Duration::from_millis(16);
    let mut last_git_status_refresh = Instant::now();
    let mut last_git_log_refresh = Instant::now();
    let mut last_output_refresh = Instant::now();
    let mut last_resize = (0u16, 0u16);

    let git_status_interval = Duration::from_secs(2);
    let git_log_interval = Duration::from_secs(3);
    let output_interval = Duration::from_millis(50);
    let mut last_bg_check = Instant::now();
    let bg_check_interval = Duration::from_secs(2);

    loop {
        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        // Resize tmux pane to match our viewport if changed
        if (app.output_width, app.output_height) != last_resize
            && app.output_width > 0
            && app.output_height > 0
        {
            if let Some(session) = app.session_manager.active_session() {
                let name = session.name.clone();
                let _ = tmux::resize_pane(&name, app.output_width, app.output_height);
            }
            last_resize = (app.output_width, app.output_height);
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

        // Check background tabs for output changes
        if now.duration_since(last_bg_check) >= bg_check_interval {
            check_background_notifications(&mut app);
            last_bg_check = now;
        }

        let picker_mode = app.show_picker || app.show_close_confirm;
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

            if app.show_picker {
                match action {
                    Action::Confirm => app.picker_select_confirm()?,
                    Action::Cancel => app.close_picker(),
                    Action::PickerChar(c) => {
                        app.picker_filter.push(c);
                        app.picker_selected = 0;
                    }
                    Action::PickerBackspace => {
                        app.picker_filter.pop();
                        app.picker_selected = 0;
                    }
                    Action::ScrollDown | Action::NextTab => app.picker_move_down(),
                    Action::ScrollUp | Action::PrevTab => app.picker_move_up(),
                    Action::Exit => app.close_picker(),
                    _ => {}
                }
                continue;
            }

            match action {
                Action::Exit => app.should_quit = true,
                Action::NextTab => {
                    // Total tabs = sessions + (1 if welcome open)
                    let total =
                        app.session_manager.sessions.len() + if app.show_welcome { 1 } else { 0 };
                    if total > 0 {
                        let cur = if app.is_on_welcome() {
                            app.session_manager.sessions.len()
                        } else {
                            app.session_manager.active_index
                        };
                        let next = (cur + 1) % total;
                        if next >= app.session_manager.sessions.len() {
                            // Landing on the welcome tab
                            app.session_manager.active_index = next;
                        } else {
                            app.session_manager.active_index = next;
                        }
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
                        if prev >= app.session_manager.sessions.len() {
                            // Landing on the welcome tab
                            app.session_manager.active_index = prev;
                        } else {
                            app.session_manager.active_index = prev;
                        }
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
                    // Open and switch to the welcome/splash tab
                    app.show_welcome = true;
                    app.session_manager.active_index = app.session_manager.sessions.len();
                }
                Action::ProjectPicker => app.open_picker(),
                Action::CloseInstance => {
                    if app.is_on_welcome() {
                        if !app.session_manager.sessions.is_empty() {
                            // Close welcome tab, go to nearest session
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
                        // If no sessions, can't close the only tab
                    } else if !app.session_manager.sessions.is_empty() {
                        app.show_close_confirm = true;
                    }
                }
                Action::TogglePanel => {
                    if app.show_right_panel && app.focus == app::FocusPanel::GitPanel {
                        // Already focused on git panel — hide it
                        app.show_right_panel = false;
                        app.focus = app::FocusPanel::Output;
                    } else if app.show_right_panel {
                        // Panel visible but not focused — focus it
                        app.focus = app::FocusPanel::GitPanel;
                    } else {
                        // Panel hidden — show and focus it
                        app.show_right_panel = true;
                        app.focus = app::FocusPanel::GitPanel;
                    }
                    app.git_status_scroll = 0;
                    app.git_log_scroll = 0;
                    last_resize = (0, 0);
                }
                Action::ForwardChars(chars) => {
                    if let Some(session) = app.session_manager.active_session() {
                        let name = session.name.clone();
                        let _ = tmux::send_keys(&name, &chars);
                        did_forward = true;
                        app.last_input_time = Some(Instant::now());
                        app.follow_mode = true;
                    }
                }
                Action::ForwardSpecial(key) => {
                    if let Some(session) = app.session_manager.active_session() {
                        let name = session.name.clone();
                        let _ = tmux::send_special_key(&name, &key);
                        did_forward = true;
                        app.last_input_time = Some(Instant::now());
                        app.follow_mode = true;
                    }
                }
                Action::ForwardCtrl(c) => {
                    if let Some(session) = app.session_manager.active_session() {
                        let name = session.name.clone();
                        let _ = tmux::send_special_key(&name, &format!("C-{}", c));
                        did_forward = true;
                        app.last_input_time = Some(Instant::now());
                        app.follow_mode = true;
                    }
                }
                Action::EscapeKey => {
                    // If git panel is fullscreen (narrow + showing), close it
                    let size = terminal.size().unwrap_or_default();
                    if app.show_right_panel && size.width < 100 {
                        app.show_right_panel = false;
                        app.focus = app::FocusPanel::Output;
                        last_resize = (0, 0);
                    } else if let Some(session) = app.session_manager.active_session() {
                        let name = session.name.clone();
                        let _ = tmux::send_special_key(&name, "Escape");
                        did_forward = true;
                        app.last_input_time = Some(Instant::now());
                    }
                }
                Action::ScrollUp => {
                    let scroll_git = app.focus == app::FocusPanel::GitPanel && app.show_right_panel;
                    if scroll_git {
                        app.git_status_scroll = app.git_status_scroll.saturating_add(1);
                        app.git_log_scroll = app.git_log_scroll.saturating_add(1);

                        // Lazy load: fetch more log when near bottom
                        let log_lines = app.git_log.lines().count() as u16;
                        if app.git_log_has_more && app.git_log_scroll + 20 >= log_lines {
                            app.git_log_limit += 200;
                            refresh_git_log(&mut app);
                        }
                    } else {
                        app.follow_mode = false;
                        app.scroll_offset = app.scroll_offset.saturating_add(1);
                    }
                }
                Action::ScrollDown => {
                    let scroll_git = app.focus == app::FocusPanel::GitPanel && app.show_right_panel;
                    if scroll_git {
                        app.git_status_scroll = app.git_status_scroll.saturating_sub(1);
                        app.git_log_scroll = app.git_log_scroll.saturating_sub(1);
                    } else {
                        app.scroll_offset = app.scroll_offset.saturating_sub(1);
                        if app.scroll_offset == 0 {
                            app.follow_mode = true;
                        }
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

fn refresh_output(app: &mut App) {
    if let Some(session) = app.session_manager.active_session() {
        let name = session.name.clone();
        if let Ok(output) = tmux::capture_pane(&name) {
            app.claude_output = output;
        }
    }
}

fn refresh_git_status(app: &mut App) {
    if let Some(session) = app.session_manager.active_session() {
        let dir = session.directory.clone();
        if !dir.is_empty() {
            if let Ok(status) = git::status_short(&dir) {
                app.git_status = status;
            }
            if let Ok(branch) = git::current_branch(&dir) {
                app.git_branch = branch;
            }
            app.git_upstream = git::upstream_counts(&dir);
            app.git_diff_stats = git::diff_stats(&dir);
            app.git_remote_branch = git::remote_tracking_branch(&dir).unwrap_or_default();
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

/// Check background (non-active) tabs for significant output changes.
fn check_background_notifications(app: &mut App) {
    let active = app.session_manager.active_index;
    for (i, session) in app.session_manager.sessions.iter_mut().enumerate() {
        if i == active {
            // Active tab: update baseline, never notify
            if let Ok(output) = tmux::capture_pane(&session.name) {
                let trimmed_len = output.trim().len();
                session.last_output_len = trimmed_len;
            }
            continue;
        }

        if let Ok(output) = tmux::capture_pane(&session.name) {
            let trimmed_len = output.trim().len();
            // Significant change: output grew by 50+ chars (ignores cursor blinks etc)
            if trimmed_len > session.last_output_len + 50 {
                session.has_notification = true;
            }
            session.last_output_len = trimmed_len;
        }
    }
}

/// Clear notification on the currently active tab.
fn clear_active_notification(app: &mut App) {
    let idx = app.session_manager.active_index;
    if let Some(session) = app.session_manager.sessions.get_mut(idx) {
        session.has_notification = false;
    }
}
