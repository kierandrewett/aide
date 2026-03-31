use ansi_to_tui::IntoText;
use unicode_width::UnicodeWidthStr;

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph},
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

    app.tab_bar_area = tab_chunks[0];
    draw_tabs(frame, app, tab_chunks[0], is_narrow);
    let body_area = tab_chunks[1];

    if app.is_on_welcome() {
        draw_splash(frame, app, body_area);
        draw_status_bar(frame, app, status_area);
        if app.show_picker {
            draw_picker(frame, app, size);
        }
        return;
    }

    if app.show_right_panel && is_narrow {
        app.output_area = Rect::default();
        app.git_panel_area = body_area;
        draw_right_panel(frame, app, body_area, is_narrow);
    } else if app.show_right_panel {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(body_area);

        app.output_area = h_chunks[0];
        app.git_panel_area = h_chunks[1];
        draw_claude_output(frame, app, h_chunks[0], is_narrow);
        draw_right_panel(frame, app, h_chunks[1], is_narrow);
    } else {
        app.output_area = body_area;
        app.git_panel_area = Rect::default();
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

fn draw_tabs(frame: &mut Frame, app: &mut App, area: Rect, is_narrow: bool) {
    let is_focused = app.focus == FocusPanel::Output;
    let on_welcome = app.is_on_welcome();

    let mut titles: Vec<Line> = app
        .session_manager
        .sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let is_active = !on_welcome && i == app.session_manager.active_index;
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

    // Add the welcome tab if it exists
    if app.show_welcome || app.session_manager.sessions.is_empty() {
        let style = if on_welcome {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        titles.push(Line::from(Span::styled("aide", style)));
    }

    let selected = if on_welcome {
        titles.len().saturating_sub(1)
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
    let divider_w = divider.width();

    // Calculate available width inside the block (subtract borders)
    let inner_w = if is_narrow {
        area.width as usize
    } else {
        area.width.saturating_sub(2) as usize // side borders
    };

    // Compute each tab's display width (including " name " padding)
    let tab_widths: Vec<usize> = titles.iter().map(|t| t.width() + 2).collect();

    // Stable scroll offset — only shift when selected tab is out of view
    let arrow_w = 2; // "◀ " or " ▶"
    let total_w: usize =
        tab_widths.iter().sum::<usize>() + titles.len().saturating_sub(1) * divider_w;

    let mut start = app.tab_scroll_offset;
    #[allow(unused_assignments)]
    let mut end = titles.len();

    let needs_overflow = total_w > inner_w;

    if needs_overflow && !titles.is_empty() {
        // Ensure selected tab is visible by adjusting scroll offset
        if selected < start {
            start = selected;
        }

        // Find how many tabs fit from `start`
        end = start;
        let mut used = 0usize;
        #[allow(clippy::needless_range_loop)]
        for i in start..titles.len() {
            let left_space = if i > 0 && start > 0 { arrow_w } else { 0 };
            let right_space = arrow_w; // assume more to the right
            let budget = inner_w.saturating_sub(left_space + right_space);

            let cost = if i == start {
                tab_widths[i]
            } else {
                divider_w + tab_widths[i]
            };
            if used + cost > budget && i > selected {
                break;
            }
            used += cost;
            end = i + 1;
        }

        // If selected is past the visible end, shift start right
        if selected >= end {
            end = selected + 1;
            used = tab_widths[selected];
            start = selected;
            // Expand left to fill
            while start > 0 {
                let left_space = if start - 1 > 0 { arrow_w } else { 0 };
                let right_space = if end < titles.len() { arrow_w } else { 0 };
                let budget = inner_w.saturating_sub(left_space + right_space);
                let cost = divider_w + tab_widths[start - 1];
                if used + cost > budget {
                    break;
                }
                used += cost;
                start -= 1;
            }
        }

        // Recalculate: if we're at the end, no right arrow needed — try fitting more
        if end >= titles.len() {
            let right_space = 0;
            let left_space = if start > 0 { arrow_w } else { 0 };
            let budget = inner_w.saturating_sub(left_space + right_space);
            let mut recalc_used: usize = tab_widths[start..end].iter().sum::<usize>()
                + (end - start).saturating_sub(1) * divider_w;
            while start > 0 {
                let new_left = if start - 1 > 0 { arrow_w } else { 0 };
                let new_budget = inner_w.saturating_sub(new_left);
                let cost = divider_w + tab_widths[start - 1];
                if recalc_used + cost > new_budget {
                    break;
                }
                recalc_used += cost;
                start -= 1;
            }
            let _ = budget; // used above
        }

        // Update the persistent scroll offset
        app.tab_scroll_offset = start;
    } else {
        start = 0;
        end = titles.len();
        app.tab_scroll_offset = 0;
    }

    let has_left = start > 0;
    let has_right = end < titles.len();

    // Build visible titles with adjusted selected index
    let visible_titles: Vec<Line> = titles[start..end].to_vec();
    let visible_selected = selected.saturating_sub(start);

    // Render manually with overflow indicators
    let mut spans: Vec<Span> = Vec::new();
    let mut tab_click_zones: Vec<(u16, u16, usize)> = Vec::new();

    // Track x position (account for block border on non-narrow)
    let mut cursor_x = area.x + if is_narrow { 0 } else { 1 };

    if has_left {
        spans.push(Span::styled("◀ ", Style::default().fg(Color::DarkGray)));
        cursor_x += 2;
    }

    for (i, title) in visible_titles.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(divider, Style::default().fg(Color::DarkGray)));
            cursor_x += divider_w as u16;
        }
        let is_sel = i == visible_selected;
        let text = title
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>();
        let tab_text = format!(" {} ", text);
        let tab_w = tab_text.width() as u16;

        // Record click zone: (x_start, x_end, tab_index)
        let tab_index = start + i;
        tab_click_zones.push((cursor_x, cursor_x + tab_w, tab_index));

        if is_sel {
            spans.push(Span::styled(
                tab_text,
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            let style = title.spans.first().map(|s| s.style).unwrap_or_default();
            spans.push(Span::styled(tab_text, style));
        }
        cursor_x += tab_w;
    }

    if has_right {
        spans.push(Span::styled(" ▶", Style::default().fg(Color::DarkGray)));
    }

    app.tab_click_zones = tab_click_zones;

    let tab_line = Line::from(spans);
    let paragraph = Paragraph::new(tab_line).block(block);
    frame.render_widget(paragraph, area);
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
        " Output  ● ".to_string()
    } else if !app.follow_mode && max_scroll_back > 0 {
        let pct = ((max_scroll_back - app.scroll_offset.min(max_scroll_back)) as f32
            / max_scroll_back as f32
            * 100.0) as u16;
        format!(" Output  ↑{} ({}%) ", app.scroll_offset, pct)
    } else {
        " Output ".to_string()
    };

    let block = if is_narrow {
        Block::default()
    } else {
        focused_block(&title, is_focused)
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
    let on_splash = app.is_on_welcome();

    let w = area.width as usize;
    let is_narrow = area.height >= 2;

    let (left_text, git_text, hints): (String, String, &str) = if on_splash {
        let left = " aide ".to_string();
        let git = String::new();
        let h = if app.session_manager.sessions.is_empty() {
            "^P pick  ^X exit "
        } else {
            "Tab/S-Tab switch  ^P pick  ^W close  ^X exit "
        };
        (left, git, h)
    } else {
        let directory = if let Some(s) = app.session_manager.active_session() {
            tilde_path(&s.directory)
        } else {
            "~".to_string()
        };

        let branch = if app.git_branch.is_empty() {
            "—".to_string()
        } else {
            app.git_branch.clone()
        };

        let (behind, ahead) = app.git_upstream.unwrap_or((0, 0));
        let upstream_text = format!("↓{} ↑{}", behind, ahead);

        let diff_text = match app.git_diff_stats {
            Some((added, deleted)) => format!("+{} -{}", added, deleted),
            None => "+0 -0".to_string(),
        };

        let typing_indicator = if app.is_typing() { " ●" } else { "" };

        let left = format!(" {}{} ", directory, typing_indicator);
        let git = format!(" {} {} {} ", branch, upstream_text, diff_text);

        let h = if app.focus == FocusPanel::GitPanel && app.show_right_panel {
            "^G back  ↑↓ scroll  ^X exit "
        } else {
            "Tab/S-Tab switch  ^T new  ^W close  ^G git  ^X exit "
        };

        (left, git, h)
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

    // Solid background clear
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
            .border_style(Style::default().fg(FOCUSED_BORDER))
            .title(Span::styled(
                " Confirm ",
                Style::default()
                    .fg(FOCUSED_BORDER)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(Color::Black).fg(Color::White)),
    );

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
