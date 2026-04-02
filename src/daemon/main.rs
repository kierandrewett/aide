//! aide-daemon: background process managing PTY sessions.
//! Communicates with aide clients via a Unix domain socket.

mod protocol {
    include!("../protocol/mod.rs");
}

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};

use protocol::{Request, Response, SessionInfo};

struct PtySession {
    session_id: String,
    cwd: String,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    output_buf: Vec<u8>,
    alive: bool,
    _child: Box<dyn portable_pty::Child + Send>,
}

type Sessions = Arc<Mutex<HashMap<String, PtySession>>>;

fn main() {
    // Fully detach from the parent's terminal so child PTY processes
    // cannot write to it via /dev/tty
    unsafe {
        libc::setsid();
        libc::ioctl(0, libc::TIOCNOTTY);

        // Close ALL inherited file descriptors above stderr.
        // The parent (aide) has the real terminal on fd 1 (stdout).
        // Even though we set Stdio::null() for 0/1/2, higher fds
        // (duplicated by the runtime, crossterm, etc.) may still
        // point to the real terminal. Child PTY processes inherit
        // these and can write escape sequences that corrupt our display.
        for fd in 3..1024 {
            libc::close(fd);
        }
    }

    let sock_path = protocol::socket_path();
    if let Some(parent) = sock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Remove stale socket
    let _ = std::fs::remove_file(&sock_path);

    let listener = match UnixListener::bind(&sock_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("aide-daemon: failed to bind socket: {}", e);
            std::process::exit(1);
        }
    };

    // Write PID lockfile
    let lock_path = protocol::lock_path();
    let _ = std::fs::write(&lock_path, format!("{}", std::process::id()));

    // Set up log file
    let log_path = protocol::log_path();
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .ok();

    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));

    // Spawn output reader threads for each session
    let log = move |msg: &str| {
        if let Some(ref _f) = log_file {
            // Simple logging — could be enhanced
            let _ = std::io::stderr().write_all(format!("[aide-daemon] {}\n", msg).as_bytes());
        }
    };

    log("daemon started");

    // Set socket to non-blocking isn't needed — we use threads
    listener
        .set_nonblocking(false)
        .expect("set_nonblocking failed");

    // Accept connections in a loop
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let sessions = sessions.clone();
                std::thread::spawn(move || {
                    handle_client(stream, sessions);
                });
            }
            Err(e) => {
                eprintln!("aide-daemon: accept error: {}", e);
            }
        }
    }

    // Cleanup
    let _ = std::fs::remove_file(&sock_path);
    let _ = std::fs::remove_file(&lock_path);
}

fn handle_client(stream: UnixStream, sessions: Sessions) {
    let reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut writer = stream;

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.is_empty() {
            continue;
        }

        let request: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::Error {
                    message: format!("invalid request: {}", e),
                };
                let _ = send_response(&mut writer, &resp);
                continue;
            }
        };

        let response = handle_request(request, &sessions);
        if let Err(_) = send_response(&mut writer, &response) {
            break;
        }

        // If shutdown was requested, exit
        if matches!(response, Response::Ok) {
            // Check if it was a shutdown
        }
    }
}

