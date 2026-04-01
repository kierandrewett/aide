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

/// Render a scrollbar on the right edge of a panel area.
fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    is_narrow: bool,
    scroll_offset: u16,
    max_scroll: u16,
) {
    if max_scroll == 0 || area.height < 3 {
        return;
    }

    let border_top: u16 = if is_narrow { 1 } else { 1 };
    let border_bottom: u16 = if is_narrow { 0 } else { 1 };
    let track_height = area
        .height
        .saturating_sub(border_top + border_bottom)
        .max(1) as usize;
    if track_height < 2 {
        return;
    }

    let total_content = max_scroll + track_height as u16;
    let thumb_size = ((track_height as f64 * track_height as f64) / total_content as f64)
        .ceil()
        .max(1.0)
        .min(track_height as f64) as usize;

    let scrollable = track_height.saturating_sub(thumb_size);
    let thumb_pos = if max_scroll > 0 {
        ((scroll_offset as f64 / max_scroll as f64) * scrollable as f64).round() as usize
    } else {
        0
    };

    let bar_x = area.x + area.width.saturating_sub(1);
    let bar_y_start = area.y + border_top;

    for i in 0..track_height {
        let y = bar_y_start + i as u16;
        if y >= area.y + area.height.saturating_sub(border_bottom) {
            break;
        }
        let ch = if i >= thumb_pos && i < thumb_pos + thumb_size {
            "┃"
        } else {
            "│"
        };
        let style = if i >= thumb_pos && i < thumb_pos + thumb_size {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::Rgb(60, 60, 60))
        };
        let scroll_area = Rect::new(bar_x, y, 1, 1);
        frame.render_widget(Paragraph::new(Span::styled(ch, style)), scroll_area);
    }

    // Scroll position overlay indicator at top-right
    if scroll_offset > 0 {
        let pct = (scroll_offset as f64 / max_scroll as f64 * 100.0) as u16;
        let indicator = format!(" {}% ", pct);
        let ind_w = indicator.width() as u16;
        let ind_x = area.x + area.width.saturating_sub(ind_w + 1);
        let ind_y = area.y;
        if ind_w + 1 < area.width {
            let ind_area = Rect::new(ind_x, ind_y, ind_w, 1);
            frame.render_widget(
                Paragraph::new(Span::styled(
                    indicator,
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                )),
                ind_area,
            );
        }
    }
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let size = frame.area();
    let is_narrow = size.width < 100;
    let status_height = if is_narrow { 2 } else { 1 };
    let tab_height: u16 = 2;

    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(status_height)])
        .split(size);

    let content_area = main_chunks[0];
    let status_area = main_chunks[1];

    let tab_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(tab_height), Constraint::Min(1)])
        .split(content_area);

    app.tab_bar_area = tab_chunks[0];
    draw_tabs(frame, app, tab_chunks[0], is_narrow);
    let body_area = tab_chunks[1];

    if app.is_on_welcome() {
        app.output_area = Rect::default();
        app.git_panel_area = Rect::default();
        app.file_browser_area = Rect::default();
        draw_splash(frame, app, body_area);
        draw_status_bar(frame, app, status_area);
        if app.show_picker || app.show_command_palette {
            draw_command_palette(frame, app, size);
        }
        return;
    }

    // Calculate layout with optional file browser on left
    let file_browser_width: u16 = if app.show_file_browser && !is_narrow {
        (size.width / 5).max(20).min(40)
    } else {
        0
    };

    let main_body = if file_browser_width > 0 {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(file_browser_width),
                Constraint::Min(1),
            ])
            .split(body_area);
        app.file_browser_area = h_chunks[0];
        draw_file_browser(frame, app, h_chunks[0], is_narrow);
        h_chunks[1]
    } else {
        app.file_browser_area = Rect::default();
        body_area
    };

    // File view mode: if a file is open, split between file content and terminal
    if app.viewing_file.is_some() && !is_narrow {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_body);

        draw_file_viewer(frame, app, h_chunks[0], is_narrow);
        app.output_area = h_chunks[1];
        app.git_panel_area = Rect::default();
        draw_claude_output(frame, app, h_chunks[1], is_narrow);
    } else if app.viewing_file.is_some() && is_narrow {
        // Mobile: show file content full-width (toggle with ctrl+f)
        if app.show_file_view {
            draw_file_viewer(frame, app, main_body, is_narrow);
            app.output_area = Rect::default();
            app.git_panel_area = Rect::default();
        } else {
            app.output_area = main_body;
            app.git_panel_area = Rect::default();
            draw_claude_output(frame, app, main_body, is_narrow);
        }
    } else if app.show_right_panel && is_narrow {
        app.output_area = Rect::default();
        app.git_panel_area = main_body;
        draw_right_panel(frame, app, main_body, is_narrow);
    } else if app.show_right_panel {
        let h_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
            .split(main_body);

        app.output_area = h_chunks[0];
        app.git_panel_area = h_chunks[1];
        draw_claude_output(frame, app, h_chunks[0], is_narrow);
        draw_right_panel(frame, app, h_chunks[1], is_narrow);
    } else {
        app.output_area = main_body;
        app.git_panel_area = Rect::default();
        draw_claude_output(frame, app, main_body, is_narrow);
    }

    draw_status_bar(frame, app, status_area);

    // Overlays
    if app.show_close_confirm {
        draw_confirm_dialog(frame, size);
    }
    if app.show_picker || app.show_command_palette {
        draw_command_palette(frame, app, size);
    }

    // File browser overlay on narrow mode
    if app.show_file_browser && is_narrow {
        let overlay_w = (size.width * 3 / 4).min(size.width);
        let overlay_area = Rect::new(0, tab_height, overlay_w, body_area.height);
        app.file_browser_area = overlay_area;
        draw_file_browser(frame, app, overlay_area, is_narrow);
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
        Span::styled("open folder   ", Style::default().fg(Color::DarkGray)),
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

fn draw_tabs(frame: &mut Frame, app: &mut App, area: Rect, _is_narrow: bool) {
    let on_welcome = app.is_on_welcome();

    let mut titles: Vec<(String, bool, bool)> = app
        .session_manager
        .sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let is_active = !on_welcome && i == app.session_manager.active_index;
            let label = if s.has_notification && !is_active {
                format!("* {}", s.name)
            } else {
                s.name.clone()
            };
            (label, is_active, s.has_notification && !is_active)
        })
        .collect();

    if app.show_welcome || app.session_manager.sessions.is_empty() {
        titles.push(("aide".to_string(), on_welcome, false));
    }

    let selected = if on_welcome {
        titles.len().saturating_sub(1)
    } else {
        app.session_manager.active_index
    };

    // Consistent tab bar: bottom border only, no title, dark background
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(Color::Rgb(60, 60, 60)));

    let divider = " ";
    let divider_w = divider.width();

    let inner_w = area.width as usize;
    let tab_widths: Vec<usize> = titles.iter().map(|(label, _, _)| label.width() + 2).collect();
    let arrow_w = 2;
    let total_w: usize =
        tab_widths.iter().sum::<usize>() + titles.len().saturating_sub(1) * divider_w;

    let mut start = app.tab_scroll_offset;
    #[allow(unused_assignments)]
    let mut end = titles.len();

    let needs_overflow = total_w > inner_w;

    if needs_overflow && !titles.is_empty() {
        if selected < start {
            start = selected;
        }

        end = start;
        let mut used = 0usize;
        #[allow(clippy::needless_range_loop)]
        for i in start..titles.len() {
            let left_space = if start > 0 { arrow_w } else { 0 };
            let right_space = arrow_w;
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

        if selected >= end {
            end = selected + 1;
            used = tab_widths[selected];
            start = selected;
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

        if end >= titles.len() {
            let left_space = if start > 0 { arrow_w } else { 0 };
            let budget = inner_w.saturating_sub(left_space);
            let mut recalc_used: usize = tab_widths[start..end].iter().sum::<usize>()
                + (end - start).saturating_sub(1) * divider_w;
            while start > 0 {
                let new_left = if start - 1 > 0 { arrow_w } else { 0 };
                let cost = divider_w + tab_widths[start - 1];
                if recalc_used + cost > inner_w.saturating_sub(new_left) {
                    break;
                }
                recalc_used += cost;
                start -= 1;
            }
            let _ = budget;
        }

        app.tab_scroll_offset = start;
    } else {
        start = 0;
        end = titles.len();
        app.tab_scroll_offset = 0;
    }

    let has_left = start > 0;
    let has_right = end < titles.len();

    let visible_titles: Vec<&(String, bool, bool)> = titles[start..end].iter().collect();
    let visible_selected = selected.saturating_sub(start);

    let mut spans: Vec<Span> = Vec::new();
    let mut tab_click_zones: Vec<(u16, u16, usize)> = Vec::new();
    let mut cursor_x = area.x;

    if has_left {
        spans.push(Span::styled(
            "◀ ",
            Style::default()
                .fg(Color::DarkGray)
                .bg(Color::Rgb(30, 30, 30)),
        ));
        cursor_x += 2;
    }

    for (i, (label, _is_active, has_notif)) in visible_titles.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                divider,
                Style::default().bg(Color::Rgb(30, 30, 30)),
            ));
            cursor_x += divider_w as u16;
        }
        let is_sel = i == visible_selected;
        let tab_text = format!(" {} ", label);
        let tab_w = tab_text.width() as u16;

        let tab_index = start + i;
        tab_click_zones.push((cursor_x, cursor_x + tab_w, tab_index));

        let style = if is_sel {
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(60, 60, 80))
                .add_modifier(Modifier::BOLD)
        } else if *has_notif {
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::Rgb(30, 30, 30))
        } else {
            Style::default()
                .fg(Color::Rgb(140, 140, 140))
                .bg(Color::Rgb(30, 30, 30))
        };

        spans.push(Span::styled(tab_text, style));
        cursor_x += tab_w;
    }

    // Fill remaining space with background
    let remaining = (area.width as usize).saturating_sub(cursor_x.saturating_sub(area.x) as usize + if has_right { 2 } else { 0 });
    if remaining > 0 {
        spans.push(Span::styled(
            " ".repeat(remaining),
            Style::default().bg(Color::Rgb(30, 30, 30)),
        ));
    }

    if has_right {
        spans.push(Span::styled(
            " ▶",
            Style::default()
                .fg(Color::DarkGray)
                .bg(Color::Rgb(30, 30, 30)),
        ));
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
        " Output ● ".to_string()
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

    // Render scrollbar on output
    let scroll_pos = max_scroll_back.saturating_sub(app.scroll_offset.min(max_scroll_back));
    render_scrollbar(frame, area, is_narrow, scroll_pos, max_scroll_back);
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
    let border_w: u16 = if is_narrow { 0 } else { 2 };
    let inner_width = area.width.saturating_sub(border_w) as usize;

    let mut lines: Vec<Line> = Vec::new();

    // Branch header
    let branch_line = if app.git_remote_branch.is_empty() {
        Line::from(vec![
            Span::styled(" ", Style::default()),
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
            Span::styled(" ", Style::default()),
            Span::styled(
                &app.git_branch,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" → ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.git_remote_branch, Style::default().fg(Color::DarkGray)),
            sync_icon,
        ])
    };
    lines.push(branch_line);
    lines.push(Line::from(""));

    // Parse status lines: [filename] [flex space] +added -removed [A/M/D]
    for line in app.git_status.lines() {
        if line.starts_with("##") {
            continue;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let (index_status, worktree_status) = if line.len() >= 2 {
            (
                line.chars().next().unwrap_or(' '),
                line.chars().nth(1).unwrap_or(' '),
            )
        } else {
            (' ', ' ')
        };

        let filename = if line.len() > 3 { &line[3..] } else { trimmed };

        // Determine the primary status letter and color
        let (status_char, status_color) = match (index_status, worktree_status) {
            ('?', '?') => ('?', Color::DarkGray),
            ('A', _) | (_, 'A') => ('A', Color::Green),
            ('D', _) | (_, 'D') => ('D', Color::Red),
            ('R', _) => ('R', Color::Magenta),
            ('M', _) | (_, 'M') => ('M', Color::Yellow),
            ('C', _) => ('C', Color::Cyan),
            _ => ('?', Color::DarkGray),
        };

        // Get per-file diff stats
        let (file_added, file_removed) = app
            .git_file_stats
            .get(filename)
            .copied()
            .unwrap_or((0, 0));

        let added_str = format!("+{}", file_added);
        let removed_str = format!("-{}", file_removed);
        let status_str = format!(" {}", status_char);

        // Calculate flexible padding
        // Layout: " {filename}  {pad}  +N -N  S"
        let prefix_w = 1; // leading space
        let fname_w = filename.width();
        let suffix_w = added_str.width() + 1 + removed_str.width() + status_str.width() + 1;
        let used = prefix_w + fname_w + 2 + suffix_w;
        let pad = if inner_width > used {
            inner_width - used
        } else {
            1
        };

        lines.push(Line::from(vec![
            Span::styled(" ", Style::default()),
            Span::styled(filename.to_string(), Style::default().fg(Color::White)),
            Span::raw(" ".repeat(pad)),
            Span::styled(added_str, Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(removed_str, Style::default().fg(Color::Red)),
            Span::styled(
                status_str,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]));
    }

    if lines.len() <= 2 {
        lines.push(Line::from(Span::styled(
            " ✓ Working tree clean",
            Style::default().fg(Color::Green),
        )));
    }

    let border_overhead: u16 = if is_narrow { 1 } else { 2 };
    let total = lines.len() as u16;
    let visible = area.height.saturating_sub(border_overhead);
    let max_scroll = total.saturating_sub(visible);
    app.git_status_scroll = app.git_status_scroll.min(max_scroll);

    let title = " Status ".to_string();
    let block = git_panel_block(&title, is_focused, is_narrow);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((app.git_status_scroll, 0));

    frame.render_widget(paragraph, area);

    // Render scrollbar
    render_scrollbar(frame, area, is_narrow, app.git_status_scroll, max_scroll);
}

fn draw_git_log(frame: &mut Frame, app: &mut App, area: Rect, is_narrow: bool) {
    let is_focused = app.focus == FocusPanel::GitPanel;
    let border_w: u16 = if is_narrow { 0 } else { 2 };
    let inner_width = area.width.saturating_sub(border_w) as usize;

    let mut lines: Vec<Line> = Vec::new();

    for line in app.git_log.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        // Extract graph prefix
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

        // Color graph characters individually for better visual hierarchy
        let mut graph_str = String::new();
        for ch in graph_part.chars() {
            match ch {
                '*' => {
                    if !graph_str.is_empty() {
                        spans.push(Span::styled(
                            std::mem::take(&mut graph_str),
                            Style::default().fg(Color::Rgb(80, 80, 120)),
                        ));
                    }
                    spans.push(Span::styled(
                        "*",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ));
                }
                _ => graph_str.push(ch),
            }
        }
        if !graph_str.is_empty() {
            spans.push(Span::styled(
                graph_str,
                Style::default().fg(Color::Rgb(80, 80, 120)),
            ));
        }

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

        // Hash - short and dim
        let hash = parts[0];
        spans.push(Span::styled(
            hash.to_string(),
            Style::default().fg(Color::Yellow),
        ));

        if parts.len() < 2 {
            lines.push(Line::from(spans));
            continue;
        }

        let remainder = parts[1];

        // Check for decoration
        if remainder.starts_with('(') {
            if let Some(close) = remainder.find(')') {
                let decoration = &remainder[1..close];
                spans.push(Span::styled(" [", Style::default().fg(Color::DarkGray)));

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
                    } else if ref_name.starts_with("origin/") || ref_name.starts_with("upstream/")
                    {
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
                spans.push(Span::styled("] ", Style::default().fg(Color::DarkGray)));

                let after_dec = &remainder[close + 1..].trim_start();
                if let Some(time_start) = after_dec.rfind('(') {
                    let msg = &after_dec[..time_start].trim_end();
                    let time_raw = &after_dec[time_start + 1..];
                    let time_clean = time_raw.trim_end_matches(')');
                    // Truncate message if needed to fit time
                    let msg_str = truncate_str(msg, inner_width.saturating_sub(graph_part.width() + hash.len() + 20));
                    spans.push(Span::styled(
                        msg_str,
                        Style::default().fg(Color::White),
                    ));
                    spans.push(Span::styled(
                        format!("  {}", time_clean),
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
                let time_raw = &remainder[time_start + 1..];
                let time_clean = time_raw.trim_end_matches(')');
                let msg_str = truncate_str(msg, inner_width.saturating_sub(graph_part.width() + hash.len() + 15));
                spans.push(Span::styled(
                    format!(" {}", msg_str),
                    Style::default().fg(Color::White),
                ));
                spans.push(Span::styled(
                    format!("  {}", time_clean),
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
            " No commits yet",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let border_overhead: u16 = if is_narrow { 1 } else { 2 };
    let total = lines.len() as u16;
    let visible = area.height.saturating_sub(border_overhead);
    let max_scroll = total.saturating_sub(visible);
    app.git_log_scroll = app.git_log_scroll.min(max_scroll);

    let title = " Log ".to_string();
    let block = git_panel_block(&title, is_focused, is_narrow);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((app.git_log_scroll, 0));

    frame.render_widget(paragraph, area);

    // Render scrollbar
    render_scrollbar(frame, area, is_narrow, app.git_log_scroll, max_scroll);
}

/// Truncate a string to fit within max_width, adding "..." if truncated.
fn truncate_str(s: &str, max_width: usize) -> String {
    if max_width < 4 {
        return s.chars().take(max_width).collect();
    }
    if s.width() <= max_width {
        s.to_string()
    } else {
        let mut result = String::new();
        let mut w = 0;
        for ch in s.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if w + cw + 3 > max_width {
                result.push_str("...");
                break;
            }
            result.push(ch);
            w += cw;
        }
        result
    }
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

    // Build path segment
    let directory = if on_splash {
        "aide".to_string()
    } else if let Some(s) = app.session_manager.active_session() {
        tilde_path(&s.directory)
    } else {
        "~".to_string()
    };

    // Build branch + upstream + diff segment
    let git_spans: Vec<Span> = if on_splash || app.git_branch.is_empty() {
        Vec::new()
    } else {
        let branch = &app.git_branch;
        let (behind, ahead) = app.git_upstream.unwrap_or((0, 0));
        let (added, deleted) = app.git_diff_stats.unwrap_or((0, 0));

        let mut spans = Vec::new();
        spans.push(Span::styled(
            format!(" {} ", branch),
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::Rgb(40, 40, 40))
                .add_modifier(Modifier::BOLD),
        ));
        if behind > 0 || ahead > 0 {
            let mut sync = String::new();
            if ahead > 0 {
                sync.push_str(&format!("↑{}", ahead));
            }
            if behind > 0 {
                if !sync.is_empty() {
                    sync.push(' ');
                }
                sync.push_str(&format!("↓{}", behind));
            }
            spans.push(Span::styled(
                format!(" {} ", sync),
                Style::default()
                    .fg(Color::Yellow)
                    .bg(Color::Rgb(40, 40, 40)),
            ));
        }
        if added > 0 || deleted > 0 {
            spans.push(Span::styled(
                format!(" +{}", added),
                Style::default()
                    .fg(Color::Green)
                    .bg(Color::Rgb(40, 40, 40)),
            ));
            spans.push(Span::styled(
                format!(" -{} ", deleted),
                Style::default()
                    .fg(Color::Red)
                    .bg(Color::Rgb(40, 40, 40)),
            ));
        }
        spans
    };

    // Build keybind hints
    let hint_spans: Vec<Span> = if on_splash {
        if app.session_manager.sessions.is_empty() {
            vec![
                hint_key("^P"),
                hint_label("commands "),
                hint_key("^X"),
                hint_label("exit "),
            ]
        } else {
            vec![
                hint_key("^P"),
                hint_label("commands "),
                hint_key("^X"),
                hint_label("exit "),
            ]
        }
    } else if app.focus == FocusPanel::GitPanel && app.show_right_panel {
        vec![
            hint_key("^G"),
            hint_label("back "),
            hint_key("^X"),
            hint_label("exit "),
        ]
    } else {
        vec![
            hint_key("^P"),
            hint_label("commands "),
            hint_key("^B"),
            hint_label("files "),
            hint_key("^G"),
            hint_label("git "),
            hint_key("^X"),
            hint_label("exit "),
        ]
    };

    let git_w: usize = git_spans.iter().map(|s| s.content.width()).sum();
    let hints_w: usize = hint_spans.iter().map(|s| s.content.width()).sum();

    // Truncate path if needed — never truncate branch/changes before path
    let max_path_w = w
        .saturating_sub(git_w)
        .saturating_sub(hints_w)
        .saturating_sub(2); // minimal padding
    let path_display = if directory.width() > max_path_w && max_path_w > 4 {
        truncate_str(&directory, max_path_w)
    } else {
        directory.clone()
    };

    let path_span = Span::styled(
        format!(" {} ", path_display),
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );
    let path_w = path_display.width() + 2;

    if is_narrow {
        // Two-row layout
        // Row 1: [path] [pad] [branch + stats]
        let line1_pad = w.saturating_sub(path_w + git_w);
        let mut line1_spans = vec![path_span.clone()];
        line1_spans.push(Span::styled(
            " ".repeat(line1_pad),
            Style::default().bg(Color::Rgb(40, 40, 40)),
        ));
        line1_spans.extend(git_spans.iter().cloned());
        let line1 = Line::from(line1_spans);

        // Row 2: [hints left-aligned]
        let line2_pad = w.saturating_sub(hints_w);
        let mut line2_spans: Vec<Span> = Vec::new();
        line2_spans.push(Span::styled(
            " ",
            Style::default().bg(Color::Rgb(40, 40, 40)),
        ));
        line2_spans.extend(hint_spans.iter().cloned());
        if line2_pad > 1 {
            line2_spans.push(Span::styled(
                " ".repeat(line2_pad.saturating_sub(1)),
                Style::default().bg(Color::Rgb(40, 40, 40)),
            ));
        }
        let line2 = Line::from(line2_spans);

        let text = Text::from(vec![line1, line2]);
        frame.render_widget(Paragraph::new(text), area);
    } else {
        // Single-row: [path] [branch+stats] [pad] [hints]
        let left_w = path_w + git_w;
        let padding = w.saturating_sub(left_w + hints_w);

        let mut spans = vec![path_span];
        spans.extend(git_spans);
        spans.push(Span::styled(
            " ".repeat(padding),
            Style::default().bg(Color::Rgb(40, 40, 40)),
        ));
        spans.extend(hint_spans);

        let bar = Line::from(spans);
        frame.render_widget(Paragraph::new(bar), area);
    }
}

fn hint_key(key: &str) -> Span<'_> {
    Span::styled(
        key,
        Style::default()
            .fg(Color::White)
            .bg(Color::Rgb(40, 40, 40))
            .add_modifier(Modifier::BOLD),
    )
}

fn hint_label(label: &str) -> Span<'_> {
    Span::styled(
        label,
        Style::default()
            .fg(Color::DarkGray)
            .bg(Color::Rgb(40, 40, 40)),
    )
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

fn draw_command_palette(frame: &mut Frame, app: &App, area: Rect) {
    let dialog_width = 60u16.min(area.width.saturating_sub(4));
    let dialog_height = 20u16.min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.height.min(3); // Position near top like VS Code
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    // Clear background
    let clear_lines: Vec<Line> = (0..dialog_height)
        .map(|_| {
            Line::from(Span::styled(
                " ".repeat(dialog_width as usize),
                Style::default().bg(Color::Rgb(30, 30, 30)),
            ))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(clear_lines).style(Style::default().bg(Color::Rgb(30, 30, 30))),
        dialog_area,
    );

    let inner_height = dialog_height.saturating_sub(2) as usize;

    // Use the command palette if it's open, otherwise use the picker (legacy)
    let (filter, selected, items): (&str, usize, Vec<(String, Option<&str>)>) =
        if app.show_command_palette {
            let palette_items = app.command_palette_items();
            let items: Vec<(String, Option<&str>)> = palette_items
                .iter()
                .map(|i| {
                    let kind = match &i.kind {
                        crate::app::PaletteKind::OpenFolder => Some("folder"),
                        crate::app::PaletteKind::OpenProject(_) => Some("project"),
                        crate::app::PaletteKind::NewTerminal => Some("terminal"),
                        crate::app::PaletteKind::ToggleGit => Some("panel"),
                        crate::app::PaletteKind::ToggleFileBrowser => Some("panel"),
                    };
                    (i.label.clone(), kind)
                })
                .collect();
            (
                &app.command_palette_filter,
                app.command_palette_selected,
                items,
            )
        } else {
            let filtered = app.filtered_projects();
            let items: Vec<(String, Option<&str>)> =
                filtered.iter().map(|p| (p.clone(), Some("project"))).collect();
            (&app.picker_filter, app.picker_selected, items)
        };

    let mut lines = vec![Line::from(vec![
        Span::styled(" > ", Style::default().fg(Color::Cyan)),
        Span::styled(
            filter,
            Style::default().fg(Color::White),
        ),
        Span::styled(
            "_",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::SLOW_BLINK),
        ),
    ])];

    let visible_slots = inner_height.saturating_sub(1);
    let scroll_start = if selected >= visible_slots {
        selected - visible_slots + 1
    } else {
        0
    };

    for (i, (label, kind)) in items
        .iter()
        .enumerate()
        .skip(scroll_start)
        .take(visible_slots)
    {
        let is_sel = i == selected;
        let bg = if is_sel {
            Color::Rgb(60, 60, 80)
        } else {
            Color::Rgb(30, 30, 30)
        };

        let mut spans = Vec::new();
        spans.push(Span::styled(
            if is_sel { " > " } else { "   " },
            Style::default().fg(Color::Cyan).bg(bg),
        ));
        spans.push(Span::styled(
            label.clone(),
            Style::default()
                .fg(if is_sel { Color::White } else { Color::Rgb(200, 200, 200) })
                .bg(bg)
                .add_modifier(if is_sel { Modifier::BOLD } else { Modifier::empty() }),
        ));

        if let Some(k) = kind {
            spans.push(Span::styled(
                format!("  {}", k),
                Style::default().fg(Color::DarkGray).bg(bg),
            ));
        }

        // Fill rest of line with background
        let line_w: usize = spans.iter().map(|s| s.content.width()).sum();
        let pad = (dialog_width.saturating_sub(2) as usize).saturating_sub(line_w);
        if pad > 0 {
            spans.push(Span::styled(
                " ".repeat(pad),
                Style::default().bg(bg),
            ));
        }

        lines.push(Line::from(spans));
    }

    let title = if app.show_command_palette {
        " Command Palette "
    } else {
        " Open Folder "
    };

    let paragraph = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(80, 80, 120)))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ))
            .style(Style::default().bg(Color::Rgb(30, 30, 30)).fg(Color::White)),
    );

    frame.render_widget(paragraph, dialog_area);
}

fn draw_file_browser(frame: &mut Frame, app: &App, area: Rect, is_narrow: bool) {
    let is_focused = app.focus == FocusPanel::FileBrowser;

    let block = if is_narrow {
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Rgb(60, 60, 60)))
            .style(Style::default().bg(Color::Rgb(25, 25, 25)))
    } else {
        focused_block(" Explorer ", is_focused)
    };

    let inner_height = area.height.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = Vec::new();

    if app.file_browser.entries.is_empty() {
        lines.push(Line::from(Span::styled(
            " No files",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let scroll = app.file_browser.scroll_offset as usize;

        for (i, entry) in app
            .file_browser
            .entries
            .iter()
            .enumerate()
            .skip(scroll)
            .take(inner_height)
        {
            let is_sel = i == app.file_browser.selected;
            let indent = "  ".repeat(entry.depth);
            let icon = if entry.is_dir {
                if entry.expanded {
                    "▾ "
                } else {
                    "▸ "
                }
            } else {
                "  "
            };

            let bg = if is_sel {
                Color::Rgb(60, 60, 80)
            } else {
                Color::Rgb(25, 25, 25)
            };

            let name_color = match entry.git_status {
                Some('A') => Color::Green,
                Some('M') => Color::Yellow,
                Some('D') => Color::Red,
                Some('?') => Color::DarkGray,
                _ => {
                    if entry.is_dir {
                        Color::Rgb(200, 200, 200)
                    } else {
                        Color::Rgb(180, 180, 180)
                    }
                }
            };

            let mut spans = vec![
                Span::styled(
                    format!(" {}{}", indent, icon),
                    Style::default().fg(Color::Rgb(100, 100, 100)).bg(bg),
                ),
                Span::styled(
                    &entry.name,
                    Style::default()
                        .fg(name_color)
                        .bg(bg)
                        .add_modifier(if entry.is_dir {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ];

            // Git status indicator on the right
            if let Some(status) = entry.git_status {
                let (ch, color) = match status {
                    'A' => ("A", Color::Green),
                    'M' => ("M", Color::Yellow),
                    'D' => ("D", Color::Red),
                    '?' => ("U", Color::DarkGray),
                    _ => ("", Color::DarkGray),
                };
                if !ch.is_empty() {
                    spans.push(Span::styled(
                        format!(" {}", ch),
                        Style::default().fg(color).bg(bg),
                    ));
                }
            }

            lines.push(Line::from(spans));
        }
    }

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);

    // Scrollbar for file browser
    let total = app.file_browser.entries.len() as u16;
    let visible = inner_height as u16;
    let max_scroll = total.saturating_sub(visible);
    if max_scroll > 0 {
        render_scrollbar(frame, area, is_narrow, app.file_browser.scroll_offset, max_scroll);
    }
}

fn draw_file_viewer(frame: &mut Frame, app: &App, area: Rect, is_narrow: bool) {
    let title = if let Some(ref path) = app.viewing_file {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        format!(" {} ", name)
    } else {
        " File ".to_string()
    };

    let block = if is_narrow {
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::default().fg(Color::Rgb(60, 60, 60)))
            .title(Span::styled(
                &title,
                Style::default().fg(Color::Cyan),
            ))
    } else {
        focused_block(&title, app.focus == FocusPanel::Output)
    };

    let inner_height = area.height.saturating_sub(2);

    let mut lines: Vec<Line> = Vec::new();
    for (i, line) in app.file_content.lines().enumerate() {
        let line_num = format!("{:>4} ", i + 1);
        lines.push(Line::from(vec![
            Span::styled(
                line_num,
                Style::default().fg(Color::Rgb(80, 80, 80)),
            ),
            Span::styled(line.to_string(), Style::default().fg(Color::Rgb(200, 200, 200))),
        ]));
    }

    let total = lines.len() as u16;
    let max_scroll = total.saturating_sub(inner_height);
    let scroll = app.file_scroll.min(max_scroll);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .scroll((scroll, 0));

    frame.render_widget(paragraph, area);

    render_scrollbar(frame, area, is_narrow, scroll, max_scroll);
}
