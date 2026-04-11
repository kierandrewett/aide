use crate::pty_backend::DaemonClient;
use anyhow::Result;

#[derive(Clone, Debug)]
pub struct Session {
    pub name: String,
    pub session_id: String,
    pub project_name: String,
    pub directory: String,
    pub instance_number: u32,
    pub has_notification: bool,
    /// Byte offset into the daemon output buffer for incremental reads.
    pub output_offset: usize,
}

pub struct SessionManager {
    pub sessions: Vec<Session>,
    pub active_index: usize,
    pub command: String,
    pub daemon: Option<DaemonClient>,
}

impl SessionManager {
    pub fn new(command: String) -> Self {
        Self {
            sessions: Vec::new(),
            active_index: 0,
            command,
            daemon: None,
        }
    }

    /// Connect to daemon, starting it if needed.
    pub fn connect_daemon(&mut self) -> Result<()> {
        let client = DaemonClient::connect()?;
        self.daemon = Some(client);
        Ok(())
    }

    fn daemon(&mut self) -> Result<&mut DaemonClient> {
        if self.daemon.is_none() {
            self.connect_daemon()?;
        }
        self.daemon
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("no daemon connection"))
    }

    /// Reconnect to existing sessions on startup.
    pub fn reconnect_existing(&mut self) -> Result<()> {
        let daemon = self.daemon()?;
        let existing = daemon.list_sessions()?;
        for info in existing {
            if !info.alive {
                continue;
            }
            if let Some((project, num)) = parse_session_name(&info.session_id) {
                self.sessions.push(Session {
                    name: info.session_id.clone(),
                    session_id: info.session_id,
                    project_name: project,
                    directory: info.cwd,
                    instance_number: num,
                    has_notification: false,
                    output_offset: 0,
                });
            }
        }
        self.restore_tab_order();
        Ok(())
    }

    /// Create a new session for a project directory.
    pub fn create_session(&mut self, project_name: &str, directory: &str) -> Result<&Session> {
        let instance_number = self.next_instance_number(project_name);
        let session_name = format!("{}_{}", sanitize_name(project_name), instance_number);

        let command = self.command.clone();
        let daemon = self.daemon()?;
        daemon.create_session(&session_name, directory, &command, &[])?;

        self.sessions.push(Session {
            name: session_name.clone(),
            session_id: session_name,
            project_name: project_name.to_string(),
            directory: directory.to_string(),
            instance_number,
            has_notification: false,

            output_offset: 0,
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
        let session_id = self.sessions[index].session_id.clone();
        if let Ok(daemon) = self.daemon() {
            let _ = daemon.kill_session(&session_id);
        }
        self.sessions.remove(index);
        if self.active_index >= self.sessions.len() && !self.sessions.is_empty() {
            self.active_index = self.sessions.len() - 1;
        }
        self.save_tab_order();
        Ok(())
    }

    /// Write raw input to the active session's PTY.
    pub fn write_input(&mut self, data: &[u8]) -> Result<()> {
        let session_id = self
            .active_session()
            .map(|s| s.session_id.clone())
            .ok_or_else(|| anyhow::anyhow!("no active session"))?;
        let daemon = self.daemon()?;
        daemon.write_input(&session_id, data)
    }

    /// Write input to a specific session.
    #[allow(dead_code)]
    pub fn write_input_to(&mut self, session_id: &str, data: &[u8]) -> Result<()> {
        let daemon = self.daemon()?;
        daemon.write_input(session_id, data)
    }

    /// Read incremental output from the active session.
    pub fn read_output(&mut self) -> Result<String> {
        let idx = self.active_index;
        let session_id = self
            .sessions
            .get(idx)
            .ok_or_else(|| anyhow::anyhow!("no active session"))?
            .session_id
            .clone();
        let daemon = self.daemon()?;
        let (data, new_offset) = daemon.read_output(&session_id, 0)?; // Read full buffer
        if let Some(s) = self.sessions.get_mut(idx) {
            s.output_offset = new_offset;
        }
        Ok(String::from_utf8_lossy(&data).to_string())
    }

    /// Read raw bytes from the active session (for vt100 parser).
    /// Uses incremental reads from the last known offset.
    pub fn read_output_bytes(&mut self) -> Result<(Vec<u8>, usize)> {
        let idx = self.active_index;
        let (session_id, offset) = {
            let s = self
                .sessions
                .get(idx)
                .ok_or_else(|| anyhow::anyhow!("no active session"))?;
            (s.session_id.clone(), s.output_offset)
        };
        let daemon = self.daemon()?;
        let (data, new_offset) = daemon.read_output(&session_id, offset)?;
        if let Some(s) = self.sessions.get_mut(idx) {
            s.output_offset = new_offset;
        }
        Ok((data, new_offset))
    }

    /// Read the full output buffer for the active session (for parser reset).
    pub fn read_output_bytes_full(&mut self) -> Result<(Vec<u8>, usize)> {
        let idx = self.active_index;
        let session_id = self
            .sessions
            .get(idx)
            .ok_or_else(|| anyhow::anyhow!("no active session"))?
            .session_id
            .clone();
        let daemon = self.daemon()?;
        let (data, new_offset) = daemon.read_output(&session_id, 0)?;
        if let Some(s) = self.sessions.get_mut(idx) {
            s.output_offset = new_offset;
        }
        Ok((data, new_offset))
    }

    /// Read incremental raw bytes from a specific session by index.
    pub fn read_output_bytes_for(&mut self, index: usize) -> Result<(Vec<u8>, usize)> {
        let (session_id, offset) = {
            let s = self
                .sessions
                .get(index)
                .ok_or_else(|| anyhow::anyhow!("no session at index"))?;
            (s.session_id.clone(), s.output_offset)
        };
        let daemon = self.daemon()?;
        daemon.read_output(&session_id, offset)
    }

    /// Resize the active session's PTY.
    pub fn resize_active(&mut self, cols: u16, rows: u16) -> Result<()> {
        let session_id = self
            .active_session()
            .map(|s| s.session_id.clone())
            .ok_or_else(|| anyhow::anyhow!("no active session"))?;
        let daemon = self.daemon()?;
        daemon.resize(&session_id, cols, rows)
    }

    pub fn active_session(&self) -> Option<&Session> {
        self.sessions.get(self.active_index)
    }

    /// Save current tab order to /tmp so it persists across aide restarts.
    pub fn save_tab_order(&self) {
        let names: Vec<&str> = self.sessions.iter().map(|s| s.name.as_str()).collect();
        let _ = std::fs::write(tab_order_file(), names.join("\n"));
    }

    /// Reorder sessions to match the saved tab order.
    pub fn restore_tab_order(&mut self) {
        let data = match std::fs::read_to_string(tab_order_file()) {
            Ok(d) => d,
            Err(_) => return,
        };
        let saved_order: Vec<&str> = data.lines().collect();
        if saved_order.is_empty() {
            return;
        }

        self.sessions.sort_by_key(|s| {
            saved_order
                .iter()
                .position(|n| *n == s.name)
                .unwrap_or(usize::MAX)
        });
    }

    fn next_instance_number(&self, project_name: &str) -> u32 {
        let sanitized = sanitize_name(project_name);
        self.sessions
            .iter()
            .filter(|s| sanitize_name(&s.project_name) == sanitized)
            .map(|s| s.instance_number)
            .max()
            .unwrap_or(0)
            + 1
    }
}

fn tab_order_file() -> String {
    // Use the real UID so multiple users on the same machine don't share state.
    let uid = unsafe { libc::getuid() };
    format!("/tmp/aide-tab-order-{}", uid)
}

/// Sanitize a name for use as a session ID (replace dots and special chars).
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

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