fn send_response(writer: &mut UnixStream, resp: &Response) -> std::io::Result<()> {
    let json = serde_json::to_string(resp).unwrap();
    writer.write_all(json.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn handle_request(req: Request, sessions: &Sessions) -> Response {
    match req {
        Request::Ping => Response::Pong,

        Request::ListSessions => {
            let map = sessions.lock().unwrap();
            let list: Vec<SessionInfo> = map
                .values()
                .map(|s| SessionInfo {
                    session_id: s.session_id.clone(),
                    cwd: s.cwd.clone(),
                    alive: s.alive,
                })
                .collect();
            Response::SessionList { sessions: list }
        }

        Request::CreateSession {
            session_id,
            cwd,
            command,
            args,
        } => {
            let pty_system = native_pty_system();
            let pair = match pty_system.openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            }) {
                Ok(p) => p,
                Err(e) => {
                    return Response::Error {
                        message: format!("failed to open pty: {}", e),
                    }
                }
            };

            let mut cmd = CommandBuilder::new(&command);
            for arg in &args {
                cmd.arg(arg);
            }
            cmd.cwd(&cwd);

            // Inherit environment
            for (key, val) in std::env::vars() {
                cmd.env(key, val);
            }
            cmd.env("TERM", "xterm-256color");

            let child = match pair.slave.spawn_command(cmd) {
                Ok(c) => c,
                Err(e) => {
                    return Response::Error {
                        message: format!("failed to spawn: {}", e),
                    }
                }
            };

            let pty_writer = pair.master.take_writer().unwrap();
            let mut pty_reader = pair.master.try_clone_reader().unwrap();

            let session = PtySession {
                session_id: session_id.clone(),
                cwd,
                writer: pty_writer,
                master: pair.master,
                output_buf: Vec::with_capacity(64 * 1024),
                alive: true,
                _child: child,
            };

            sessions.lock().unwrap().insert(session_id.clone(), session);

            // Spawn a thread to continuously read PTY output
            let sessions_clone = sessions.clone();
            let sid = session_id.clone();
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match pty_reader.read(&mut buf) {
                        Ok(0) => {
                            // EOF — process exited
                            if let Ok(mut map) = sessions_clone.lock() {
                                if let Some(s) = map.get_mut(&sid) {
                                    s.alive = false;
                                }
                            }
                            break;
                        }
                        Ok(n) => {
                            if let Ok(mut map) = sessions_clone.lock() {
                                if let Some(s) = map.get_mut(&sid) {
                                    s.output_buf.extend_from_slice(&buf[..n]);
                                    // Cap buffer at 2MB
                                    if s.output_buf.len() > 2 * 1024 * 1024 {
                                        let drain = s.output_buf.len() - 1024 * 1024;
                                        s.output_buf.drain(..drain);
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            if let Ok(mut map) = sessions_clone.lock() {
                                if let Some(s) = map.get_mut(&sid) {
                                    s.alive = false;
                                }
                            }
                            break;
                        }
                    }
                }
            });

            Response::SessionCreated { session_id }
        }

        Request::WriteInput { session_id, data } => {
            let mut map = sessions.lock().unwrap();
            match map.get_mut(&session_id) {
                Some(s) => match s.writer.write_all(&data) {
                    Ok(_) => {
                        let _ = s.writer.flush();
                        Response::Ok
                    }
                    Err(e) => Response::Error {
                        message: format!("write failed: {}", e),
                    },
                },
                None => Response::Error {
                    message: format!("session not found: {}", session_id),
                },
            }
        }

        Request::ReadOutput {
            session_id,
            since_offset,
        } => {
            let map = sessions.lock().unwrap();
            match map.get(&session_id) {
                Some(s) => {
                    let start = since_offset.min(s.output_buf.len());
                    let data = s.output_buf[start..].to_vec();
                    let offset = s.output_buf.len();
                    Response::Output { data, offset }
                }
                None => Response::Error {
                    message: format!("session not found: {}", session_id),
                },
            }
        }

        Request::Resize {
            session_id,
            cols,
            rows,
        } => {
            let mut map = sessions.lock().unwrap();
            match map.get_mut(&session_id) {
                Some(s) => {
                    match s.master.resize(PtySize {
                        rows,
                        cols,
                        pixel_width: 0,
                        pixel_height: 0,
                    }) {
                        Ok(_) => Response::Ok,
                        Err(e) => Response::Error {
                            message: format!("resize failed: {}", e),
                        },
                    }
                }
                None => Response::Error {
                    message: format!("session not found: {}", session_id),
                },
            }
        }

        Request::KillSession { session_id } => {
            let mut map = sessions.lock().unwrap();
            if map.remove(&session_id).is_some() {
                Response::Ok
            } else {
                Response::Error {
                    message: format!("session not found: {}", session_id),
                }
            }
        }

        Request::Shutdown => {
            // Spawn a delayed exit to send response first
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(100));
                std::process::exit(0);
            });
            Response::Ok
        }
    }
}
