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

#[cfg(test)]
mod tests {
    use super::*;

    // ── Serialization round-trips ────────────────────────────────────────────

    #[test]
    fn roundtrip_list_sessions() {
        let req = Request::ListSessions;
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, Request::ListSessions));
    }

    #[test]
    fn roundtrip_create_session() {
        let req = Request::CreateSession {
            session_id: "s1".to_string(),
            cwd: "/home/user/dev".to_string(),
            command: "/bin/bash".to_string(),
            args: vec!["-l".to_string()],
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        match back {
            Request::CreateSession {
                session_id,
                cwd,
                command,
                args,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(cwd, "/home/user/dev");
                assert_eq!(command, "/bin/bash");
                assert_eq!(args, vec!["-l"]);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_write_input_binary() {
        // Binary payload including non-UTF-8 bytes
        let payload: Vec<u8> = (0u8..=255u8).collect();
        let req = Request::WriteInput {
            session_id: "sess".to_string(),
            data: payload.clone(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        match back {
            Request::WriteInput { session_id, data } => {
                assert_eq!(session_id, "sess");
                assert_eq!(data, payload);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_write_input_empty() {
        let req = Request::WriteInput {
            session_id: "s".to_string(),
            data: vec![],
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        match back {
            Request::WriteInput { data, .. } => assert!(data.is_empty()),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_read_output() {
        let req = Request::ReadOutput {
            session_id: "s2".to_string(),
            since_offset: 42,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        match back {
            Request::ReadOutput {
                session_id,
                since_offset,
            } => {
                assert_eq!(session_id, "s2");
                assert_eq!(since_offset, 42);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_resize() {
        let req = Request::Resize {
            session_id: "s3".to_string(),
            cols: 200,
            rows: 50,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        match back {
            Request::Resize {
                session_id,
                cols,
                rows,
            } => {
                assert_eq!(session_id, "s3");
                assert_eq!(cols, 200);
                assert_eq!(rows, 50);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_ping_pong() {
        let req = Request::Ping;
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, Request::Ping));

        let resp = Response::Pong;
        let json = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, Response::Pong));
    }

    #[test]
    fn roundtrip_response_error() {
        let resp = Response::Error {
            message: "something went wrong".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        match back {
            Response::Error { message } => assert_eq!(message, "something went wrong"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_response_output_binary() {
        let payload: Vec<u8> = vec![0x1b, b'[', b'1', b'm', b'H', b'i', 0x1b, b'[', b'm'];
        let resp = Response::Output {
            data: payload.clone(),
            offset: 1024,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        match back {
            Response::Output { data, offset } => {
                assert_eq!(data, payload);
                assert_eq!(offset, 1024);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_session_list() {
        let resp = Response::SessionList {
            sessions: vec![
                SessionInfo {
                    session_id: "a".to_string(),
                    cwd: "/tmp".to_string(),
                    alive: true,
                },
                SessionInfo {
                    session_id: "b".to_string(),
                    cwd: "/home".to_string(),
                    alive: false,
                },
            ],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        match back {
            Response::SessionList { sessions } => {
                assert_eq!(sessions.len(), 2);
                assert_eq!(sessions[0].session_id, "a");
                assert!(sessions[0].alive);
                assert!(!sessions[1].alive);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn roundtrip_protocol_version() {
        let resp = Response::ProtocolVersion {
            version: PROTOCOL_VERSION,
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        match back {
            Response::ProtocolVersion { version } => assert_eq!(version, PROTOCOL_VERSION),
            _ => panic!("wrong variant"),
        }
    }

    // ── base64 encoding specifics ────────────────────────────────────────────

    #[test]
    fn base64_known_vectors() {
        // "Man" -> "TWFu"
        let req = Request::WriteInput {
            session_id: "x".to_string(),
            data: b"Man".to_vec(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("TWFu"), "expected base64 'TWFu' in {json}");
    }

    #[test]
    fn base64_padding_one_byte() {
        // single byte 0x00 -> "AA=="
        let req = Request::WriteInput {
            session_id: "x".to_string(),
            data: vec![0x00],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("AA=="), "expected 'AA==' in {json}");
    }

    #[test]
    fn base64_padding_two_bytes() {
        // two bytes 0x00 0x00 -> "AAA="
        let req = Request::WriteInput {
            session_id: "x".to_string(),
            data: vec![0x00, 0x00],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("AAA="), "expected 'AAA=' in {json}");
    }
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
