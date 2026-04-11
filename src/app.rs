use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use ratatui::layout::Rect;

use crate::config::Config;
use crate::editor_pane::EditorPane;

/// Resolve the editor command, falling back to a sibling binary in the same
/// directory as the current executable when running in a dev/cargo context.
/// This lets `aide` find `aide-editor` at `./target/debug/aide-editor` without
/// requiring it to be installed in PATH.
fn resolve_editor_command(cmd: &str) -> String {
    // Only attempt the sibling-binary lookup for commands that are a plain
    // binary name (no path separators, no arguments yet).
    let first_token = cmd.split_whitespace().next().unwrap_or(cmd);
    if !first_token.contains('/') && !first_token.contains('\\') {
        // Check if the binary is already reachable via PATH.
        let in_path = std::process::Command::new("which")
            .arg(first_token)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !in_path {
            // Try the directory that contains the currently running executable.
            if let Ok(exe) = std::env::current_exe() {
                if let Some(dir) = exe.parent() {
                    let sibling = dir.join(first_token);
                    if sibling.is_file() {
                        // Replace just the binary token with the full path, preserving any args.
                        let rest = cmd[first_token.len()..].to_string();
                        return format!("{}{}", sibling.to_string_lossy(), rest);
                    }
                }
            }
        }
    }
    cmd.to_string()
}
use crate::filebrowser::FileBrowser;
use crate::git::{self, CommitFile, GitWorker};
use crate::sessions::SessionManager;

/// Severity level for a toast notification.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum NotificationLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// A transient toast notification shown in the bottom-right corner.
#[allow(dead_code)]
pub struct Notification {
    pub message: String,
    pub level: NotificationLevel,
    pub created_at: Instant,
    /// How long before the notification auto-dismisses.
    pub ttl: Duration,
}

impl Notification {
    #[allow(dead_code)]
    pub fn new(message: impl Into<String>, level: NotificationLevel) -> Self {
        Self {
            message: message.into(),
            level,
            created_at: Instant::now(),
            ttl: Duration::from_secs(5),
        }
    }

    #[allow(dead_code)]
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.ttl
    }
}

/// A background job (e.g. git push) that runs outside the PTY.
pub struct BackgroundJob {
    pub label: String,
    pub started: Instant,
    pub result: Arc<Mutex<Option<JobResult>>>,
}

pub struct JobResult {
    pub success: bool,
    pub output: String,
}

#[derive(Clone, Copy, PartialEq)]
pub enum FocusPanel {
    Output,
    FileViewer,
    GitStatus,
    GitLog,
    FileBrowser,
}

