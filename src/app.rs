use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;

use crate::config::Config;
use crate::git;
use crate::sessions::SessionManager;
use crate::tmux;

#[derive(Clone, Copy, PartialEq)]
pub enum FocusPanel {
    Output,
    GitPanel,
}

pub struct App {
    pub session_manager: SessionManager,
    pub show_right_panel: bool,
    pub claude_output: String,
    pub git_status: String,
    pub git_log: String,
    pub git_branch: String,
    pub git_upstream: Option<(usize, usize)>,
    pub git_diff_stats: Option<(usize, usize)>,
    pub scroll_offset: u16,
    pub follow_mode: bool,
    pub should_quit: bool,
    pub show_close_confirm: bool,
    pub show_picker: bool,
    pub picker_filter: String,
    pub picker_selected: usize,
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
}

impl App {
    pub fn new(config: Config) -> Self {
        let projects_dir = PathBuf::from(&config.projects_dir);
        let available_projects = discover_projects(&projects_dir);

        Self {
            session_manager: SessionManager::new(config.command),
            show_right_panel: false,
            claude_output: String::new(),
            git_status: String::new(),
            git_log: String::new(),
            git_branch: String::new(),
            git_upstream: None,
            git_diff_stats: None,
            scroll_offset: 0,
            follow_mode: true,
            should_quit: false,
            show_close_confirm: false,
            show_picker: false,
            picker_filter: String::new(),
            picker_selected: 0,
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
        }
    }

    pub fn init(&mut self) -> Result<()> {
        self.session_manager.reconnect_existing()?;
        self.refresh_data();
        Ok(())
    }

    pub fn refresh_data(&mut self) {
        if let Some(session) = self.session_manager.active_session() {
            let name = session.name.clone();
            let dir = session.directory.clone();

            // Capture claude output
            if let Ok(output) = tmux::capture_pane(&name) {
                self.claude_output = output;
            }

            // Git data
            if !dir.is_empty() {
                if let Ok(status) = git::status_short(&dir) {
                    self.git_status = status;
                }
                if let Ok(log) = git::log_oneline(&dir, self.git_log_limit) {
                    // If we got fewer lines than limit, there's no more history
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
            }
        } else {
            self.claude_output.clear();
            self.git_status.clear();
            self.git_log.clear();
            self.git_branch.clear();
            self.git_upstream = None;
            self.git_diff_stats = None;
            self.git_remote_branch.clear();
        }
    }

    pub fn create_session_for_project(&mut self, project: &str) -> Result<()> {
        let dir = self.projects_dir.join(project);
        let dir_str = dir.to_string_lossy().to_string();
        self.session_manager.create_session(project, &dir_str)?;
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
            self.close_picker();
            self.show_welcome = false;
            self.create_session_for_project(&project)?;
        }
        Ok(())
    }

    pub fn picker_move_down(&mut self) {
        let count = self.filtered_projects().len();
        if count > 0 {
            self.picker_selected = (self.picker_selected + 1) % count;
        }
    }

    pub fn is_typing(&self) -> bool {
        self.last_input_time
            .map(|t| t.elapsed().as_millis() < 1500)
            .unwrap_or(false)
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
