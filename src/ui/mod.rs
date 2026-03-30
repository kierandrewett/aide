use ansi_to_tui::IntoText;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::app::App;

pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();
    let show_right = app.show_right_panel && size.width >= 80;

    // Main vertical split: content area + status bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(size);

    let content_area = main_chunks[0];
    let status_area = main_chunks[1];

    if show_right {
        // Horizontal split: left (tabs+output) | right (git panels)
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(content_area);

        draw_left_panel(frame, app, h_chunks[0]);
        draw_right_panel(frame, app, h_chunks[1]);
    } else {
        draw_left_panel(frame, app, content_area);
    }

    draw_status_bar(frame, app, status_area);

    // Draw overlays
    if app.show_close_confirm {
        draw_confirm_dialog(frame, size);
    }

    if app.show_picker {
        draw_picker(frame, app, size);
    }
}

fn draw_left_panel(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    draw_tabs(frame, app, chunks[0]);
    draw_claude_output(frame, app, chunks[1]);
}

fn draw_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = app
        .session_manager
        .sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let style = if i == app.session_manager.active_index {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            Line::from(Span::styled(&s.name, style))
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title(" Sessions "))
        .select(app.session_manager.active_index)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

fn draw_claude_output(frame: &mut Frame, app: &App, area: Rect) {
    let text = app
        .claude_output
        .as_bytes()
        .to_vec()
        .into_text()
        .unwrap_or_else(|_| Text::raw(&app.claude_output));
    let total_lines = text.lines.len() as u16;
    let visible_height = area.height.saturating_sub(2);
    let max_scroll = total_lines.saturating_sub(visible_height);
    let scroll = app.scroll_offset.min(max_scroll);

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Output "))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}

fn draw_right_panel(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_git_status(frame, app, chunks[0]);
    draw_git_log(frame, app, chunks[1]);
}

fn draw_git_status(frame: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = app
        .git_status
        .lines()
        .map(|line| {
            let color = if line.starts_with("##") {
                Color::Cyan
            } else if line.starts_with(" M") || line.starts_with("M ") {
                Color::Yellow
            } else if line.starts_with(" A") || line.starts_with("A ") {
                Color::Green
            } else if line.starts_with(" D") || line.starts_with("D ") {
                Color::Red
            } else if line.starts_with("??") {
                Color::DarkGray
            } else {
                Color::White
            };
            Line::from(Span::styled(line.to_string(), Style::default().fg(color)))
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Git Status "))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn draw_git_log(frame: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = app
        .git_log
        .lines()
        .map(|line| {
            let color = if line.contains('*') {
                Color::Green
            } else {
                Color::White
            };
            Line::from(Span::styled(line.to_string(), Style::default().fg(color)))
        })
        .collect();

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Git Log "))
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, area);
}

fn shorten_path(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = if !home.is_empty() && path.starts_with(&home) {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_string()
    };

    // Shorten intermediate directories to first char: ~/dev/myproject -> ~/d/myproject
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 2 {
        return path;
    }
    let last = parts.last().unwrap();
    let shortened: Vec<String> = parts[..parts.len() - 1]
        .iter()
        .map(|p| {
            if p.is_empty() || *p == "~" {
                p.to_string()
            } else {
                p.chars().next().unwrap().to_string()
            }
        })
        .collect();
    format!("{}/{}", shortened.join("/"), last)
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let (directory, session_name) = if let Some(s) = app.session_manager.active_session() {
        (shorten_path(&s.directory), s.name.as_str())
    } else {
        ("~".to_string(), "no session")
    };

    let branch = if app.git_branch.is_empty() {
        "—".to_string()
    } else {
        app.git_branch.clone()
    };

    let upstream = match app.git_upstream {
        Some((behind, ahead)) => {
            let mut parts = Vec::new();
            if behind > 0 {
                parts.push(format!("⇣{}", behind));
            }
            if ahead > 0 {
                parts.push(format!("⇡{}", ahead));
            }
            if parts.is_empty() {
                "✓".to_string()
            } else {
                parts.join(" ")
            }
        }
        None => String::new(),
    };

    let total_width = area.width as usize;

    let left = format!(" {} {} ", directory, session_name);
    let right_parts = format!(" {} {} ^T new ^W close ^G panel ^X exit ", branch, upstream);

    let padding = total_width
        .saturating_sub(left.len())
        .saturating_sub(right_parts.len());

    let bar = Line::from(vec![
        Span::styled(
            left,
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".repeat(padding), Style::default().bg(Color::DarkGray)),
        Span::styled(
            format!(" {} {} ", branch, upstream),
            Style::default()
                .fg(Color::White)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "^T new ^W close ^G panel ^X exit ",
            Style::default().fg(Color::Gray).bg(Color::DarkGray),
        ),
    ]);

    let paragraph = Paragraph::new(bar);
    frame.render_widget(paragraph, area);
}

fn draw_confirm_dialog(frame: &mut Frame, area: Rect) {
    let dialog_width = 40u16;
    let dialog_height = 5u16;
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            " Close this session? (y/n) ",
            Style::default().fg(Color::Yellow),
        )),
    ];

    let paragraph = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Confirm ")
            .style(Style::default().bg(Color::Black).fg(Color::White)),
    );

    // Clear area first
    let clear = Paragraph::new("").style(Style::default().bg(Color::Black));
    frame.render_widget(clear, dialog_area);
    frame.render_widget(paragraph, dialog_area);
}

fn draw_picker(frame: &mut Frame, app: &App, area: Rect) {
    let dialog_width = 60u16.min(area.width.saturating_sub(4));
    let dialog_height = 20u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    let mut lines = vec![
        Line::from(Span::styled(
            format!(" Filter: {}_ ", app.picker_filter),
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
    ];

    for (i, project) in app.filtered_projects().iter().enumerate() {
        let style = if i == app.picker_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        let prefix = if i == app.picker_selected {
            " ▸ "
        } else {
            "   "
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, project),
            style,
        )));
    }

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Select Project ")
            .style(Style::default().bg(Color::Black).fg(Color::White)),
    );

    let clear = Paragraph::new("").style(Style::default().bg(Color::Black));
    frame.render_widget(clear, dialog_area);
    frame.render_widget(paragraph, dialog_area);
}
