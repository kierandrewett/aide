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
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use ratatui::prelude::*;

use app::App;
use input::Action;

fn main() -> Result<()> {
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal);

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

    let tick_rate = Duration::from_millis(100);
    let mut last_git_status_refresh = Instant::now();
    let mut last_git_log_refresh = Instant::now();
    let mut last_output_refresh = Instant::now();

    let git_status_interval = Duration::from_secs(2);
    let git_log_interval = Duration::from_secs(3);
    let output_interval = Duration::from_millis(500);

    loop {
        terminal.draw(|frame| ui::draw(frame, &app))?;

        let now = Instant::now();

        if now.duration_since(last_output_refresh) >= output_interval {
            if let Some(session) = app.session_manager.active_session() {
                let name = session.name.clone();
                if let Ok(output) = crate::tmux::capture_pane(&name) {
                    app.claude_output = output;
                }
            }
            last_output_refresh = now;
        }

        if now.duration_since(last_git_status_refresh) >= git_status_interval {
            if let Some(session) = app.session_manager.active_session() {
                let dir = session.directory.clone();
                if !dir.is_empty() {
                    if let Ok(status) = crate::git::status_short(&dir) {
                        app.git_status = status;
                    }
                    if let Ok(branch) = crate::git::current_branch(&dir) {
                        app.git_branch = branch;
                    }
                    app.git_upstream = crate::git::upstream_counts(&dir);
                }
            }
            last_git_status_refresh = now;
        }

        if now.duration_since(last_git_log_refresh) >= git_log_interval {
            if let Some(session) = app.session_manager.active_session() {
                let dir = session.directory.clone();
                if !dir.is_empty() {
                    if let Ok(log) = crate::git::log_oneline(&dir) {
                        app.git_log = log;
                    }
                }
            }
            last_git_log_refresh = now;
        }

        let action = input::poll_action(tick_rate);

        if app.show_close_confirm {
            match action {
                Action::Char('y') | Action::Char('Y') => {
                    app.show_close_confirm = false;
                    let idx = app.session_manager.active_index;
                    app.session_manager.close_session(idx)?;
                    app.refresh_data();
                }
                Action::Char('n') | Action::Char('N') | Action::Cancel => {
                    app.show_close_confirm = false;
                }
                _ => {}
            }
            continue;
        }

        if app.show_picker {
            match action {
                Action::Confirm => {
                    app.picker_select_confirm()?;
                }
                Action::Cancel => {
                    app.close_picker();
                }
                Action::Char(c) => {
                    app.picker_filter.push(c);
                    app.picker_selected = 0;
                }
                Action::Backspace => {
                    app.picker_filter.pop();
                    app.picker_selected = 0;
                }
                Action::ScrollDown | Action::NextTab => {
                    app.picker_move_down();
                }
                Action::ScrollUp | Action::PrevTab => {
                    app.picker_move_up();
                }
                Action::Exit => {
                    app.close_picker();
                }
                _ => {}
            }
            continue;
        }

        match action {
            Action::Exit => {
                app.should_quit = true;
            }
            Action::NextTab => {
                app.session_manager.next_tab();
                app.scroll_offset = 0;
                app.refresh_data();
            }
            Action::PrevTab => {
                app.session_manager.prev_tab();
                app.scroll_offset = 0;
                app.refresh_data();
            }
            Action::NewInstance => {
                app.open_picker();
            }
            Action::ProjectPicker => {
                app.open_picker();
            }
            Action::CloseInstance => {
                if !app.session_manager.sessions.is_empty() {
                    app.show_close_confirm = true;
                }
            }
            Action::TogglePanel => {
                app.show_right_panel = !app.show_right_panel;
            }
            Action::ScrollUp => {
                app.scroll_offset = app.scroll_offset.saturating_sub(3);
            }
            Action::ScrollDown => {
                app.scroll_offset = app.scroll_offset.saturating_add(3);
            }
            Action::None => {}
            _ => {}
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
