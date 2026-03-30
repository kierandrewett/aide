use anyhow::Result;

use crate::tmux;

#[derive(Clone, Debug)]
pub struct Session {
    pub name: String,
    pub project_name: String,
    pub directory: String,
    pub instance_number: u32,
    pub has_notification: bool,
    pub last_output_len: usize,
}

pub struct SessionManager {
    pub sessions: Vec<Session>,
    pub active_index: usize,
    pub command: String,
}

impl SessionManager {
    pub fn new(command: String) -> Self {
        Self {
            sessions: Vec::new(),
            active_index: 0,
            command,
        }
    }

    /// Reconnect to existing aide tmux sessions on startup.
    pub fn reconnect_existing(&mut self) -> Result<()> {
        let existing = tmux::list_sessions()?;
        for name in existing {
            // Only pick up sessions that look like aide sessions (name_number pattern)
            if let Some((project, num)) = parse_session_name(&name) {
                // Try to get the session's working directory
                let dir = get_session_directory(&name).unwrap_or_default();
                self.sessions.push(Session {
                    name: name.clone(),
                    project_name: project,
                    directory: dir,
                    instance_number: num,
                    has_notification: false,
                    last_output_len: 0,
                });
            }
        }
        self.restore_tab_order();
        Ok(())
    }

    /// Create a new session for a project directory.
    pub fn create_session(&mut self, project_name: &str, directory: &str) -> Result<&Session> {
        let instance_number = self.next_instance_number(project_name);
        let session_name = format!("{}_{}", project_name, instance_number);

        tmux::create_session(&session_name, directory)?;
        tmux::run_command(&session_name, &self.command)?;

        self.sessions.push(Session {
            name: session_name,
            project_name: project_name.to_string(),
            directory: directory.to_string(),
            instance_number,
            has_notification: false,
            last_output_len: 0,
        });

        self.active_index = self.sessions.len() - 1;
        self.save_tab_order();
        Ok(self.sessions.last().unwrap())
    }

    /// Close a session by index.
    pub fn close_session(&mut self, index: usize) -> Result<()> {
        if index >= self.sessions.len() {
            return Ok(());
        }
        let session = &self.sessions[index];
        let _ = tmux::kill_session(&session.name);
        self.sessions.remove(index);
        if self.active_index >= self.sessions.len() && !self.sessions.is_empty() {
            self.active_index = self.sessions.len() - 1;
        }
        self.save_tab_order();
        Ok(())
    }

    pub fn active_session(&self) -> Option<&Session> {
        self.sessions.get(self.active_index)
    }

    /// Save current tab order to /tmp so it persists across aide restarts.
    pub fn save_tab_order(&self) {
        let names: Vec<&str> = self.sessions.iter().map(|s| s.name.as_str()).collect();
        let _ = std::fs::write(TAB_ORDER_FILE, names.join("\n"));
    }

    /// Reorder sessions to match the saved tab order.
    pub fn restore_tab_order(&mut self) {
        let data = match std::fs::read_to_string(TAB_ORDER_FILE) {
            Ok(d) => d,
            Err(_) => return,
        };
        let saved_order: Vec<&str> = data.lines().collect();
        if saved_order.is_empty() {
            return;
        }

        // Sort sessions by their position in the saved order.
        // Sessions not in the saved list go to the end.
        self.sessions.sort_by_key(|s| {
            saved_order
                .iter()
                .position(|n| *n == s.name)
                .unwrap_or(usize::MAX)
        });
    }

    fn next_instance_number(&self, project_name: &str) -> u32 {
        self.sessions
            .iter()
            .filter(|s| s.project_name == project_name)
            .map(|s| s.instance_number)
            .max()
            .unwrap_or(0)
            + 1
    }
}

const TAB_ORDER_FILE: &str = "/tmp/aide-tab-order";

fn parse_session_name(name: &str) -> Option<(String, u32)> {
    let last_underscore = name.rfind('_')?;
    let project = &name[..last_underscore];
    let num_str = &name[last_underscore + 1..];
    let num = num_str.parse().ok()?;
    if project.is_empty() {
        return None;
    }
    Some((project.to_string(), num))
}

fn get_session_directory(session_name: &str) -> Result<String> {
    let output = std::process::Command::new("tmux")
        .args([
            "display-message",
            "-t",
            session_name,
            "-p",
            "#{pane_current_path}",
        ])
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
