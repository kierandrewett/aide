// Shared protocol types for daemon <-> client IPC over Unix domain sockets.
// Messages are newline-delimited JSON.

use serde::{Deserialize, Serialize};

/// Bump this when the wire format changes (e.g. hex→base64).
/// Client checks on connect; mismatched daemon is killed and respawned.
pub const PROTOCOL_VERSION: u32 = 2;

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
    /// Get protocol version.
    Version,
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
    ProtocolVersion {
        version: u32,
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
#[allow(dead_code)]
pub fn lock_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/tmp/aide-{}", unsafe { libc::getuid() }));
    let dir = std::path::PathBuf::from(runtime_dir).join("aide");
    dir.join("daemon.lock")
}

/// Get the daemon log file path.
#[allow(dead_code)]
pub fn log_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/tmp/aide-{}", unsafe { libc::getuid() }));
    let dir = std::path::PathBuf::from(runtime_dir).join("aide");
    dir.join("daemon.log")
}

mod base64_bytes {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn serialize<S: Serializer>(data: &[u8], ser: S) -> Result<S::Ok, S::Error> {
        let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
        for chunk in data.chunks(3) {
            let b0 = chunk[0] as u32;
            let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
            let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
            let n = (b0 << 16) | (b1 << 8) | b2;
            result.push(CHARS[((n >> 18) & 63) as usize] as char);
            result.push(CHARS[((n >> 12) & 63) as usize] as char);
            if chunk.len() > 1 {
                result.push(CHARS[((n >> 6) & 63) as usize] as char);
            } else {
                result.push('=');
            }
            if chunk.len() > 2 {
                result.push(CHARS[(n & 63) as usize] as char);
            } else {
                result.push('=');
            }
        }
        result.serialize(ser)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
        let encoded = String::deserialize(de)?;
        let bytes = encoded.as_bytes();
        let mut result = Vec::with_capacity(bytes.len() * 3 / 4);

        for chunk in bytes.chunks(4) {
            if chunk.len() < 2 {
                break;
            }
            let a = decode_char(chunk[0]) as u32;
            let b = decode_char(chunk[1]) as u32;
            let c = if chunk.len() > 2 && chunk[2] != b'=' {
                decode_char(chunk[2]) as u32
            } else {
                0
            };
            let d = if chunk.len() > 3 && chunk[3] != b'=' {
                decode_char(chunk[3]) as u32
            } else {
                0
            };
            let n = (a << 18) | (b << 12) | (c << 6) | d;
            result.push((n >> 16) as u8);
            if chunk.len() > 2 && chunk[2] != b'=' {
                result.push((n >> 8) as u8);
            }
            if chunk.len() > 3 && chunk[3] != b'=' {
                result.push(n as u8);
            }
        }
        Ok(result)
    }

    fn decode_char(c: u8) -> u8 {
        match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => 0,
        }
    }
}
