//! Shared text selection state and clipboard utilities.
//!
//! Used by both the aide IDE (PTY panes) and aide-editor (file editing).
//! Coordinates are always `(row: usize, col: usize)`.

use ratatui::style::Color;

/// Selection highlight background — consistent across all surfaces.
pub const SELECTION_BG: Color = Color::Rgb(55, 85, 150);

/// Tracks an in-progress or completed text selection as anchor + end pairs.
#[derive(Clone, Default)]
pub struct SelectionState {
    anchor: Option<(usize, usize)>,
    end: Option<(usize, usize)>,
    /// True while the mouse button is held down.
    pub dragging: bool,
}

impl SelectionState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Called on mouse button down. Resets any existing selection.
    pub fn mouse_down(&mut self, row: usize, col: usize) {
        self.anchor = Some((row, col));
        self.end = None;
        self.dragging = true;
    }

    /// Called during mouse drag. Updates the live end point.
    /// Accepts drags even without a preceding mouse_down (e.g. embedded PTY
    /// where press and release may arrive out of order).
    pub fn mouse_drag(&mut self, row: usize, col: usize) {
        if self.anchor.is_some() {
            self.end = Some((row, col));
            self.dragging = true;
        }
    }

    /// Called on mouse button release. Clears selection if the mouse never moved.
    pub fn mouse_up(&mut self, row: usize, col: usize) {
        self.dragging = false;
        if self.end.is_none()
            || self.end == self.anchor
            || self.end == Some((row, col)) && self.anchor == Some((row, col))
        {
            self.anchor = None;
            self.end = None;
        }
    }

    /// Clear all selection state.
    pub fn clear(&mut self) {
        self.anchor = None;
        self.end = None;
        self.dragging = false;
    }

    /// True if a non-empty selection exists.
    #[allow(dead_code)]
    pub fn has_selection(&self) -> bool {
        self.anchor.is_some() && self.end.is_some() && self.anchor != self.end
    }

    /// Normalized selection bounds: `(start_row, start_col, end_row, end_col)`.
    /// Returns `None` if no selection or selection has no extent.
    pub fn bounds(&self) -> Option<(usize, usize, usize, usize)> {
        let (ar, ac) = self.anchor?;
        let (er, ec) = self.end?;
        if (ar, ac) == (er, ec) {
            return None;
        }
        if ar < er || (ar == er && ac <= ec) {
            Some((ar, ac, er, ec))
        } else {
            Some((er, ec, ar, ac))
        }
    }

    /// True if `(row, col)` falls inside the selection.
    /// `line_cols` is the total number of columns on a row — used to extend
    /// selection to end-of-line for interior rows.
    #[allow(dead_code)]
    pub fn contains(&self, row: usize, col: usize, line_cols: usize) -> bool {
        let (sr, sc, er, ec) = match self.bounds() {
            Some(b) => b,
            None => return false,
        };
        if row < sr || row > er {
            return false;
        }
        let line_start = if row == sr { sc } else { 0 };
        let line_end = if row == er {
            ec
        } else {
            line_cols.saturating_sub(1)
        };
        col >= line_start && col <= line_end
    }
}

/// Extract the selected text from a slice of document lines.
/// `(sr, sc)` is the inclusive start; `(er, ec)` is the inclusive end.
#[allow(dead_code)]
pub fn extract_from_lines(lines: &[String], sr: usize, sc: usize, er: usize, ec: usize) -> String {
    let mut text = String::new();
    for row in sr..=er {
        if row >= lines.len() {
            break;
        }
        let chars: Vec<char> = lines[row].chars().collect();
        let start = if row == sr { sc } else { 0 };
        let end = if row == er { ec } else { chars.len() };
        if row > sr {
            text.push('\n');
        }
        let seg: String = chars[start.min(chars.len())..end.min(chars.len())]
            .iter()
            .collect();
        text.push_str(&seg);
    }
    text
}

/// Send `text` to the system clipboard via the OSC 52 escape sequence.
/// Works in any terminal that supports OSC 52, and through SSH/tmux when
/// `set-clipboard on` is configured.
pub fn copy_to_clipboard(text: &str) {
    use std::io::Write;
    let encoded = base64_encode(text.as_bytes());
    let osc = format!("\x1b]52;c;{}\x07", encoded);
    let _ = std::io::stdout().write_all(osc.as_bytes());
    let _ = std::io::stdout().flush();
}

pub fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(CHARS[((n >> 18) & 63) as usize] as char);
        out.push(CHARS[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            CHARS[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            CHARS[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[allow(dead_code)]
pub fn base64_decode(s: &str) -> Vec<u8> {
    fn val(c: u8) -> u8 {
        match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => 0,
        }
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let a = val(chunk[0]);
        let b = val(chunk[1]);
        out.push((a << 2) | (b >> 4));
        if chunk.len() > 2 && chunk[2] != b'=' {
            let c = val(chunk[2]);
            out.push((b << 4) | (c >> 2));
            if chunk.len() > 3 && chunk[3] != b'=' {
                let d = val(chunk[3]);
                out.push((c << 6) | d);
            }
        }
    }
    out
}
