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
    GitPanel,
    FileBrowser,
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
    // Click target areas
    pub tab_bar_area: Rect,
    pub output_area: Rect,
    pub git_panel_area: Rect,
    pub tab_click_zones: Vec<(u16, u16, usize)>,
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
            tab_bar_area: Rect::default(),
            output_area: Rect::default(),
            git_panel_area: Rect::default(),
            tab_click_zones: Vec::new(),
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

    pub fn create_session_for_project(&mut self, project: &str) -> Result<()> {
        let dir = self.projects_dir.join(project);
        let dir_str = dir.to_string_lossy().to_string();
        self.session_manager.create_session(project, &dir_str)?;
        self.refresh_data();
        Ok(())
    }

    /// Open folder by full path (for command palette "Open Folder").
    pub fn open_folder(&mut self, path: &str) -> Result<()> {
        let name = std::path::Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("session");
        self.session_manager.create_session(name, path)?;
        self.refresh_data();
        Ok(())
    }

    pub fn filtered_projects(&self) -> Vec<String> {
        let filter = self.picker_filter.to_lowercase();
        self.available_projects
            .iter()
            .filter(|p| filter.is_empty() || p.to_lowercase().contains(&filter))
            .cloned()
            .collect()
    }

    pub fn open_picker(&mut self) {
        self.available_projects = discover_projects(&self.projects_dir);
        self.show_picker = true;
        self.picker_filter.clear();
        self.picker_selected = 0;
    }

    pub fn close_picker(&mut self) {
        self.show_picker = false;
        self.picker_filter.clear();
        self.picker_selected = 0;
    }

    pub fn picker_select_confirm(&mut self) -> Result<()> {
        let filtered = self.filtered_projects();
        if let Some(project) = filtered.get(self.picker_selected).cloned() {
            let was_on_welcome = self.is_on_welcome();
            self.close_picker();
            self.create_session_for_project(&project)?;
            if was_on_welcome {
                self.show_welcome = false;
            }
        }
        Ok(())
    }

    pub fn picker_move_down(&mut self) {
        let count = self.filtered_projects().len();
        if count > 0 {
            self.picker_selected = (self.picker_selected + 1) % count;
        }
    }

    pub fn picker_move_up(&mut self) {
        let count = self.filtered_projects().len();
        if count > 0 {
            self.picker_selected = if self.picker_selected == 0 {
                count - 1
            } else {
                self.picker_selected - 1
            };
        }
    }

    // Command palette
    pub fn open_command_palette(&mut self) {
        self.available_projects = discover_projects(&self.projects_dir);
        self.show_command_palette = true;
        self.command_palette_filter.clear();
        self.command_palette_selected = 0;
    }

    pub fn close_command_palette(&mut self) {
        self.show_command_palette = false;
        self.command_palette_filter.clear();
        self.command_palette_selected = 0;
    }

    pub fn command_palette_items(&self) -> Vec<PaletteItem> {
        let filter = self.command_palette_filter.to_lowercase();
        let mut items = Vec::new();

        // Built-in commands
        items.push(PaletteItem {
            label: "Open Folder...".to_string(),
            kind: PaletteKind::OpenFolder,
        });
        items.push(PaletteItem {
            label: "New Terminal".to_string(),
            kind: PaletteKind::NewTerminal,
        });
        items.push(PaletteItem {
            label: "Toggle Git Panel".to_string(),
            kind: PaletteKind::ToggleGit,
        });
        items.push(PaletteItem {
            label: "Toggle File Browser".to_string(),
            kind: PaletteKind::ToggleFileBrowser,
        });

        // Add projects as "Open: project_name"
        for project in &self.available_projects {
            items.push(PaletteItem {
                label: format!("Open: {}", project),
                kind: PaletteKind::OpenProject(project.clone()),
            });
        }

        if filter.is_empty() {
            items
        } else {
            items
                .into_iter()
                .filter(|i| i.label.to_lowercase().contains(&filter))
                .collect()
        }
    }

    pub fn command_palette_confirm(&mut self) -> Result<()> {
        let items = self.command_palette_items();
        if let Some(item) = items.get(self.command_palette_selected).cloned() {
            let was_on_welcome = self.is_on_welcome();
            self.close_command_palette();
            match item.kind {
                PaletteKind::OpenFolder => {
                    // Fall back to project picker for now
                    self.open_picker();
                }
                PaletteKind::OpenProject(project) => {
                    self.create_session_for_project(&project)?;
                    if was_on_welcome {
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
            }
        }
        Ok(())
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
    pub kind: PaletteKind,
}

#[derive(Clone)]
pub enum PaletteKind {
    OpenFolder,
    OpenProject(String),
    NewTerminal,
    ToggleGit,
    ToggleFileBrowser,
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
