use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;
use ratatui::layout::Rect;

use crate::config::Config;
use crate::filebrowser::FileBrowser;
use crate::git;
use crate::sessions::SessionManager;

#[derive(Clone, Copy, PartialEq)]
pub enum FocusPanel {
    Output,
    FileViewer,
    GitStatus,
    GitLog,
    FileBrowser,
}

#[derive(Clone, Debug)]
pub struct TextSelection {
    /// Start position in screen coordinates (col, row) relative to output area
    pub start_col: u16,
    pub start_row: u16,
    /// Current/end position
    pub end_col: u16,
    pub end_row: u16,
    /// Whether user is currently dragging
    pub active: bool,
}

/// Callbacks for vt100 parser to capture PTY title changes.
#[derive(Clone, Default)]
pub struct PtyCallbacks {
    pub title: String,
}

impl vt100::Callbacks for PtyCallbacks {
    fn set_window_title(&mut self, _screen: &mut vt100::Screen, title: &[u8]) {
        self.title = String::from_utf8_lossy(title).to_string();
    }
}

pub struct App {
    pub session_manager: SessionManager,
    pub show_right_panel: bool,
    pub show_file_browser: bool,
    pub claude_output: String,
    pub git_status: String,
    pub git_log: String,
    pub git_branch: String,
    pub git_upstream: Option<(usize, usize)>,
    pub git_diff_stats: Option<(usize, usize)>,
    pub git_file_stats: HashMap<String, (usize, usize)>,
    pub scroll_offset: u16,
    pub follow_mode: bool,
    pub should_quit: bool,
    pub show_close_confirm: bool,
    pub show_picker: bool,
    pub picker_filter: String,
    pub picker_selected: usize,
    pub show_command_palette: bool,
    pub command_palette_filter: String,
    pub command_palette_selected: usize,
    pub projects_dir: PathBuf,
    pub available_projects: Vec<String>,
    pub last_input_time: Option<Instant>,
    pub focus: FocusPanel,
    pub output_height: u16,
    pub output_width: u16,
    pub git_status_scroll: u16,
    pub git_log_scroll: u16,
    pub git_remote_branch: String,
    pub git_log_limit: usize,
    pub git_log_has_more: bool,
    pub show_welcome: bool,
    pub tab_scroll_offset: usize,
    pub is_narrow: bool,
    pub file_browser: FileBrowser,
    /// Currently viewed file path (None = no file open)
    pub viewing_file: Option<String>,
    /// Cached file content for viewer
    pub file_content: String,
    /// Pre-highlighted lines cache (avoids re-running syntect every frame)
    pub file_highlighted: Vec<Vec<(syntect::highlighting::Style, String)>>,
    /// File viewer vertical scroll offset
    pub file_scroll: u16,
    /// File viewer horizontal scroll offset
    pub file_scroll_h: u16,
    /// On narrow mode, whether to show file view or terminal
    pub show_file_view: bool,
    // Click target areas
    pub tab_bar_area: Rect,
    pub output_area: Rect,
    #[allow(dead_code)]
    pub git_panel_area: Rect,
    pub file_browser_area: Rect,
    pub file_viewer_area: Rect,
    pub tab_click_zones: Vec<(u16, u16, usize)>,
    /// Cached project files for command palette (populated on open)
    pub cached_project_files: Vec<(String, String, String)>,
    /// Transient error message to display in the UI (auto-clears on next action)
    pub error_message: Option<String>,
    // PTY terminal emulator
    pub pty_parser: Option<vt100::Parser<PtyCallbacks>>,
    pub pty_session_id: String,
    pub pty_last_len: usize,
    pub pty_title: String,
    // Text selection state
    pub selection: Option<TextSelection>,
    // Sub-areas for git panel click detection
    pub git_status_area: Rect,
    pub git_log_area: Rect,
}

