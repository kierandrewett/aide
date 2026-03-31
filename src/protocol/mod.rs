// Shared protocol types for daemon <-> client IPC over Unix domain sockets.
// Messages are newline-delimited JSON.

use serde::{Deserialize, Serialize};

/// Request from client to daemon.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    /// List all active sessions.
    ListSessions,
    /// Create a new session with a given ID, working directory, and command.
    CreateSession {
        session_id: String,
        cwd: String,
        command: String,
        args: Vec<String>,
    },
    /// Write raw bytes to a session's PTY stdin.
    WriteInput {
        session_id: String,
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
    },
    /// Read available output from a session's PTY.
    ReadOutput {
        session_id: String,
        /// If provided, only return output after this byte offset.
        since_offset: usize,
    },
    /// Resize a session's PTY.
    Resize {
        session_id: String,
        cols: u16,
        rows: u16,
    },
    /// Kill/close a session.
    KillSession { session_id: String },
    /// Ping to check daemon is alive.
    Ping,
    /// Ask daemon to shut down.
    Shutdown,
}

/// Response from daemon to client.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Response {
    Ok,
    Error {
        message: String,
    },
    Pong,
    SessionList {
        sessions: Vec<SessionInfo>,
    },
    Output {
        #[serde(with = "base64_bytes")]
        data: Vec<u8>,
        offset: usize,
    },
    SessionCreated {
        session_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub cwd: String,
    pub alive: bool,
}

/// Get the daemon socket path.
pub fn socket_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/tmp/aide-{}", unsafe { libc::getuid() }));
    let dir = std::path::PathBuf::from(runtime_dir).join("aide");
    dir.join("daemon.sock")
}

/// Get the daemon lock file path.
pub fn lock_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/tmp/aide-{}", unsafe { libc::getuid() }));
    let dir = std::path::PathBuf::from(runtime_dir).join("aide");
    dir.join("daemon.lock")
}

/// Get the daemon log file path.
pub fn log_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/tmp/aide-{}", unsafe { libc::getuid() }));
    let dir = std::path::PathBuf::from(runtime_dir).join("aide");
    dir.join("daemon.log")
}

mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(data: &Vec<u8>, ser: S) -> Result<S::Ok, S::Error> {
        // Simple hex encoding for now — fast enough and no extra deps
        let hex: String = data.iter().map(|b| format!("{:02x}", b)).collect();
        hex.serialize(ser)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
        let hex = String::deserialize(de)?;
        let bytes: Result<Vec<u8>, _> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16))
            .collect();
        bytes.map_err(serde::de::Error::custom)
    }
}