/// What a rendered row in the git log panel represents (used for click handling).
#[derive(Clone)]
pub enum GitLogRow {
    /// A commit row — stores the short hash.
    Commit(String),
    /// A file row inside an expanded commit.
    File { hash: String, file_idx: usize },
    /// A graph-only line (no clickable content).
    Graph,
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

/// Per-tab snapshot of layout state.
/// Saved when leaving a tab and restored when returning to it.
/// The EditorPane (PTY process) is kept in App::editor_panes separately
/// since it cannot be cloned.
#[derive(Clone)]
pub struct TabLayout {
    // File viewer
    pub viewing_file: Option<String>,
    pub show_file_view: bool,
    // Panels
    pub show_file_browser: bool,
    pub focus: FocusPanel,
    // Terminal scrollback
    pub scroll_offset: u16,
    pub follow_mode: bool,
    // Git panel
    pub git_log_scroll: u16,
    pub git_status_scroll: u16,
    pub git_log_limit: usize,
    // File browser cursor
    pub file_browser_selected: usize,
    pub file_browser_scroll_offset: u16,
}

impl Default for TabLayout {
    fn default() -> Self {
        Self {
            viewing_file: None,
            show_file_view: false,
            show_file_browser: false,
            focus: FocusPanel::Output,
            scroll_offset: 0,
            follow_mode: true,
            git_log_scroll: 0,
            git_status_scroll: 0,
            git_log_limit: 100,
            file_browser_selected: 0,
            file_browser_scroll_offset: 0,
        }
    }
}

pub struct App {
    pub session_manager: SessionManager,
    pub icons: bool,
    pub config: Config,
    /// Per-session layout snapshots keyed by session ID.
    pub tab_layouts: HashMap<String, TabLayout>,
    /// Per-session EditorPane instances (can't go in TabLayout because not Clone).
    pub editor_panes: HashMap<String, EditorPane>,
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
    /// On narrow mode, whether to show file view or terminal
    pub show_file_view: bool,
    /// Active editor PTY pane (spawned when a file is opened)
    pub editor_pane: Option<EditorPane>,
    /// Editor command from config (resolved at startup)
    pub editor_command: String,
    /// Dimensions set by draw_file_viewer each frame, used to resize the pane
    pub editor_pane_rows: u16,
    pub editor_pane_cols: u16,
    // Settings modal
    pub show_settings: bool,
    pub settings_row: usize,
    pub settings_editing: bool,
    pub settings_buf: String,
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
    pub pty_last_scrollback: u16,
    /// Set when PTY needs a forced resize (e.g. after reconnecting to an existing session)
    pub needs_pty_resize: bool,
    /// Set when the terminal needs a full repaint (e.g. after parser reinitialization)
    pub needs_full_repaint: bool,
    // Text selection state — one per pane, only one active at a time
    pub selection: crate::selection::SelectionState,
    /// True when `selection` belongs to the editor pane; false = PTY output pane.
    pub selection_in_editor: bool,
    // Background jobs (git commands etc.)
    pub bg_jobs: Vec<BackgroundJob>,
    /// Message to show briefly in status bar after a job completes
    pub status_message: Option<(String, Instant, bool)>, // (msg, when, is_error)
    // Sub-areas for git panel click detection
    pub git_status_area: Rect,
    pub git_log_area: Rect,
    // Expanded commit state (click to toggle)
    pub expanded_commits: HashSet<String>,
    pub commit_files: HashMap<String, Vec<CommitFile>>,
    /// Populated by draw_git_log; maps display-row index → row type for click handling.
    pub git_log_rows: Vec<GitLogRow>,
    // Background git worker
    pub git_worker: GitWorker,
    /// Cached command palette items (regenerated only on filter change)
    pub cached_palette_items: Option<Vec<PaletteItem>>,
    /// Use-count per palette item label — drives MRU ordering when filter is empty.
    pub palette_usage: HashMap<String, u64>,
    /// Active toast notifications (bottom-right overlay stack).
    #[allow(dead_code)]
    pub notifications: Vec<Notification>,
    /// Double-click tracking for git log file rows: (display_row, click_time)
    pub last_git_log_click: Option<(usize, Instant)>,
    /// Double-click tracking for git status rows: (row_index, click_time)
    pub last_git_status_click: Option<(usize, Instant)>,
    /// Currently selected git status row index (into visible file entries)
    pub git_status_selected: Option<usize>,
    /// Currently selected git log display row index
    pub git_log_selected_row: Option<usize>,
    /// Content area of the file viewer pane (excludes scrollbar column and borders).
    /// Set each frame by draw_file_viewer; used by click handler to avoid forwarding scrollbar clicks.
    pub file_viewer_content_area: Rect,
}

impl App {
    pub fn new(config: Config) -> Self {
        let projects_dir = PathBuf::from(&config.projects_dir);
        let available_projects = discover_projects(&projects_dir);
        let editor_command = config.editor_command.clone();

        Self {
            session_manager: SessionManager::new(config.command.clone()),
            icons: config.icons,
            config,
            tab_layouts: HashMap::new(),
            editor_panes: HashMap::new(),
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
            show_file_view: false,
            editor_pane: None,
            editor_command,
            editor_pane_rows: 24,
            editor_pane_cols: 80,
            show_settings: false,
            settings_row: 0,
            settings_editing: false,
            settings_buf: String::new(),
            tab_bar_area: Rect::default(),
            output_area: Rect::default(),
            git_panel_area: Rect::default(),
            file_browser_area: Rect::default(),
            file_viewer_area: Rect::default(),
            tab_click_zones: Vec::new(),
            cached_project_files: Vec::new(),
            error_message: None,
            selection: crate::selection::SelectionState::new(),
            selection_in_editor: false,
            bg_jobs: Vec::new(),
            status_message: None,
            pty_parser: None,
            pty_session_id: String::new(),
            pty_last_len: 0,
            pty_title: String::new(),
            pty_last_scrollback: 0,
            needs_pty_resize: false,
            needs_full_repaint: false,
            git_status_area: Rect::default(),
            git_log_area: Rect::default(),
            expanded_commits: HashSet::new(),
            commit_files: HashMap::new(),
            git_log_rows: Vec::new(),
            git_worker: GitWorker::new(),
            cached_palette_items: None,
            palette_usage: HashMap::new(),
            notifications: Vec::new(),
            last_git_log_click: None,
            last_git_status_click: None,
            git_status_selected: None,
            git_log_selected_row: None,
            file_viewer_content_area: Rect::default(),
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
                // Kick off background git refresh
                self.git_worker.request_refresh(&dir, self.git_log_limit);

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

    /// Poll the git worker for new data. Returns true if a new snapshot arrived.
    pub fn poll_git(&mut self) -> bool {
        if let Some(snap) = self.git_worker.take_snapshot() {
            let branch_changed = !self.git_branch.is_empty() && snap.branch != self.git_branch;
            self.git_status = snap.status.clone();
            self.git_log = snap.log;
            self.git_branch = snap.branch;
            self.git_remote_branch = snap.remote_branch;
            self.git_upstream = snap.upstream;
            self.git_diff_stats = snap.diff_stats;
            self.git_file_stats = snap.file_stats;
            self.git_log_has_more = snap.log_has_more;
            self.file_browser.update_git_status(&snap.status);

            // When the branch changes, fetch from remote so upstream counts stay current.
            if branch_changed {
                if let Some(session) = self.session_manager.active_session() {
                    let dir = session.directory.trim_end_matches('/').to_string();
                    self.git_worker.fetch_and_refresh(&dir, self.git_log_limit);
                }
            }

            true
        } else {
            false
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

    /// Spawn a background command (e.g. git push) and track it.
    pub fn spawn_bg_command(&mut self, label: &str, command: &str, directory: &str) {
        let result: Arc<Mutex<Option<JobResult>>> = Arc::new(Mutex::new(None));
        let result_clone = result.clone();
        let cmd_str = command.to_string();
        let dir_str = directory.to_string();

        std::thread::spawn(move || {
            let output = std::process::Command::new("sh")
                .args(["-c", &cmd_str])
                .current_dir(&dir_str)
                .output();

            let job_result = match output {
                Ok(o) => {
                    let stdout = String::from_utf8_lossy(&o.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&o.stderr).to_string();
                    let combined = if stderr.is_empty() {
                        stdout
                    } else if stdout.is_empty() {
                        stderr
                    } else {
                        format!("{}\n{}", stdout, stderr)
                    };
                    JobResult {
                        success: o.status.success(),
                        output: combined.trim().to_string(),
                    }
                }
                Err(e) => JobResult {
                    success: false,
                    output: format!("Failed to run: {}", e),
                },
            };

            if let Ok(mut r) = result_clone.lock() {
                *r = Some(job_result);
            }
        });

        self.bg_jobs.push(BackgroundJob {
            label: label.to_string(),
            started: Instant::now(),
            result,
        });
    }

    /// Poll background jobs, moving completed ones to status_message.
    pub fn poll_bg_jobs(&mut self) {
        let mut i = 0;
        while i < self.bg_jobs.len() {
            let done = {
                let lock = self.bg_jobs[i].result.lock().unwrap();
                lock.is_some()
            };
            if done {
                let job = self.bg_jobs.remove(i);
                let result = job.result.lock().unwrap().take().unwrap();
                let msg = if result.output.is_empty() {
                    if result.success {
                        format!("{} ✓", job.label)
                    } else {
                        format!("{} failed", job.label)
                    }
                } else {
                    let first_line = result.output.lines().next().unwrap_or("");
                    if result.success {
                        format!("{}: {}", job.label, first_line)
                    } else {
                        format!("{} failed: {}", job.label, first_line)
                    }
                };
                self.status_message = Some((msg, Instant::now(), !result.success));
                // Refresh git data after git commands complete
                self.refresh_data();
            } else {
                i += 1;
            }
        }

        // Clear status message after 5 seconds
        if let Some((_, when, _)) = &self.status_message {
            if when.elapsed().as_secs() >= 5 {
                self.status_message = None;
            }
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
        self.cached_palette_items = None;
    }

    pub fn close_command_palette(&mut self) {
        self.show_command_palette = false;
        self.show_picker = false;
        self.command_palette_filter.clear();
        self.command_palette_selected = 0;
        self.cached_palette_items = None;
    }

    /// Invalidate the palette cache (call when filter changes).
    pub fn invalidate_palette_cache(&mut self) {
        self.cached_palette_items = None;
    }

    pub fn command_palette_items(&self) -> Vec<PaletteItem> {
        let filter = self.command_palette_filter.to_lowercase();
        // Normalized filter: strip punctuation so "git push" matches "Git: Push"
        let filter_norm = normalize_for_match(&filter);
        let has_session = self.session_manager.active_session().is_some() && !self.is_on_welcome();
        let mut items = Vec::new();

        // Built-in commands
        let commands: Vec<(&str, PaletteKind)> = vec![
            ("Open Folder...", PaletteKind::OpenFolder),
            ("New Tab", PaletteKind::NewTerminal),
            ("Toggle Git Panel", PaletteKind::ToggleGit),
            ("Toggle File Browser", PaletteKind::ToggleFileBrowser),
            ("Settings", PaletteKind::OpenSettings),
        ];
        for (label, kind) in commands {
            items.push(PaletteItem {
                label: label.to_string(),
                subtitle: String::new(),
                kind,
            });
        }

        // Git commands (only when in a session)
        if has_session {
            let git_commands: Vec<(&str, &str, &str)> = vec![
                ("Git: Push", "git push", "Push commits to remote"),
                ("Git: Pull", "git pull", "Pull changes from remote"),
                ("Git: Fetch", "git fetch", "Fetch from remote"),
                ("Git: Stash", "git stash", "Stash working changes"),
                ("Git: Stash Pop", "git stash pop", "Restore stashed changes"),
                ("Git: Commit", "git commit", "Open commit editor"),
            ];
            for (label, cmd, subtitle) in git_commands {
                items.push(PaletteItem {
                    label: label.to_string(),
                    subtitle: subtitle.to_string(),
                    kind: PaletteKind::RunCommand(cmd.to_string()),
                });
            }

            // Git branch switching
            let show_branches = filter_norm.starts_with("git switch")
                || filter_norm.starts_with("git checkout")
                || filter_norm.starts_with("git branch");
            if show_branches {
                if let Some(session) = self.session_manager.active_session() {
                    let dir = session.directory.clone();
                    if !dir.is_empty() {
                        let current = self.git_branch.clone();
                        for branch in git::list_branches(&dir) {
                            if branch == current {
                                continue;
                            }
                            items.push(PaletteItem {
                                label: format!("Git: Switch to {}", branch),
                                subtitle: String::new(),
                                kind: PaletteKind::GitCheckout(branch),
                            });
                        }
                    }
                }
            }
        }

        // Show projects when not in a session, or when filter matches "Open:" flow
        let show_projects = !has_session || filter_norm.starts_with("open");
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
            // Sort by most-recently-used (highest count first); unvisited items stay in place
            if !self.palette_usage.is_empty() {
                items.sort_by(|a, b| {
                    let ua = self.palette_usage.get(&a.label).copied().unwrap_or(0);
                    let ub = self.palette_usage.get(&b.label).copied().unwrap_or(0);
                    ub.cmp(&ua)
                });
            }
            items
        } else {
            let mut scored: Vec<(i32, PaletteItem)> = items
                .into_iter()
                .filter_map(|i| {
                    let label_norm = normalize_for_match(&i.label.to_lowercase());
                    let subtitle_norm = normalize_for_match(&i.subtitle.to_lowercase());
                    // Score against label (primary) and subtitle (secondary)
                    let score = fuzzy_score(&filter_norm, &label_norm)
                        .or_else(|| fuzzy_score(&filter_norm, &subtitle_norm).map(|s| s - 5)) // subtitle matches rank a bit lower
                        .or_else(|| {
                            // Fallback: raw filter against unnormalized strings
                            if i.label.to_lowercase().contains(&filter)
                                || i.subtitle.to_lowercase().contains(&filter)
                            {
                                Some(1)
                            } else {
                                None
                            }
                        })?;
                    Some((score, i))
                })
                .collect();
            // Sort best match first; stable so equal-scored items keep insertion order
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            scored.into_iter().map(|(_, i)| i).collect()
        }
    }

    pub fn command_palette_confirm(&mut self) {
        let items = self.palette_items_cached();
        if let Some(item) = items.get(self.command_palette_selected).cloned() {
            // Track usage for MRU ordering
            *self.palette_usage.entry(item.label.clone()).or_insert(0) += 1;
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
                PaletteKind::RunCommand(cmd) => {
                    if let Some(session) = self.session_manager.active_session() {
                        let dir = session.directory.clone();
                        let label = cmd.clone();
                        self.spawn_bg_command(&label, &cmd, &dir);
                    }
                }
                PaletteKind::GitCheckout(branch) => {
                    if let Some(session) = self.session_manager.active_session() {
                        let dir = session.directory.clone();
                        let cmd = format!("git checkout {}", branch);
                        self.spawn_bg_command(&cmd, &cmd, &dir);
                    }
                }
                PaletteKind::OpenSettings => {
                    self.open_settings();
                }
            }
        }
    }

    pub fn open_settings(&mut self) {
        self.show_settings = true;
        self.settings_row = 0;
        self.settings_editing = false;
        self.settings_buf.clear();
    }

    /// Available cursor shapes in cycle order: (config_value, display_name).
    pub const CURSOR_SHAPES: &'static [(&'static str, &'static str)] = &[
        ("default", "Default"),
        ("block", "Block"),
        ("blinking_block", "Block (blink)"),
        ("underline", "Underline"),
        ("blinking_underline", "Underline (blink)"),
        ("bar", "Bar"),
        ("blinking_bar", "Bar (blink)"),
    ];

    /// Available editor themes in cycle order.
    pub const EDITOR_THEMES: &'static [(&'static str, &'static str)] = &[
        ("github-dark", "GitHub Dark"),
        ("one-dark", "One Dark Pro"),
        ("dracula", "Dracula"),
        ("nord", "Nord"),
        ("monokai", "Monokai"),
        ("solarized-dark", "Solarized Dark"),
    ];

    /// Called when Enter is pressed on a settings row.
    pub fn settings_confirm(&mut self) {
        if self.settings_editing {
            // Commit the edit
            match self.settings_row {
                0 => self.config.command = self.settings_buf.clone(),
                1 => self.config.editor_command = self.settings_buf.clone(),
                2 => self.config.projects_dir = self.settings_buf.clone(),
                _ => {}
            }
            self.settings_editing = false;
            self.settings_buf.clear();
        } else {
            match self.settings_row {
                3 => {
                    // Toggle icons boolean
                    self.config.icons = !self.config.icons;
                    self.icons = self.config.icons;
                }
                4 => {
                    // Cycle theme forward (Enter = next)
                    self.cycle_theme(1);
                }
                5 => {
                    // Cycle cursor shape forward (Enter = next)
                    self.cycle_cursor_shape(1);
                }
                _ => {
                    // Enter edit mode for string fields
                    self.settings_editing = true;
                    self.settings_buf = match self.settings_row {
                        0 => self.config.command.clone(),
                        1 => self.config.editor_command.clone(),
                        2 => self.config.projects_dir.clone(),
                        _ => String::new(),
                    };
                }
            }
        }
    }

    /// Cycle the cursor shape by `delta` (+1 = next, -1 = prev).
    pub fn cycle_cursor_shape(&mut self, delta: i32) {
        let shapes = Self::CURSOR_SHAPES;
        let cur = shapes
            .iter()
            .position(|(id, _)| *id == self.config.cursor_shape)
            .unwrap_or(0) as i32;
        let next = ((cur + delta).rem_euclid(shapes.len() as i32)) as usize;
        self.config.cursor_shape = shapes[next].0.to_string();
    }

    /// Cycle the editor theme by `delta` (+1 = next, -1 = prev).
    pub fn cycle_theme(&mut self, delta: i32) {
        let themes = Self::EDITOR_THEMES;
        let cur = themes
            .iter()
            .position(|(id, _)| *id == self.config.editor_theme)
            .unwrap_or(0) as i32;
        let next = ((cur + delta).rem_euclid(themes.len() as i32)) as usize;
        self.config.editor_theme = themes[next].0.to_string();
        // Notify the running editor in real time via bracketed-paste side-channel
        if let Some(ep) = &mut self.editor_pane {
            let msg = format!("\x1b[200~aide-theme:{}\x1b[201~", self.config.editor_theme);
            ep.write_input(msg.as_bytes());
        }
    }

    /// Save config to disk and apply relevant live changes.
    pub fn settings_save(&mut self) {
        if self.settings_editing {
            self.settings_confirm();
        }
        // Apply live
        self.editor_command = self.config.editor_command.clone();
        self.icons = self.config.icons;
        let _ = self.config.save();
        self.show_settings = false;
        self.settings_editing = false;
        self.settings_buf.clear();
    }

    /// Get (and cache) command palette items.
    pub fn palette_items_cached(&mut self) -> Vec<PaletteItem> {
        if let Some(ref items) = self.cached_palette_items {
            return items.clone();
        }
        let items = self.command_palette_items();
        self.cached_palette_items = Some(items.clone());
        items
    }

    pub fn command_palette_move_down(&mut self) {
        let count = self.palette_items_cached().len();
        if count > 0 {
            self.command_palette_selected = (self.command_palette_selected + 1) % count;
        }
    }

    pub fn command_palette_move_up(&mut self) {
        let count = self.palette_items_cached().len();
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
        // Drop any existing editor pane first
        self.editor_pane = None;

        let rows = self.editor_pane_rows.max(4);
        let cols = self.editor_pane_cols.max(20);
        let cmd = resolve_editor_command(&self.editor_command);

        match EditorPane::spawn(
            &cmd,
            path,
            rows,
            cols,
            &self.config.editor_theme,
            &self.config.cursor_shape,
        ) {
            Ok(pane) => {
                self.editor_pane = Some(pane);
                self.viewing_file = Some(path.to_string());
                self.show_file_view = true;
            }
            Err(e) => {
                self.error_message = Some(format!("Failed to open editor: {}", e));
            }
        }
    }

    /// Close the file viewer and kill the editor pane.
    pub fn close_file(&mut self) {
        self.editor_pane = None;
        self.viewing_file = None;
        self.show_file_view = false;
    }

    pub fn is_on_welcome(&self) -> bool {
        if self.session_manager.sessions.is_empty() {
            return true;
        }
        self.show_welcome
            && self.session_manager.active_index >= self.session_manager.sessions.len()
    }

    /// Snapshot the current layout into `tab_layouts` under the active session ID.
    /// Also stash the EditorPane in `editor_panes` so it survives tab switching.
    pub fn save_tab_layout(&mut self) {
        let id = match self
            .session_manager
            .sessions
            .get(self.session_manager.active_index)
        {
            Some(s) => s.session_id.clone(),
            None => return,
        };
        // Move editor pane into the per-session map
        if let Some(pane) = self.editor_pane.take() {
            self.editor_panes.insert(id.clone(), pane);
        } else {
            self.editor_panes.remove(&id);
        }
        self.tab_layouts.insert(
            id,
            TabLayout {
                viewing_file: self.viewing_file.clone(),
                show_file_view: self.show_file_view,
                show_file_browser: self.show_file_browser,
                focus: self.focus,
                scroll_offset: self.scroll_offset,
                follow_mode: self.follow_mode,
                git_log_scroll: self.git_log_scroll,
                git_status_scroll: self.git_status_scroll,
                git_log_limit: self.git_log_limit,
                file_browser_selected: self.file_browser.selected,
                file_browser_scroll_offset: self.file_browser.scroll_offset,
            },
        );
    }

    /// Restore layout from `tab_layouts` for the active session, or apply defaults.
    pub fn restore_tab_layout(&mut self) {
        let id = match self
            .session_manager
            .sessions
            .get(self.session_manager.active_index)
        {
            Some(s) => s.session_id.clone(),
            None => {
                // Welcome screen — reset to defaults
                self.close_file();
                self.show_file_browser = false;
                self.focus = FocusPanel::Output;
                self.scroll_offset = 0;
                self.follow_mode = true;
                self.git_log_scroll = 0;
                self.git_status_scroll = 0;
                self.git_log_limit = 100;
                return;
            }
        };

        // Restore the EditorPane for this session
        self.editor_pane = self.editor_panes.remove(&id);

        let layout = self.tab_layouts.get(&id).cloned().unwrap_or_default();
        self.viewing_file = layout.viewing_file;
        self.show_file_view = layout.show_file_view;
        self.show_file_browser = layout.show_file_browser;
        self.focus = layout.focus;
        self.scroll_offset = layout.scroll_offset;
        self.follow_mode = layout.follow_mode;
        self.git_log_scroll = layout.git_log_scroll;
        self.git_status_scroll = layout.git_status_scroll;
        self.git_log_limit = layout.git_log_limit;
        self.file_browser.selected = layout.file_browser_selected;
        self.file_browser.scroll_offset = layout.file_browser_scroll_offset;
    }

    pub fn is_typing(&self) -> bool {
        self.last_input_time
            .map(|t| t.elapsed().as_millis() < 1500)
            .unwrap_or(false)
    }

    /// Toggle expansion of a commit in the git log panel.
    /// Fetches changed files on first expand (synchronous, fast).
    pub fn toggle_commit_expand(&mut self, hash: &str) {
        if self.expanded_commits.contains(hash) {
            self.expanded_commits.remove(hash);
        } else {
            self.expanded_commits.insert(hash.to_string());
            if !self.commit_files.contains_key(hash) {
                if let Some(session) = self.session_manager.active_session() {
                    let dir = session.directory.clone();
                    if !dir.is_empty() {
                        let files = git::fetch_commit_files(&dir, hash);
                        self.commit_files.insert(hash.to_string(), files);
                    }
                }
            }
        }
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
    /// Run a command in the active PTY
    RunCommand(String),
    /// Switch to a git branch (runs git checkout)
    GitCheckout(String),
    /// Open the settings modal
    OpenSettings,
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
                    ".",
                    "-type",
                    "f",
                    "-not",
                    "-path",
                    "*/.*",
                    "-not",
                    "-path",
                    "*/node_modules/*",
                    "-not",
                    "-path",
                    "*/target/*",
                    "-not",
                    "-path",
                    "*/__pycache__/*",
                    "-not",
                    "-path",
                    "*/venv/*",
                ])
                .current_dir(dir)
                .output();
            match find_output {
                Ok(out) => String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| l.strip_prefix("./").unwrap_or(l).to_string())
                    .collect(),
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

/// Subsequence fuzzy match with scoring.
/// Returns None if the query cannot be matched as a subsequence of target.
/// Score bonuses: consecutive runs, word-boundary starts.
fn subsequence_score(query: &str, target: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let q: Vec<char> = query.chars().collect();
    let t: Vec<char> = target.chars().collect();

    let mut score = 0i32;
    let mut qi = 0usize;
    let mut last_ti: Option<usize> = None;
    let mut run = 0i32;

    for ti in 0..t.len() {
        if qi >= q.len() {
            break;
        }
        if t[ti] == q[qi] {
            let consecutive = last_ti.is_some_and(|l| l + 1 == ti);
            if consecutive {
                run += 1;
                score += 4 + run; // growing bonus for unbroken runs
            } else {
                run = 0;
                score += 1;
            }
            // Word-boundary bonus: match starts right after a separator
            let at_boundary = ti == 0 || matches!(t[ti - 1], ' ' | ':' | '/' | '-' | '_' | '.');
            if at_boundary {
                score += 6;
            }
            last_ti = Some(ti);
            qi += 1;
        }
    }

    if qi == q.len() {
        Some(score)
    } else {
        None
    }
}

/// Fuzzy score with 1-typo tolerance for queries of 4+ characters.
/// Returns None if no match even with one deletion from the query.
fn fuzzy_score(query: &str, target: &str) -> Option<i32> {
    // Exact subsequence match wins outright
    if let Some(s) = subsequence_score(query, target) {
        return Some(s);
    }
    // For longer queries allow dropping any one character (handles transpositions
    // and single-key typos — e.g. "gti push" still matches "git push")
    if query.chars().count() >= 4 {
        let chars: Vec<char> = query.chars().collect();
        let mut best: Option<i32> = None;
        for skip in 0..chars.len() {
            let shortened: String = chars
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != skip)
                .map(|(_, c)| *c)
                .collect();
            if let Some(s) = subsequence_score(&shortened, target) {
                let penalised = s - 8; // typo penalty
                best = Some(best.map_or(penalised, |b: i32| b.max(penalised)));
            }
        }
        if best.is_some() {
            return best;
        }
    }
    None
}

/// Strip punctuation (colons, dots, dashes, etc.) and collapse whitespace
/// so "git push" matches "Git: Push" and "open folder" matches "Open Folder...".
fn normalize_for_match(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
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