impl App {
    pub fn new(config: Config) -> Self {
        let projects_dir = PathBuf::from(&config.projects_dir);
        let available_projects = discover_projects(&projects_dir);

        Self {
            session_manager: SessionManager::new(config.command),
            show_right_panel: false,
            show_file_browser: false,
            claude_output: String::new(),
            git_status: String::new(),
            git_log: String::new(),
            git_branch: String::new(),
            git_upstream: None,
            git_diff_stats: None,
            git_file_stats: HashMap::new(),
            scroll_offset: 0,
            follow_mode: true,
            should_quit: false,
            show_close_confirm: false,
            show_picker: false,
            picker_filter: String::new(),
            picker_selected: 0,
            show_command_palette: false,
            command_palette_filter: String::new(),
            command_palette_selected: 0,
            projects_dir,
            available_projects,
            last_input_time: None,
            focus: FocusPanel::Output,
            output_height: 0,
            output_width: 0,
            git_status_scroll: 0,
            git_log_scroll: 0,
            git_remote_branch: String::new(),
            git_log_limit: 100,
            git_log_has_more: true,
            show_welcome: true,
            tab_scroll_offset: 0,
            is_narrow: false,
            file_browser: FileBrowser::new(),
            viewing_file: None,
            file_content: String::new(),
            file_highlighted: Vec::new(),
            file_scroll: 0,
            file_scroll_h: 0,
            show_file_view: false,
            tab_bar_area: Rect::default(),
            output_area: Rect::default(),
            git_panel_area: Rect::default(),
            file_browser_area: Rect::default(),
            file_viewer_area: Rect::default(),
            tab_click_zones: Vec::new(),
            cached_project_files: Vec::new(),
            error_message: None,
            selection: None,
            pty_parser: None,
            pty_session_id: String::new(),
            pty_last_len: 0,
            pty_title: String::new(),
            git_status_area: Rect::default(),
            git_log_area: Rect::default(),
        }
    }

    pub fn init(&mut self) -> Result<()> {
        self.session_manager.connect_daemon()?;
        self.session_manager.reconnect_existing()?;
        self.refresh_data();
        Ok(())
    }

    pub fn refresh_data(&mut self) {
        if let Some(session) = self.session_manager.active_session() {
            let dir = session.directory.clone();

            if let Ok(output) = self.session_manager.read_output() {
                self.claude_output = output;
            }

            if !dir.is_empty() {
                if let Ok(status) = git::status_short(&dir) {
                    self.git_status = status;
                }
                if let Ok(log) = git::log_oneline(&dir, self.git_log_limit) {
                    let line_count = log.lines().count();
                    self.git_log_has_more = line_count >= self.git_log_limit;
                    self.git_log = log;
                }
                if let Ok(branch) = git::current_branch(&dir) {
                    self.git_branch = branch;
                }
                self.git_upstream = git::upstream_counts(&dir);
                self.git_diff_stats = git::diff_stats(&dir);
                self.git_remote_branch = git::remote_tracking_branch(&dir).unwrap_or_default();
                self.git_file_stats = git::file_diff_stats(&dir);

                // Update file browser root
                self.file_browser.set_root(&dir);
            }
        } else {
            self.claude_output.clear();
            self.git_status.clear();
            self.git_log.clear();
            self.git_branch.clear();
            self.git_upstream = None;
            self.git_diff_stats = None;
            self.git_remote_branch.clear();
            self.git_file_stats.clear();
        }
    }

    pub fn create_session_for_project(&mut self, project: &str) {
        let dir = self.projects_dir.join(project);
        let dir_str = dir.to_string_lossy().to_string();
        match self.session_manager.create_session(project, &dir_str) {
            Ok(_) => self.refresh_data(),
            Err(e) => self.error_message = Some(format!("Failed to open project: {}", e)),
        }
    }

    /// Open folder by full path (for command palette "Open Folder").
    #[allow(dead_code)]
    pub fn open_folder(&mut self, path: &str) {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("session");
        match self.session_manager.create_session(name, path) {
            Ok(_) => self.refresh_data(),
            Err(e) => self.error_message = Some(format!("Failed to open folder: {}", e)),
        }
    }

    pub fn filtered_projects(&self) -> Vec<String> {
        let filter = self.picker_filter.to_lowercase();
        self.available_projects
            .iter()
            .filter(|p| filter.is_empty() || p.to_lowercase().contains(&filter))
            .cloned()
            .collect()
    }

    #[allow(dead_code)]
    pub fn open_picker(&mut self) {
        self.open_command_palette();
    }

