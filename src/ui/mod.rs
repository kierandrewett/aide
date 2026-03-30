use ansi_to_tui::IntoText;
use unicode_width::UnicodeWidthStr;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Tabs},
    Frame,
};

use crate::app::{App, FocusPanel};

const FOCUSED_BORDER: Color = Color::Cyan;
const UNFOCUSED_BORDER: Color = Color::DarkGray;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.area();
    let is_narrow = size.width < 100;
    let status_height = if is_narrow { 2 } else { 1 };
    let tab_height: u16 = if is_narrow { 2 } else { 3 };

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(status_height)])
        .split(size);

    let content_area = main_chunks[0];
    let status_area = main_chunks[1];

    // Always show tab bar + content; welcome is just the content of a special tab
    let tab_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(tab_height), Constraint::Min(1)])
        .split(content_area);

    draw_tabs(frame, app, tab_chunks[0], is_narrow);
    let body_area = tab_chunks[1];

    if app.show_welcome || app.session_manager.sessions.is_empty() {
        draw_splash(frame, app, body_area);
        draw_status_bar(frame, app, status_area);
        if app.show_picker {
            draw_picker(frame, app, size);
        }
        return;
    }

    if app.show_right_panel && is_narrow {
        draw_right_panel(frame, app, body_area, is_narrow);
    } else if app.show_right_panel {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(body_area);

        draw_claude_output(frame, app, h_chunks[0], is_narrow);
        draw_right_panel(frame, app, h_chunks[1], is_narrow);
    } else {
        draw_claude_output(frame, app, body_area, is_narrow);
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

fn draw_splash(frame: &mut Frame, app: &App, area: Rect) {
    let typing_indicator = if app.is_typing() { " ●" } else { "" };

    // Big logo needs ~40 cols wide and ~14 rows tall (logo + subtitle + hints + padding)
    let use_big_logo = area.width >= 45 && area.height >= 14;

    let mut lines: Vec<Line> = Vec::new();

    if use_big_logo {
        let logo = vec![
            "",
            "        ██████╗ ██╗██████╗ ███████╗",
            "       ██╔══██╗██║██╔══██╗██╔════╝",
            "       ███████║██║██║  ██║█████╗  ",
            "       ██╔══██║██║██║  ██║██╔══╝  ",
            "       ██║  ██║██║██████╔╝███████╗",
            "       ╚═╝  ╚═╝╚═╝╚═════╝ ╚══════╝",
            "",
        ];

        let content_height = logo.len() + 4; // logo + subtitle + blank + hints + blank
        let v_pad = (area.height as usize).saturating_sub(content_height) / 2;
        for _ in 0..v_pad {
            lines.push(Line::from(""));
        }

        for l in &logo {
            lines.push(Line::from(Span::styled(
                *l,
                Style::default().fg(Color::Cyan),
            )));
        }
    } else {
        // Compact: just the name, centered vertically
        let content_height: usize = 5; // title + blank + subtitle + blank + hints
        let v_pad = (area.height as usize).saturating_sub(content_height) / 2;
        for _ in 0..v_pad {
            lines.push(Line::from(""));
        }

        lines.push(Line::from(Span::styled(
            "  ── aide ──",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(Span::styled(
        format!("  Terminal IDE for Claude Code{}", typing_indicator),
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "  Ctrl+P ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("select project   ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "Ctrl+X ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled("quit", Style::default().fg(Color::DarkGray)),
    ]));

    let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

fn draw_tabs(frame: &mut Frame, app: &App, area: Rect, is_narrow: bool) {
    let is_focused = app.focus == FocusPanel::Output;
    let welcome_showing = app.show_welcome || app.session_manager.sessions.is_empty();

    let mut titles: Vec<Line> = app
        .session_manager
        .sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let is_active = !welcome_showing && i == app.session_manager.active_index;
            let style = if is_active {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let label = if s.has_notification && !is_active {
                format!("🔔 {}", s.name)
            } else {
                s.name.clone()
            };
            Line::from(Span::styled(label, style))
        })
        .collect();

    // Add the welcome tab
    if welcome_showing {
        titles.push(Line::from(Span::styled(
            "aide",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )));
    }

    // Selected index: welcome tab is at the end
    let selected = if welcome_showing {
        titles.len() - 1
    } else {
        app.session_manager.active_index
    };

    let border_color = if is_focused {
        FOCUSED_BORDER
    } else {
        UNFOCUSED_BORDER
    };

    let block = if is_narrow {
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(border_color))
    } else {
        focused_block(" Sessions ", is_focused)
    };

    let divider = if is_narrow { " │ " } else { " | " };

    let tabs = Tabs::new(titles)
        .block(block)
        .select(selected)
        .divider(Span::styled(divider, Style::default().fg(Color::DarkGray)))
        .highlight_style(
            Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

fn draw_claude_output(frame: &mut Frame, app: &mut App, area: Rect, is_narrow: bool) {
    let is_focused = app.focus == FocusPanel::Output;

    let border_h: u16 = if is_narrow { 0 } else { 2 };
    let border_w: u16 = if is_narrow { 0 } else { 2 };
    let inner_width = area.width.saturating_sub(border_w);
    let inner_height = area.height.saturating_sub(border_h);
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

    if app.scroll_offset > max_scroll_back {
        app.scroll_offset = max_scroll_back;
    }

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

    let block = if is_narrow {
        Block::default()
    } else {
        focused_block(title, is_focused)
    };

    let paragraph = Paragraph::new(text).block(block).scroll((top_offset, 0));

    frame.render_widget(paragraph, area);
}

fn draw_right_panel(frame: &mut Frame, app: &mut App, area: Rect, is_narrow: bool) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_git_status(frame, app, chunks[0], is_narrow);
    draw_git_log(frame, app, chunks[1], is_narrow);
}

fn git_panel_block<'a>(title: &'a str, is_focused: bool, is_narrow: bool) -> Block<'a> {
    let border_color = if is_focused {
        FOCUSED_BORDER
    } else {
        UNFOCUSED_BORDER
    };
    if is_narrow {
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(title, Style::default().fg(border_color)))
    } else {
        focused_block(title, is_focused)
    }
}

fn draw_git_status(frame: &mut Frame, app: &mut App, area: Rect, is_narrow: bool) {
    let is_focused = app.focus == FocusPanel::GitPanel;

    let mut lines: Vec<Line> = Vec::new();

    // Branch header with local → remote
    let branch_line = if app.git_remote_branch.is_empty() {
        Line::from(vec![
            Span::styled(" ⎇ ", Style::default().fg(Color::Cyan)),
            Span::styled(
                &app.git_branch,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  (no upstream)", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        let (behind, ahead) = app.git_upstream.unwrap_or((0, 0));
        let sync_icon = if behind == 0 && ahead == 0 {
            Span::styled(" ✓", Style::default().fg(Color::Green))
        } else {
            let mut parts = String::new();
            if behind > 0 {
                parts.push_str(&format!(" ↓{}", behind));
            }
            if ahead > 0 {
                parts.push_str(&format!(" ↑{}", ahead));
            }
            Span::styled(parts, Style::default().fg(Color::Yellow))
        };
        Line::from(vec![
            Span::styled(" ⎇ ", Style::default().fg(Color::Cyan)),
            Span::styled(
                &app.git_branch,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" → ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.git_remote_branch, Style::default().fg(Color::Blue)),
            sync_icon,
        ])
    };
    lines.push(branch_line);
    lines.push(Line::from(""));

    // Parse status lines with icons
    for line in app.git_status.lines() {
        if line.starts_with("##") {
            continue; // skip branch line, we render our own
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Parse the two-char status code
        let (index_status, worktree_status) = if line.len() >= 2 {
            (
                line.chars().next().unwrap_or(' '),
                line.chars().nth(1).unwrap_or(' '),
            )
        } else {
            (' ', ' ')
        };

        let filename = if line.len() > 3 { &line[3..] } else { trimmed };

        let (icon, color) = match (index_status, worktree_status) {
            ('?', '?') => ("  ？ ", Color::DarkGray),       // untracked
            ('A', _) | (_, 'A') => ("  ＋ ", Color::Green), // added
            ('D', _) | (_, 'D') => ("  ✕ ", Color::Red),    // deleted
            ('R', _) => ("  ➜ ", Color::Magenta),           // renamed
            ('M', _) => ("  ● ", Color::Green),             // staged modified
            (_, 'M') => ("  ○ ", Color::Yellow),            // unstaged modified
            ('C', _) => ("  ⊕ ", Color::Cyan),              // copied
            _ => ("  ∙ ", Color::White),
        };

        let staged_marker = match index_status {
            'M' | 'A' | 'D' | 'R' | 'C' => Span::styled(" ✓", Style::default().fg(Color::Green)),
            _ => Span::raw(""),
        };

        lines.push(Line::from(vec![
            Span::styled(icon, Style::default().fg(color)),
            Span::styled(filename.to_string(), Style::default().fg(color)),
            staged_marker,
        ]));
    }

    if lines.len() <= 2 {
        lines.push(Line::from(Span::styled(
            "  ✓ Working tree clean",
            Style::default().fg(Color::Green),
        )));
    }

    let border_overhead: u16 = if is_narrow { 1 } else { 2 };
    let total = lines.len() as u16;
    let visible = area.height.saturating_sub(border_overhead);
    let max_scroll = total.saturating_sub(visible);
    app.git_status_scroll = app.git_status_scroll.min(max_scroll);

    let at_top = app.git_status_scroll == 0;
    let at_bottom = app.git_status_scroll >= max_scroll;
    let scroll_hint = if max_scroll == 0 {
        ""
    } else if at_top {
        " ↓ more "
    } else if at_bottom {
        " ↑ more "
    } else {
        " ↑↓ "
    };
    let title = format!(" 📋 Status{}", scroll_hint);
    let block = git_panel_block(&title, is_focused, is_narrow);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((app.git_status_scroll, 0));

    frame.render_widget(paragraph, area);
}

fn draw_git_log(frame: &mut Frame, app: &mut App, area: Rect, is_narrow: bool) {
    let is_focused = app.focus == FocusPanel::GitPanel;

    let mut lines: Vec<Line> = Vec::new();

    for line in app.git_log.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        // Extract the graph prefix (*, |, /, \, spaces) from the rest
        let mut graph_end = 0;
        for (i, ch) in line.char_indices() {
            if matches!(ch, '*' | '|' | '/' | '\\' | ' ') {
                graph_end = i + ch.len_utf8();
            } else {
                break;
            }
        }

        let graph_part = &line[..graph_end];
        let rest = &line[graph_end..];

        let mut spans: Vec<Span> = Vec::new();

        let graph_colored = graph_part.to_string();
        spans.push(Span::styled(
            graph_colored,
            Style::default().fg(Color::Blue),
        ));

        if rest.is_empty() {
            lines.push(Line::from(spans));
            continue;
        }

        // Parse: hash (decoration) message (time)
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.is_empty() {
            spans.push(Span::raw(rest.to_string()));
            lines.push(Line::from(spans));
            continue;
        }

        // Hash
        let hash = parts[0];
        spans.push(Span::styled(
            hash.to_string(),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));

        if parts.len() < 2 {
            lines.push(Line::from(spans));
            continue;
        }

        let remainder = parts[1];

        // Check for decoration (refs) like (HEAD -> main, origin/main)
        if remainder.starts_with('(') {
            if let Some(close) = remainder.find(')') {
                let decoration = &remainder[1..close];
                spans.push(Span::styled(" (", Style::default().fg(Color::DarkGray)));

                // Parse individual refs
                for (j, ref_name) in decoration.split(", ").enumerate() {
                    if j > 0 {
                        spans.push(Span::styled(", ", Style::default().fg(Color::DarkGray)));
                    }
                    let ref_name = ref_name.trim();
                    if ref_name.starts_with("HEAD") {
                        spans.push(Span::styled(
                            ref_name.to_string(),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else if ref_name.starts_with("origin/") || ref_name.starts_with("upstream/") {
                        spans.push(Span::styled(
                            ref_name.to_string(),
                            Style::default().fg(Color::Red),
                        ));
                    } else if ref_name.starts_with("tag:") {
                        spans.push(Span::styled(
                            ref_name.to_string(),
                            Style::default()
                                .fg(Color::Magenta)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        spans.push(Span::styled(
                            ref_name.to_string(),
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        ));
                    }
                }
                spans.push(Span::styled(") ", Style::default().fg(Color::DarkGray)));

                // Message + time after decoration
                let after_dec = &remainder[close + 1..].trim_start();
                if let Some(time_start) = after_dec.rfind('(') {
                    let msg = &after_dec[..time_start].trim_end();
                    let time = &after_dec[time_start..];
                    spans.push(Span::styled(
                        msg.to_string(),
                        Style::default().fg(Color::White),
                    ));
                    spans.push(Span::styled(
                        format!(" {}", time),
                        Style::default().fg(Color::DarkGray),
                    ));
                } else {
                    spans.push(Span::styled(
                        after_dec.to_string(),
                        Style::default().fg(Color::White),
                    ));
                }
            } else {
                spans.push(Span::styled(
                    format!(" {}", remainder),
                    Style::default().fg(Color::White),
                ));
            }
        } else {
            // No decoration — message (time)
            if let Some(time_start) = remainder.rfind('(') {
                let msg = &remainder[..time_start].trim_end();
                let time = &remainder[time_start..];
                spans.push(Span::styled(
                    format!(" {}", msg),
                    Style::default().fg(Color::White),
                ));
                spans.push(Span::styled(
                    format!(" {}", time),
                    Style::default().fg(Color::DarkGray),
                ));
            } else {
                spans.push(Span::styled(
                    format!(" {}", remainder),
                    Style::default().fg(Color::White),
                ));
            }
        }

        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No commits yet",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let border_overhead: u16 = if is_narrow { 1 } else { 2 };
    let total = lines.len() as u16;
    let visible = area.height.saturating_sub(border_overhead);
    let max_scroll = total.saturating_sub(visible);
    app.git_log_scroll = app.git_log_scroll.min(max_scroll);

    let at_top = app.git_log_scroll == 0;
    let at_bottom = app.git_log_scroll >= max_scroll;
    let scroll_hint = if max_scroll == 0 {
        ""
    } else if at_top {
        " ↓ more "
    } else if at_bottom && app.git_log_has_more {
        " loading... "
    } else if at_bottom {
        " ── end ── "
    } else {
        " ↑↓ "
    };
    let title = format!(" 📜 Log{}", scroll_hint);
    let block = git_panel_block(&title, is_focused, is_narrow);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((app.git_log_scroll, 0));

    frame.render_widget(paragraph, area);
}

fn tilde_path(path: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() && path.starts_with(&home) {
        format!("~{}", &path[home.len()..])
    } else {
        path.to_string()
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let (directory, session_name) = if let Some(s) = app.session_manager.active_session() {
        (tilde_path(&s.directory), s.name.as_str())
    } else {
        ("~".to_string(), "aide")
    };

    let branch = if app.git_branch.is_empty() {
        "—".to_string()
    } else {
        app.git_branch.clone()
    };

    // Always show upstream counts with aligned arrows
    let (behind, ahead) = app.git_upstream.unwrap_or((0, 0));
    let upstream_text = format!("↓{} ↑{}", behind, ahead);

    // Diff stats: +added -deleted
    let diff_text = match app.git_diff_stats {
        Some((added, deleted)) => format!("+{} -{}", added, deleted),
        None => "+0 -0".to_string(),
    };

    let typing_indicator = if app.is_typing() { " ●" } else { "" };

    let w = area.width as usize;
    let is_narrow = area.height >= 2;

    let left_text = format!(" {}{} ", directory, typing_indicator);
    let git_text = format!(
        " {} {} {} {} ",
        branch, upstream_text, diff_text, session_name
    );

    // Keybinds with Tab/Shift+Tab
    let hints = if app.focus == FocusPanel::GitPanel && app.show_right_panel {
        "^G back  ↑↓ scroll  ^X exit "
    } else if app.session_manager.sessions.is_empty() {
        "^T new  ^P pick  ^X exit "
    } else {
        "Tab/S-Tab switch  ^T new  ^W close  ^G git  ^X exit "
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

    // Clear the entire area behind the dialog with a solid background
    let clear_lines: Vec<Line> = (0..dialog_height)
        .map(|_| {
            Line::from(Span::styled(
                " ".repeat(dialog_width as usize),
                Style::default().bg(Color::Black),
            ))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(clear_lines).style(Style::default().bg(Color::Black)),
        dialog_area,
    );

    let inner_height = dialog_height.saturating_sub(2) as usize;
    let mut lines = vec![
        Line::from(Span::styled(
            format!(" Filter: {}_ ", app.picker_filter),
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
    ];

    let filtered = app.filtered_projects();
    let visible_slots = inner_height.saturating_sub(2);
    let scroll_start = if app.picker_selected >= visible_slots {
        app.picker_selected - visible_slots + 1
    } else {
        0
    };

    for (i, project) in filtered
        .iter()
        .enumerate()
        .skip(scroll_start)
        .take(visible_slots)
    {
        let style = if i == app.picker_selected {
            Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White).bg(Color::Black)
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

    frame.render_widget(paragraph, dialog_area);
}
