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
    let output_interval = Duration::from_millis(100);

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
                        app.refresh_data();
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
                    app.session_manager.next_tab();
                    app.scroll_offset = 0;
                    app.follow_mode = true;
                    app.refresh_data();
                    last_resize = (0, 0); // force resize for new session
                }
                Action::PrevTab => {
                    app.session_manager.prev_tab();
                    app.scroll_offset = 0;
                    app.follow_mode = true;
                    app.refresh_data();
                    last_resize = (0, 0);
                }
                Action::NewInstance | Action::ProjectPicker => app.open_picker(),
                Action::CloseInstance => {
                    if !app.session_manager.sessions.is_empty() {
                        app.show_close_confirm = true;
                    }
                }
                Action::TogglePanel => {
                    app.show_right_panel = !app.show_right_panel;
                    last_resize = (0, 0); // viewport size will change
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
                Action::ScrollUp => {
                    if app.focus == app::FocusPanel::GitPanel {
                        app.git_scroll_offset = app.git_scroll_offset.saturating_add(3);
                    } else {
                        app.follow_mode = false;
                        app.scroll_offset = app.scroll_offset.saturating_add(3);
                    }
                }
                Action::ScrollDown => {
                    if app.focus == app::FocusPanel::GitPanel {
                        app.git_scroll_offset = app.git_scroll_offset.saturating_sub(3);
                    } else {
                        app.scroll_offset = app.scroll_offset.saturating_sub(3);
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
        }
    }
}

fn refresh_git_log(app: &mut App) {
    if let Some(session) = app.session_manager.active_session() {
        let dir = session.directory.clone();
        if !dir.is_empty() {
            if let Ok(log) = git::log_oneline(&dir) {
                app.git_log = log;
            }
        }
    }
}
