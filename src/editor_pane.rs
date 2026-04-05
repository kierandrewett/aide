//! Local PTY-backed editor pane.
//! Spawns `editor_command <file>` in a portable-pty pair and exposes a vt100
//! screen for rendering, plus write/resize helpers.

use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtySize};

pub struct EditorPane {
    pub path: String,
    pub parser: vt100::Parser,
    output_buf: Arc<Mutex<Vec<u8>>>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn portable_pty::MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send>,
    /// Scroll state reported by aide-editor via OSC title.
    pub editor_scroll: u64,
    pub editor_total: u64,
    pub editor_view_h: u64,
    pub editor_scroll_col: u64,
    pub editor_max_col: u64,
}

impl EditorPane {
    /// Spawn `editor_command <path>` in a new local PTY of size `rows × cols`.
    pub fn spawn(editor_command: &str, path: &str, rows: u16, cols: u16, theme: &str) -> Result<Self> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // Split the command string into program + pre-file args
        let parts: Vec<&str> = editor_command.split_whitespace().collect();
        let (prog, pre_args) = parts
            .split_first()
            .ok_or_else(|| anyhow::anyhow!("empty editor command"))?;

        let mut cmd = CommandBuilder::new(prog);
        for arg in pre_args {
            cmd.arg(arg);
        }
        cmd.arg(path);
        for (k, v) in std::env::vars() {
            cmd.env(k, v);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("AIDE_EMBEDDED", "1");
        cmd.env("AIDE_THEME", theme);

        let child = pair.slave.spawn_command(cmd)?;
        let writer = pair.master.take_writer()?;
        let mut reader = pair.master.try_clone_reader()?;

        let output_buf: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let buf2 = output_buf.clone();

        std::thread::spawn(move || {
            let mut tmp = [0u8; 4096];
            loop {
                match reader.read(&mut tmp) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let _ = buf2.lock().map(|mut b| b.extend_from_slice(&tmp[..n]));
                    }
                }
            }
        });

        Ok(Self {
            path: path.to_string(),
            parser: vt100::Parser::new(rows, cols, 0),
            output_buf,
            writer,
            master: pair.master,
            child,
            editor_scroll: 0,
            editor_total: 1,
            editor_view_h: rows as u64,
            editor_scroll_col: 0,
            editor_max_col: 0,
        })
    }

    /// Drain pending output into the vt100 parser. Returns true if new bytes
    /// were processed (caller should mark the UI dirty).
    pub fn drain(&mut self) -> bool {
        let bytes = match self.output_buf.lock() {
            Ok(mut buf) if !buf.is_empty() => std::mem::take(&mut *buf),
            _ => return false,
        };

        // Parse aide-editor scroll state from OSC title sequences:
        // "\x1b]2;aide:{scroll_row}/{total}/{view_h}/{scroll_col}/{max_col}\x07"
        if let Ok(s) = std::str::from_utf8(&bytes) {
            let marker = "\x1b]2;aide:";
            if let Some(start) = s.find(marker) {
                let rest = &s[start + marker.len()..];
                if let Some(end) = rest.find('\x07') {
                    let parts: Vec<&str> = rest[..end].split('/').collect();
                    if let Some(v) = parts.first().and_then(|x| x.parse::<u64>().ok()) {
                        self.editor_scroll = v;
                    }
                    if let Some(v) = parts.get(1).and_then(|x| x.parse::<u64>().ok()) {
                        self.editor_total = v;
                    }
                    if let Some(v) = parts.get(2).and_then(|x| x.parse::<u64>().ok()) {
                        self.editor_view_h = v;
                    }
                    if let Some(v) = parts.get(3).and_then(|x| x.parse::<u64>().ok()) {
                        self.editor_scroll_col = v;
                    }
                    if let Some(v) = parts.get(4).and_then(|x| x.parse::<u64>().ok()) {
                        self.editor_max_col = v;
                    }
                }
            }
        }

        self.parser.process(&bytes);
        true
    }

    /// Send bytes to the editor's stdin (key events, paste, etc.).
    pub fn write_input(&mut self, data: &[u8]) {
        let _ = self.writer.write_all(data);
    }

    /// Resize the PTY and update the vt100 parser dimensions.
    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.parser.screen_mut().set_size(rows, cols);
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    /// Check if the editor process is still running.
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}