    // Command palette
    pub fn open_command_palette(&mut self) {
        self.available_projects = discover_projects(&self.projects_dir);
        // Cache project files for fast filtering
        if let Some(session) = self.session_manager.active_session() {
            let dir = session.directory.clone();
            if !dir.is_empty() && !self.is_on_welcome() {
                self.cached_project_files = recent_project_files(&dir);
            } else {
                self.cached_project_files.clear();
            }
        } else {
            self.cached_project_files.clear();
        }
        self.show_command_palette = true;
        self.command_palette_filter.clear();
        self.command_palette_selected = 0;
    }

    pub fn close_command_palette(&mut self) {
        self.show_command_palette = false;
        self.show_picker = false;
        self.command_palette_filter.clear();
        self.command_palette_selected = 0;
    }

    pub fn command_palette_items(&self) -> Vec<PaletteItem> {
        let filter = self.command_palette_filter.to_lowercase();
        let has_session = self.session_manager.active_session().is_some() && !self.is_on_welcome();
        let mut items = Vec::new();

        // Built-in commands
        let commands: Vec<(&str, PaletteKind)> = vec![
            ("Open Folder...", PaletteKind::OpenFolder),
            ("New Tab", PaletteKind::NewTerminal),
            ("Toggle Git Panel", PaletteKind::ToggleGit),
            ("Toggle File Browser", PaletteKind::ToggleFileBrowser),
        ];
        for (label, kind) in commands {
            items.push(PaletteItem {
                label: label.to_string(),
                subtitle: String::new(),
                kind,
            });
        }

        // Show projects when not in a session, or when filter matches "Open:" flow
        let show_projects = !has_session || filter.starts_with("open:");
        if show_projects {
            for project in &self.available_projects {
                items.push(PaletteItem {
                    label: format!("Open: {}", project),
                    subtitle: String::new(),
                    kind: PaletteKind::OpenProject(project.clone()),
                });
            }
        }

        // In an open folder: show project files
        if has_session {
            for (name, rel_path, full_path) in &self.cached_project_files {
                items.push(PaletteItem {
                    label: name.clone(),
                    subtitle: rel_path.clone(),
                    kind: PaletteKind::ProjectFile(full_path.clone()),
                });
            }
        }

        if filter.is_empty() {
            items
        } else {
            items
                .into_iter()
                .filter(|i| {
                    i.label.to_lowercase().contains(&filter)
                        || i.subtitle.to_lowercase().contains(&filter)
                })
                .collect()
        }
    }

    pub fn command_palette_confirm(&mut self) {
        let items = self.command_palette_items();
        if let Some(item) = items.get(self.command_palette_selected).cloned() {
            let was_on_welcome = self.is_on_welcome();
            self.close_command_palette();
            match item.kind {
                PaletteKind::OpenFolder => {
                    // Re-open command palette with "Open:" prefix to filter to projects
                    self.open_command_palette();
                    self.command_palette_filter = "Open: ".to_string();
                }
                PaletteKind::OpenProject(project) => {
                    self.create_session_for_project(&project);
                    if was_on_welcome && self.error_message.is_none() {
                        self.show_welcome = false;
                    }
                }
                PaletteKind::NewTerminal => {
                    self.show_welcome = true;
                    self.session_manager.active_index = self.session_manager.sessions.len();
                }
                PaletteKind::ToggleGit => {
                    self.show_right_panel = !self.show_right_panel;
                }
                PaletteKind::ToggleFileBrowser => {
                    self.show_file_browser = !self.show_file_browser;
                }
                PaletteKind::ProjectFile(path) => {
                    self.open_file(&path);
                    self.show_file_browser = true;
                }
            }
        }
    }

    pub fn command_palette_move_down(&mut self) {
        let count = self.command_palette_items().len();
        if count > 0 {
            self.command_palette_selected = (self.command_palette_selected + 1) % count;
        }
    }

    pub fn command_palette_move_up(&mut self) {
        let count = self.command_palette_items().len();
        if count > 0 {
            self.command_palette_selected = if self.command_palette_selected == 0 {
                count - 1
            } else {
                self.command_palette_selected - 1
            };
        }
    }

