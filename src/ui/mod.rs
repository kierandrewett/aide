use ansi_to_tui::IntoText;
use unicode_width::UnicodeWidthStr;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::app::{App, FocusPanel};

const FOCUSED_BORDER: Color = Color::Cyan;
const UNFOCUSED_BORDER: Color = Color::DarkGray;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.area();
    let is_narrow = size.width < 100;
    let status_height = if is_narrow { 2 } else { 1 };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(status_height)])
        .split(size);

    let content_area = main_chunks[0];
    let status_area = main_chunks[1];

    if app.show_right_panel && is_narrow {
        // Narrow: git panel fullscreen
        app.focus = FocusPanel::GitPanel;
        draw_right_panel(frame, app, content_area);
    } else if app.show_right_panel {
        // Wide: side-by-side
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(content_area);

        draw_left_panel(frame, app, h_chunks[0]);
        draw_right_panel(frame, app, h_chunks[1]);
    } else {
        app.focus = FocusPanel::Output;
        draw_left_panel(frame, app, content_area);
    }

    draw_status_bar(frame, app, status_area);

    if app.show_close_confirm {
        draw_confirm_dialog(frame, size);
    }
    if app.show_picker {
        draw_picker(frame, app, size);
    }
}

fn focused_block(title: &str, focused: bool) -> Block<'_> {
    let border_color = if focused {
        FOCUSED_BORDER
    } else {
        UNFOCUSED_BORDER
    };
    let title_style = if focused {
        Style::default()
            .fg(FOCUSED_BORDER)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(UNFOCUSED_BORDER)
    };
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(title, title_style))
}

fn draw_left_panel(frame: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    draw_tabs(frame, app, chunks[0]);
    draw_claude_output(frame, app, chunks[1]);
}

fn draw_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focus == FocusPanel::Output;
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
        .block(focused_block(" Sessions ", is_focused))
        .select(app.session_manager.active_index)
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

fn draw_claude_output(frame: &mut Frame, app: &mut App, area: Rect) {
    let is_focused = app.focus == FocusPanel::Output;

    // Track viewport size for tmux resize
    let inner_width = area.width.saturating_sub(2);
    let inner_height = area.height.saturating_sub(2);
    app.output_width = inner_width;
    app.output_height = inner_height;

    let text = app
        .claude_output
        .as_bytes()
        .to_vec()
        .into_text()
        .unwrap_or_else(|_| Text::raw(&app.claude_output));

    let total_lines = text.lines.len() as u16;
    let max_scroll_back = total_lines.saturating_sub(inner_height);

    // Clamp scroll_offset to valid range
    if app.scroll_offset > max_scroll_back {
        app.scroll_offset = max_scroll_back;
    }

    // scroll_offset = lines scrolled back from bottom
    // Convert to top-offset for Paragraph::scroll()
    let top_offset = if app.follow_mode {
        max_scroll_back
    } else {
        max_scroll_back.saturating_sub(app.scroll_offset)
    };

    let title = if app.is_typing() {
        " Output  ● "
    } else if !app.follow_mode {
        " Output  ↑ scroll "
    } else {
        " Output "
    };

    let paragraph = Paragraph::new(text)
        .block(focused_block(title, is_focused))
        .scroll((top_offset, 0));

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
    let is_focused = app.focus == FocusPanel::GitPanel;
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

    let total = lines.len() as u16;
    let visible = area.height.saturating_sub(2);
    let max_scroll = total.saturating_sub(visible);
    let scroll = app.git_scroll_offset.min(max_scroll);

    let paragraph = Paragraph::new(lines)
        .block(focused_block(" Git Status ", is_focused))
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);
}

fn draw_git_log(frame: &mut Frame, app: &App, area: Rect) {
    let is_focused = app.focus == FocusPanel::GitPanel;
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
        .block(focused_block(" Git Log ", is_focused))
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

    let typing_indicator = if app.is_typing() { " ●" } else { "" };

    let w = area.width as usize;
    let is_narrow = area.height >= 2;

    let left_text = format!(" {} {}{} ", directory, session_name, typing_indicator);
    let git_text = format!(" {} {} ", branch, upstream);

    // Contextual hints based on focused panel
    let hints = if app.focus == FocusPanel::GitPanel && app.show_right_panel {
        "^G back  Scroll ↑↓  ^X exit "
    } else {
        "^T new ^W close ^G git ^X exit "
    };

    let left_w = left_text.width();
    let git_w = git_text.width();
    let hints_w = hints.width();

    if is_narrow {
        let line1_pad = w.saturating_sub(left_w).saturating_sub(git_w);
        let line1 = Line::from(vec![
            Span::styled(
                &left_text,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ".repeat(line1_pad), Style::default().bg(Color::DarkGray)),
            Span::styled(
                &git_text,
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let line2_pad = w.saturating_sub(hints_w);
        let line2 = Line::from(vec![
            Span::styled(" ".repeat(line2_pad), Style::default().bg(Color::DarkGray)),
            Span::styled(hints, Style::default().fg(Color::Gray).bg(Color::DarkGray)),
        ]);

        let text = Text::from(vec![line1, line2]);
        frame.render_widget(Paragraph::new(text), area);
    } else {
        let padding = w
            .saturating_sub(left_w)
            .saturating_sub(git_w)
            .saturating_sub(hints_w);

        let bar = Line::from(vec![
            Span::styled(
                &left_text,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ".repeat(padding), Style::default().bg(Color::DarkGray)),
            Span::styled(
                &git_text,
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(hints, Style::default().fg(Color::Gray).bg(Color::DarkGray)),
        ]);

        frame.render_widget(Paragraph::new(bar), area);
    }
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
            .border_style(Style::default().fg(FOCUSED_BORDER))
            .title(Span::styled(
                " Select Project ",
                Style::default()
                    .fg(FOCUSED_BORDER)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(Color::Black).fg(Color::White)),
    );

    let clear = Paragraph::new("").style(Style::default().bg(Color::Black));
    frame.render_widget(clear, dialog_area);
    frame.render_widget(paragraph, dialog_area);
}
