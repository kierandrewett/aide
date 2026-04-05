//! Client for communicating with the aide-daemon over Unix domain sockets.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};

use crate::protocol::{self, Request, Response, SessionInfo};

pub struct DaemonClient {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
}

impl DaemonClient {
    /// Connect to daemon, starting it if necessary.
    pub fn connect() -> Result<Self> {
        let sock = protocol::socket_path();

        // Try to connect directly
        if let Ok(client) = Self::try_connect(&sock) {
            return Ok(client);
        }

        // Daemon not running — start it
        Self::spawn_daemon()?;

        // Wait for daemon to be ready (up to 3 seconds)
        for _i in 0..30 {
            std::thread::sleep(Duration::from_millis(100));
            if let Ok(client) = Self::try_connect(&sock) {
                return Ok(client);
            }
        }

        anyhow::bail!("failed to connect to aide-daemon after starting it")
    }

    fn try_connect(sock: &PathBuf) -> Result<Self> {
        let stream = UnixStream::connect(sock).context("connect to daemon")?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let reader = BufReader::new(stream.try_clone()?);
        let mut client = Self { stream, reader };

        // Verify with ping
        let resp = client.send(&Request::Ping)?;
        match resp {
            Response::Pong => {}
            _ => anyhow::bail!("unexpected ping response"),
        }

        // Check protocol version — kill mismatched daemons
        match client.send(&Request::Version) {
            Ok(Response::ProtocolVersion { version }) => {
                if version != protocol::PROTOCOL_VERSION {
                    let _ = client.send(&Request::Shutdown);
                    anyhow::bail!(
                        "daemon protocol v{} != client v{}, restarting",
                        version,
                        protocol::PROTOCOL_VERSION
                    );
                }
            }
            _ => {
                // Old daemon doesn't understand Version request — kill it
                let _ = client.send(&Request::Shutdown);
                anyhow::bail!("daemon too old (no version support), restarting");
            }
        }

        Ok(client)
    }

    fn spawn_daemon() -> Result<()> {
        let sock_dir = protocol::socket_path().parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&sock_dir)?;

        // Find our own binary path to locate aide-daemon
        let daemon_path = std::env::current_exe()
            .ok()
            .and_then(|p| {
                let dir = p.parent()?;
                let daemon = dir.join("aide-daemon");
                if daemon.exists() {
                    Some(daemon)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| PathBuf::from("aide-daemon"));

        unsafe {
            Command::new(&daemon_path)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .pre_exec(|| {
                    // Close all inherited fds > 2 before exec.
                    // The parent (aide) has the terminal on stdout and crossterm
                    // may have /dev/tty open on higher fds. Without closing these,
                    // daemon children (Claude Code) inherit them and write escape
                    // sequences directly to our terminal, causing artifacts.
                    for fd in 3..1024 {
                        libc::close(fd);
                    }
                    Ok(())
                })
                .spawn()
                .context("failed to spawn aide-daemon")?;
        }

        Ok(())
    }

    fn send(&mut self, req: &Request) -> Result<Response> {
        let json = serde_json::to_string(req)?;
        self.stream.write_all(json.as_bytes())?;
        self.stream.write_all(b"\n")?;
        self.stream.flush()?;

        let mut line = String::new();
        self.reader.read_line(&mut line)?;
        let resp: Response = serde_json::from_str(&line)?;
        Ok(resp)
    }

    pub fn list_sessions(&mut self) -> Result<Vec<SessionInfo>> {
        match self.send(&Request::ListSessions)? {
            Response::SessionList { sessions } => Ok(sessions),
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("unexpected response"),
        }
    }

    pub fn create_session(
        &mut self,
        session_id: &str,
        cwd: &str,
        command: &str,
        args: &[&str],
    ) -> Result<String> {
        match self.send(&Request::CreateSession {
            session_id: session_id.to_string(),
            cwd: cwd.to_string(),
            command: command.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
        })? {
            Response::SessionCreated { session_id } => Ok(session_id),
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("unexpected response"),
        }
    }

    pub fn write_input(&mut self, session_id: &str, data: &[u8]) -> Result<()> {
        match self.send(&Request::WriteInput {
            session_id: session_id.to_string(),
            data: data.to_vec(),
        })? {
            Response::Ok => Ok(()),
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("unexpected response"),
        }
    }

    pub fn read_output(
        &mut self,
        session_id: &str,
        since_offset: usize,
    ) -> Result<(Vec<u8>, usize)> {
        match self.send(&Request::ReadOutput {
            session_id: session_id.to_string(),
            since_offset,
        })? {
            Response::Output { data, offset } => Ok((data, offset)),
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("unexpected response"),
        }
    }

    pub fn resize(&mut self, session_id: &str, cols: u16, rows: u16) -> Result<()> {
        match self.send(&Request::Resize {
            session_id: session_id.to_string(),
            cols,
            rows,
        })? {
            Response::Ok => Ok(()),
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("unexpected response"),
        }
    }

    pub fn kill_session(&mut self, session_id: &str) -> Result<()> {
        match self.send(&Request::KillSession {
            session_id: session_id.to_string(),
        })? {
            Response::Ok => Ok(()),
            Response::Error { message } => anyhow::bail!("{}", message),
            _ => anyhow::bail!("unexpected response"),
        }
    }
}