    /// Open a file for viewing.
    pub fn open_file(&mut self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(content) => {
                self.viewing_file = Some(path.to_string());
                self.file_highlighted = highlight_file(path, &content);
                self.file_content = content;
                self.file_scroll = 0;
                self.file_scroll_h = 0;
                self.show_file_view = true;
            }
            Err(_) => {
                // Can't read file (binary or permission denied) — ignore
            }
        }
    }

    /// Close the file viewer.
    pub fn close_file(&mut self) {
        self.viewing_file = None;
        self.file_content.clear();
        self.file_highlighted.clear();
        self.file_scroll = 0;
        self.file_scroll_h = 0;
        self.show_file_view = false;
    }

    pub fn is_on_welcome(&self) -> bool {
        if self.session_manager.sessions.is_empty() {
            return true;
        }
        self.show_welcome
            && self.session_manager.active_index >= self.session_manager.sessions.len()
    }

    pub fn is_typing(&self) -> bool {
        self.last_input_time
            .map(|t| t.elapsed().as_millis() < 1500)
            .unwrap_or(false)
    }
}

#[derive(Clone)]
pub struct PaletteItem {
    pub label: String,
    /// Optional secondary text (e.g. file path)
    pub subtitle: String,
    pub kind: PaletteKind,
}

#[derive(Clone)]
pub enum PaletteKind {
    OpenFolder,
    OpenProject(String),
    NewTerminal,
    ToggleGit,
    ToggleFileBrowser,
    /// Open a file from the project index
    ProjectFile(String),
}

/// Pre-highlight a file's content using syntect. Returns styled ranges per line.
fn highlight_file(path: &str, content: &str) -> Vec<Vec<(syntect::highlighting::Style, String)>> {
    use syntect::easy::HighlightLines;
    use syntect::highlighting::ThemeSet;
    use syntect::parsing::SyntaxSet;

    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let theme = &ts.themes["base16-eighties.dark"];
    let syntax = ss
        .find_syntax_for_file(path)
        .ok()
        .flatten()
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut h = HighlightLines::new(syntax, theme);

    content
        .lines()
        .map(|line| {
            h.highlight_line(line, &ss)
                .unwrap_or_default()
                .into_iter()
                .map(|(style, text)| (style, text.to_string()))
                .collect()
        })
        .collect()
}

/// Get project files sorted by modification time (most recent first).
/// Returns (filename, relative_path, absolute_path).
fn recent_project_files(dir: &str) -> Vec<(String, String, String)> {
    use std::process::Command;

    // Try git ls-files first (fast, respects .gitignore)
    let git_output = Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .current_dir(dir)
        .output();

    let file_list: Vec<String> = match git_output {
        Ok(out) if out.status.success() && !out.stdout.is_empty() => {
            String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.to_string())
                .collect()
        }
        _ => {
            // Not a git repo — use find, skip hidden dirs and common junk
            let find_output = Command::new("find")
                .args([
                    ".", "-type", "f",
                    "-not", "-path", "*/.*",
                    "-not", "-path", "*/node_modules/*",
                    "-not", "-path", "*/target/*",
                    "-not", "-path", "*/__pycache__/*",
                    "-not", "-path", "*/venv/*",
                ])
                .current_dir(dir)
                .output();
            match find_output {
                Ok(out) => {
                    String::from_utf8_lossy(&out.stdout)
                        .lines()
                        .filter(|l| !l.is_empty())
                        .map(|l| l.strip_prefix("./").unwrap_or(l).to_string())
                        .collect()
                }
                Err(_) => return Vec::new(),
            }
        }
    };

    // Get modification times and sort
    let root = std::path::Path::new(dir);
    let mut files_with_mtime: Vec<(String, String, String, u64)> = file_list
        .into_iter()
        .filter_map(|rel| {
            let full = root.join(&rel);
            let mtime = full
                .metadata()
                .ok()?
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs();
            let name = std::path::Path::new(&rel)
                .file_name()?
                .to_str()?
                .to_string();
            let full_str = full.to_string_lossy().to_string();
            Some((name, rel, full_str, mtime))
        })
        .collect();

    files_with_mtime.sort_by(|a, b| b.3.cmp(&a.3)); // newest first

    files_with_mtime
        .into_iter()
        .map(|(name, rel, full, _)| (name, rel, full))
        .collect()
}

fn discover_projects(dir: &PathBuf) -> Vec<String> {
    let mut projects = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                if let Some(name) = entry.file_name().to_str() {
                    if !name.starts_with('.') {
                        projects.push(name.to_string());
                    }
                }
            }
        }
    }
    projects.sort();
    projects
}
