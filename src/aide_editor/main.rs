//! aide-editor — a minimal TUI text editor with tree-sitter syntax highlighting.
//!
//! Usage: aide-editor <file>
//! Ctrl+S  save
//! Ctrl+Q / Ctrl+X  quit (prompts if unsaved)
//! Ctrl+Z  undo

use std::io;
use std::ops::Range;
use std::path::Path;
use std::time::Duration;

use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyModifiers, KeyboardEnhancementFlags, MouseButton, MouseEvent,
        MouseEventKind, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame, Terminal,
};
use tree_sitter_highlight::{Highlight, HighlightConfiguration, HighlightEvent, Highlighter};
use tree_sitter_md;
use tree_sitter_yaml;

#[path = "../selection.rs"]
mod selection;
use selection::SelectionState;

/// Columns reserved for the line-number gutter (not part of the text content).
const GUTTER: u16 = 5;
/// Rows reserved at the bottom for the separator line + status bar.
const BOTTOM_ROWS: u16 = 2;

// ---------------------------------------------------------------------------
// Highlight name → Color mapping
// ---------------------------------------------------------------------------

const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "embedded",
    "function",
    "function.builtin",
    "function.method",
    "keyword",
    "keyword.control",
    "label",
    "namespace",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.escape",
    "tag",
    "text.emphasis",
    "text.literal",
    "text.reference",
    "text.strong",
    "text.title",
    "text.uri",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
];

struct ThemeColors {
    keyword: Color,
    function: Color,
    type_: Color,
    string: Color,
    number: Color,
    comment: Color,
    constant: Color,
    operator: Color,
    attribute: Color,
    namespace: Color,
    property: Color,
    tag: Color,
    variable: Color,
    default: Color,
}

fn theme_colors(theme: &str) -> ThemeColors {
    match theme {
        "one-dark" => ThemeColors {
            keyword: Color::Rgb(198, 120, 221), // #c678dd purple
            function: Color::Rgb(97, 175, 239), // #61afef blue
            type_: Color::Rgb(229, 192, 123),   // #e5c07b yellow
            string: Color::Rgb(152, 195, 121),  // #98c379 green
            number: Color::Rgb(209, 154, 102),  // #d19a66 orange
            comment: Color::Rgb(92, 99, 112),   // #5c6370 gray
            constant: Color::Rgb(209, 154, 102),
            operator: Color::Rgb(86, 182, 194),   // #56b6c2 teal
            attribute: Color::Rgb(224, 108, 117), // #e06c75 red
            namespace: Color::Rgb(229, 192, 123),
            property: Color::Rgb(171, 178, 191), // #abb2bf
            tag: Color::Rgb(224, 108, 117),
            variable: Color::Rgb(224, 108, 117),
            default: Color::Rgb(171, 178, 191),
        },
        "dracula" => ThemeColors {
            keyword: Color::Rgb(255, 121, 198), // #ff79c6 pink
            function: Color::Rgb(80, 250, 123), // #50fa7b green
            type_: Color::Rgb(139, 233, 253),   // #8be9fd cyan
            string: Color::Rgb(241, 250, 140),  // #f1fa8c yellow
            number: Color::Rgb(189, 147, 249),  // #bd93f9 purple
            comment: Color::Rgb(98, 114, 164),  // #6272a4 blue-gray
            constant: Color::Rgb(189, 147, 249),
            operator: Color::Rgb(255, 121, 198),
            attribute: Color::Rgb(255, 121, 198),
            namespace: Color::Rgb(139, 233, 253),
            property: Color::Rgb(248, 248, 242), // #f8f8f2
            tag: Color::Rgb(255, 121, 198),
            variable: Color::Rgb(248, 248, 242),
            default: Color::Rgb(248, 248, 242),
        },
        "nord" => ThemeColors {
            keyword: Color::Rgb(129, 161, 193),  // #81a1c1 frost blue
            function: Color::Rgb(136, 192, 208), // #88c0d0 light frost
            type_: Color::Rgb(143, 188, 187),    // #8fbcbb teal
            string: Color::Rgb(163, 190, 140),   // #a3be8c green
            number: Color::Rgb(180, 142, 173),   // #b48ead purple
            comment: Color::Rgb(76, 86, 106),    // #4c566a dark gray
            constant: Color::Rgb(180, 142, 173),
            operator: Color::Rgb(129, 161, 193),
            attribute: Color::Rgb(191, 97, 106), // #bf616a red
            namespace: Color::Rgb(143, 188, 187),
            property: Color::Rgb(216, 222, 233), // #d8dee9
            tag: Color::Rgb(129, 161, 193),
            variable: Color::Rgb(216, 222, 233),
            default: Color::Rgb(216, 222, 233),
        },
        "monokai" => ThemeColors {
            keyword: Color::Rgb(249, 38, 114),  // #f92672 red/pink
            function: Color::Rgb(166, 226, 46), // #a6e22e green
            type_: Color::Rgb(102, 217, 232),   // #66d9e8 cyan
            string: Color::Rgb(230, 219, 116),  // #e6db74 yellow
            number: Color::Rgb(174, 129, 255),  // #ae81ff purple
            comment: Color::Rgb(117, 113, 94),  // #75715e brown-gray
            constant: Color::Rgb(174, 129, 255),
            operator: Color::Rgb(249, 38, 114),
            attribute: Color::Rgb(166, 226, 46),
            namespace: Color::Rgb(166, 226, 46),
            property: Color::Rgb(248, 248, 242), // #f8f8f2
            tag: Color::Rgb(249, 38, 114),
            variable: Color::Rgb(248, 248, 242),
            default: Color::Rgb(248, 248, 242),
        },
        "solarized-dark" => ThemeColors {
            keyword: Color::Rgb(133, 153, 0),   // #859900 green
            function: Color::Rgb(38, 139, 210), // #268bd2 blue
            type_: Color::Rgb(181, 137, 0),     // #b58900 yellow
            string: Color::Rgb(42, 161, 152),   // #2aa198 cyan
            number: Color::Rgb(211, 54, 130),   // #d33682 magenta
            comment: Color::Rgb(88, 110, 117),  // #586e75 base01
            constant: Color::Rgb(203, 75, 22),  // #cb4b16 orange
            operator: Color::Rgb(133, 153, 0),
            attribute: Color::Rgb(203, 75, 22),
            namespace: Color::Rgb(38, 139, 210),
            property: Color::Rgb(131, 148, 150), // #839496 base0
            tag: Color::Rgb(38, 139, 210),
            variable: Color::Rgb(131, 148, 150),
            default: Color::Rgb(131, 148, 150),
        },
        // "github-dark" and default
        _ => ThemeColors {
            keyword: Color::Rgb(255, 123, 114),  // #ff7b72 red
            function: Color::Rgb(210, 168, 255), // #d2a8ff purple
            type_: Color::Rgb(255, 166, 87),     // #ffa657 orange
            string: Color::Rgb(165, 214, 255),   // #a5d6ff light blue
            number: Color::Rgb(121, 192, 255),   // #79c0ff blue
            comment: Color::Rgb(139, 148, 158),  // #8b949e gray
            constant: Color::Rgb(121, 192, 255),
            operator: Color::Rgb(255, 123, 114),
            attribute: Color::Rgb(255, 123, 114),
            namespace: Color::Rgb(255, 166, 87),
            property: Color::Rgb(230, 237, 243), // #e6edf3
            tag: Color::Rgb(126, 231, 135),      // #7ee787 green
            variable: Color::Rgb(255, 123, 114),
            default: Color::Rgb(230, 237, 243),
        },
    }
}

fn highlight_color(idx: usize, theme: &str) -> Color {
    let t = theme_colors(theme);
    match HIGHLIGHT_NAMES.get(idx).copied().unwrap_or("") {
        "keyword" | "keyword.control" => t.keyword,
        "function" | "function.builtin" | "function.method" => t.function,
        "type" | "type.builtin" | "constructor" => t.type_,
        "string" | "string.escape" | "embedded" => t.string,
        "number" | "constant.builtin" | "boolean" => t.number,
        "comment" => t.comment,
        "constant" => t.constant,
        "operator"
        | "punctuation"
        | "punctuation.bracket"
        | "punctuation.delimiter"
        | "punctuation.special" => t.operator,
        "attribute" | "label" => t.attribute,
        "namespace" => t.namespace,
        "property" => t.property,
        "tag" => t.tag,
        "variable" | "variable.parameter" | "variable.builtin" => t.variable,
        // Markdown-specific
        "text.title" => t.keyword,
        "text.literal" => t.string,
        "text.emphasis" => t.type_,
        "text.strong" => t.function,
        "text.uri" => t.string,
        "text.reference" => t.attribute,
        _ => t.default,
    }
}

// ---------------------------------------------------------------------------
// Language detection + highlight config
// ---------------------------------------------------------------------------

/// Map a VSCode/linguist language ID to a tree-sitter extension key.
fn lang_id_to_ext(lang: &str) -> Option<&'static str> {
    match lang.to_ascii_lowercase().as_str() {
        "rust" => Some("rs"),
        "python" => Some("py"),
        "javascript" => Some("js"),
        "typescript" => Some("ts"),
        "typescriptreact" | "tsx" => Some("tsx"),
        "go" => Some("go"),
        "shellscript" | "bash" | "sh" | "shell" => Some("sh"),
        "json" | "jsonc" => Some("json"),
        "html" => Some("html"),
        "css" => Some("css"),
        "toml" => Some("toml"),
        "markdown" => Some("md"),
        "yaml" => Some("yaml"),
        _ => None,
    }
}

/// Try to determine language from `.vscode/settings.json` `files.associations`
/// and `.gitattributes` `linguist-language` entries for the given file path.
/// Returns a tree-sitter extension key like "rs", "py", etc.
fn detect_lang_from_project_config(path: &str) -> Option<&'static str> {
    let abs = std::path::Path::new(path);

    // Walk up to find project root (directory containing .vscode or .gitattributes)
    let mut dir = abs.parent();
    while let Some(d) = dir {
        // Try .vscode/settings.json
        let settings_path = d.join(".vscode/settings.json");
        if settings_path.exists() {
            if let Some(ext) = check_vscode_associations(&settings_path, abs) {
                return Some(ext);
            }
        }
        // Try .gitattributes
        let gitattrs_path = d.join(".gitattributes");
        if gitattrs_path.exists() {
            if let Some(ext) = check_gitattributes(&gitattrs_path, abs) {
                return Some(ext);
            }
        }
        // Stop at filesystem root or if we found a .git directory
        if d.join(".git").exists() {
            break;
        }
        dir = d.parent();
    }
    None
}

/// Check `.vscode/settings.json` `files.associations` for a matching glob pattern.
fn check_vscode_associations(
    settings_path: &std::path::Path,
    file: &std::path::Path,
) -> Option<&'static str> {
    let content = std::fs::read_to_string(settings_path).ok()?;
    // Minimal JSON parsing: find "files.associations" object
    let key = "\"files.associations\"";
    let start = content.find(key)?;
    let after = &content[start + key.len()..];
    // Find the opening brace of the map
    let brace = after.find('{')?;
    let map_str = &after[brace + 1..];
    let end = map_str.find('}')?;
    let map_str = &map_str[..end];

    let file_name = file.file_name()?.to_str()?;

    // Parse key-value pairs like "*.toml": "toml"
    for pair in map_str.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        // Extract pattern and language strings
        let mut parts = pair.splitn(2, ':');
        let glob_raw = parts.next()?.trim().trim_matches('"');
        let lang_raw = parts
            .next()?
            .trim()
            .trim_matches(|c: char| c == '"' || c.is_whitespace());
        // Simple glob matching: only support * wildcard
        if glob_matches(glob_raw, file_name) {
            return lang_id_to_ext(lang_raw);
        }
    }
    None
}

/// Check `.gitattributes` for `linguist-language=LANG` attributes matching the file.
fn check_gitattributes(
    gitattrs_path: &std::path::Path,
    file: &std::path::Path,
) -> Option<&'static str> {
    let content = std::fs::read_to_string(gitattrs_path).ok()?;
    let file_name = file.file_name()?.to_str()?;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let pattern = parts.next()?;
        // Only match on filename part of pattern for simplicity
        let pattern_name = std::path::Path::new(pattern)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(pattern);
        if !glob_matches(pattern_name, file_name) {
            continue;
        }
        // Look for linguist-language=LANG in remaining attrs
        for attr in parts {
            if let Some(lang) = attr.strip_prefix("linguist-language=") {
                return lang_id_to_ext(lang);
            }
        }
    }
    None
}

/// Minimal glob matching supporting `*` (matches anything except `/`) and `?`.
fn glob_matches(pattern: &str, name: &str) -> bool {
    let pat: Vec<char> = pattern.chars().collect();
    let nm: Vec<char> = name.chars().collect();
    glob_matches_inner(&pat, &nm)
}

fn glob_matches_inner(pat: &[char], name: &[char]) -> bool {
    match (pat.first(), name.first()) {
        (None, None) => true,
        (Some('*'), _) => {
            // * matches zero or more characters (not /)
            glob_matches_inner(&pat[1..], name)
                || (!name.is_empty() && name[0] != '/' && glob_matches_inner(pat, &name[1..]))
        }
        (Some('?'), Some(_)) => glob_matches_inner(&pat[1..], &name[1..]),
        (Some(p), Some(n)) if p == n => glob_matches_inner(&pat[1..], &name[1..]),
        _ => false,
    }
}

/// Detect language from filename or file content (first 20 lines).
/// Returns a file extension string like "sh", "py", "rs" that can be fed to
/// `LangConfig::from_path`. Returns None if unknown.
fn detect_lang_by_name_or_content(fname: &str, path: &str) -> Option<&'static str> {
    // Well-known exact filenames
    match fname {
        "makefile" | "gnumakefile" | "bsdmakefile" => return None, // no grammar
        "dockerfile" | "containerfile" => return None,
        ".bashrc" | ".bash_profile" | ".bash_logout" | ".profile" => return Some("sh"),
        ".zshrc" | ".zprofile" | ".zshenv" | ".zlogin" | ".zlogout" => return Some("sh"),
        ".fishrc" | "config.fish" => return Some("sh"),
        ".inputrc" => return None,
        "gemfile" | "rakefile" | "podfile" | "vagrantfile" => return None,
        "brewfile" => return None,
        "justfile" => return None,
        "pyproject.toml" => return Some("toml"),
        _ => {}
    }

    // Well-known filename prefixes / suffixes
    if fname.starts_with(".env") || fname.ends_with(".env") {
        return None; // plain text
    }

    // Read first 20 lines for content-based detection
    let head: Vec<String> = std::fs::File::open(path)
        .ok()
        .and_then(|f| {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(f);
            Some(reader.lines().take(20).filter_map(|l| l.ok()).collect())
        })
        .unwrap_or_default();

    if head.is_empty() {
        return None;
    }

    let first = head[0].trim();

    // Vim modeline in first or last line: "vim: set ft=LANG:" or "# vim: ft=LANG"
    for line in head.iter() {
        let l = line.trim();
        if let Some(pos) = l.find("vim:") {
            let after = &l[pos + 4..];
            for part in after.split_whitespace().chain(after.split(':')) {
                let p = part.trim_matches(|c: char| !c.is_alphanumeric() && c != '=');
                if let Some(ft) = p
                    .strip_prefix("ft=")
                    .or_else(|| p.strip_prefix("filetype="))
                {
                    let ft = ft.trim_end_matches(|c: char| !c.is_alphanumeric());
                    return match ft {
                        "sh" | "bash" | "zsh" => Some("sh"),
                        "python" | "py" => Some("py"),
                        "rust" | "rs" => Some("rs"),
                        "javascript" | "js" => Some("js"),
                        "typescript" | "ts" => Some("ts"),
                        "go" => Some("go"),
                        "json" => Some("json"),
                        "toml" => Some("toml"),
                        "html" => Some("html"),
                        "css" => Some("css"),
                        "markdown" | "md" => Some("md"),
                        "ruby" | "rb" => Some("rb"),
                        _ => None,
                    };
                }
            }
        }
    }

    // Shebang detection
    if first.starts_with("#!") {
        let shebang = first.trim_start_matches("#!").trim();
        // Handle "/usr/bin/env python3" style
        let bin = shebang.split_whitespace().last().unwrap_or(shebang);
        let bin = bin.rsplit('/').next().unwrap_or(bin);
        return match bin {
            "bash" | "sh" | "dash" | "ash" | "ksh" | "zsh" | "fish" => Some("sh"),
            b if b.starts_with("python") => Some("py"),
            b if b.starts_with("ruby") || b == "rake" => None,
            b if b.starts_with("node") => Some("js"),
            b if b.starts_with("deno") => Some("ts"),
            "perl" => None,
            "lua" => None,
            _ => None,
        };
    }

    // Heuristic: look for strong language markers in first few lines
    let text: String = head.join("\n");
    if text.contains("fn main()") || text.contains("use std::") || text.contains("impl ") {
        return Some("rs");
    }
    if text.contains("def ")
        && (text.contains("import ") || text.contains("class ") || text.contains("print("))
    {
        return Some("py");
    }
    if text.contains("package main") || text.contains("import (") {
        return Some("go");
    }
    if text.contains("const ") && (text.contains("=>") || text.contains("require(")) {
        return Some("js");
    }

    None
}

struct LangConfig {
    hl_config: HighlightConfiguration,
    lang_name: &'static str,
}

impl LangConfig {
    fn from_path(path: &str) -> Option<Self> {
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let fname = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        let (language, name, hq, iq, lq): (tree_sitter::Language, &str, &str, &str, &str) =
            match ext.as_str() {
                "rs" => (
                    tree_sitter::Language::new(tree_sitter_rust::LANGUAGE),
                    "rust",
                    tree_sitter_rust::HIGHLIGHTS_QUERY,
                    tree_sitter_rust::INJECTIONS_QUERY,
                    "",
                ),
                "py" | "pyw" | "pyi" => (
                    tree_sitter::Language::new(tree_sitter_python::LANGUAGE),
                    "python",
                    tree_sitter_python::HIGHLIGHTS_QUERY,
                    "",
                    "",
                ),
                "js" | "mjs" | "cjs" => (
                    tree_sitter::Language::new(tree_sitter_javascript::LANGUAGE),
                    "javascript",
                    tree_sitter_javascript::HIGHLIGHT_QUERY,
                    tree_sitter_javascript::INJECTIONS_QUERY,
                    tree_sitter_javascript::LOCALS_QUERY,
                ),
                "ts" => (
                    tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
                    "typescript",
                    tree_sitter_typescript::HIGHLIGHTS_QUERY,
                    "",
                    tree_sitter_typescript::LOCALS_QUERY,
                ),
                "tsx" => (
                    tree_sitter::Language::new(tree_sitter_typescript::LANGUAGE_TSX),
                    "tsx",
                    tree_sitter_typescript::HIGHLIGHTS_QUERY,
                    "",
                    tree_sitter_typescript::LOCALS_QUERY,
                ),
                "go" => (
                    tree_sitter::Language::new(tree_sitter_go::LANGUAGE),
                    "go",
                    tree_sitter_go::HIGHLIGHTS_QUERY,
                    "",
                    "",
                ),
                "sh" | "bash" | "zsh" | "fish" => (
                    tree_sitter::Language::new(tree_sitter_bash::LANGUAGE),
                    "bash",
                    tree_sitter_bash::HIGHLIGHT_QUERY,
                    "",
                    "",
                ),
                "json" => (
                    tree_sitter::Language::new(tree_sitter_json::LANGUAGE),
                    "json",
                    tree_sitter_json::HIGHLIGHTS_QUERY,
                    "",
                    "",
                ),
                "html" | "htm" => (
                    tree_sitter::Language::new(tree_sitter_html::LANGUAGE),
                    "html",
                    tree_sitter_html::HIGHLIGHTS_QUERY,
                    tree_sitter_html::INJECTIONS_QUERY,
                    "",
                ),
                "css" => (
                    tree_sitter::Language::new(tree_sitter_css::LANGUAGE),
                    "css",
                    tree_sitter_css::HIGHLIGHTS_QUERY,
                    "",
                    "",
                ),
                "toml" => (
                    tree_sitter::Language::new(tree_sitter_toml_ng::LANGUAGE),
                    "toml",
                    tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
                    "",
                    "",
                ),
                "md" | "mdx" | "markdown" => (
                    tree_sitter::Language::new(tree_sitter_md::LANGUAGE),
                    "markdown",
                    tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
                    "",
                    "",
                ),
                "yaml" | "yml" => (
                    tree_sitter::Language::new(tree_sitter_yaml::LANGUAGE),
                    "yaml",
                    tree_sitter_yaml::HIGHLIGHTS_QUERY,
                    "",
                    "",
                ),
                _ => {
                    // Fallback: project config (vscode/gitattributes), then filename/content sniff
                    let detected = detect_lang_from_project_config(path)
                        .or_else(|| detect_lang_by_name_or_content(&fname, path));
                    return detected.and_then(|det_ext| Self::from_ext(det_ext));
                }
            };

        let mut hl_config = HighlightConfiguration::new(language, name, hq, iq, lq).ok()?;
        hl_config.configure(HIGHLIGHT_NAMES);
        Some(LangConfig {
            hl_config,
            lang_name: name,
        })
    }

    /// Create a LangConfig from a detected extension string (e.g. "sh", "py").
    fn from_ext(ext: &str) -> Option<Self> {
        Self::from_path(&format!("file.{}", ext))
    }

    fn name(&self) -> &str {
        self.lang_name
    }
}

// ---------------------------------------------------------------------------
// Highlight span: (line, col_byte_start, col_byte_end, color)
// ---------------------------------------------------------------------------

/// Per-line pre-computed highlight spans (byte ranges within the line).
type LineHighlights = Vec<(Range<usize>, Color)>;

fn compute_highlights(source: &str, lang: &mut LangConfig, theme: &str) -> Vec<LineHighlights> {
    let mut result: Vec<LineHighlights> = source.lines().map(|_| Vec::new()).collect();
    if result.is_empty() {
        return result;
    }

    let mut hl = Highlighter::new();
    let events = match hl.highlight(&lang.hl_config, source.as_bytes(), None, |_| None) {
        Ok(e) => e,
        Err(_) => return result,
    };

    // Build line byte-offset index
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(source.char_indices().filter_map(
            |(i, c)| {
                if c == '\n' {
                    Some(i + 1)
                } else {
                    None
                }
            },
        ))
        .collect();

    let mut current_hl: Option<Highlight> = None;
    for event in events.flatten() {
        match event {
            HighlightEvent::HighlightStart(h) => current_hl = Some(h),
            HighlightEvent::HighlightEnd => current_hl = None,
            HighlightEvent::Source { start, end } => {
                if let Some(h) = current_hl {
                    let color = highlight_color(h.0, theme);
                    // Map byte range [start, end) to lines
                    let s_line = line_starts
                        .partition_point(|&ls| ls <= start)
                        .saturating_sub(1);
                    let e_line = line_starts
                        .partition_point(|&ls| ls <= end.saturating_sub(1))
                        .saturating_sub(1);
                    for line_idx in s_line..=e_line {
                        if line_idx >= result.len() {
                            break;
                        }
                        let ls = line_starts[line_idx];
                        let line_end = line_starts
                            .get(line_idx + 1)
                            .map(|&e| e.saturating_sub(1))
                            .unwrap_or(source.len());
                        let col_start = start.saturating_sub(ls).min(line_end - ls);
                        let col_end = end.min(line_end).saturating_sub(ls);
                        if col_start < col_end {
                            result[line_idx].push((col_start..col_end, color));
                        }
                    }
                }
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Binary file helpers
// ---------------------------------------------------------------------------

/// Returns true if the first 8192 bytes contain a null byte (binary file heuristic).
fn detect_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

/// Returns true if the file looks like an image (magic bytes or extension).
fn detect_image(bytes: &[u8], path: &str) -> bool {
    // Magic bytes
    if bytes.starts_with(b"\x89PNG") {
        return true;
    }
    if bytes.starts_with(b"\xFF\xD8\xFF") {
        return true;
    }
    if bytes.starts_with(b"GIF8") {
        return true;
    }
    if bytes.starts_with(b"RIFF") && bytes.len() >= 12 && &bytes[8..12] == b"WEBP" {
        return true;
    }
    if bytes.starts_with(b"BM") {
        return true;
    }
    if bytes.starts_with(b"\x49\x49\x2A\x00") || bytes.starts_with(b"\x4D\x4D\x00\x2A") {
        return true;
    }
    if bytes.starts_with(b"\x00\x00\x01\x00") {
        return true;
    }
    // Extension fallback
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tiff" | "tif" | "avif"
    )
}

/// Try to get a PNG representation of the image.
/// Returns Some(png_bytes) if successful.
fn prepare_preview_png(bytes: &[u8], path: &str) -> Option<Vec<u8>> {
    if bytes.starts_with(b"\x89PNG") {
        return Some(bytes.to_vec());
    }
    // Try ImageMagick convert
    let output = std::process::Command::new("convert")
        .arg(path)
        .arg("png:-")
        .output()
        .ok()?;
    if output.status.success() && !output.stdout.is_empty() {
        Some(output.stdout)
    } else {
        None
    }
}

/// Returns true if the terminal supports Kitty graphics protocol.
fn supports_kitty_graphics() -> bool {
    std::env::var("KITTY_WINDOW_ID").is_ok()
        || std::env::var("TERM")
            .map(|t| t == "xterm-kitty")
            .unwrap_or(false)
}

/// Convert binary bytes to a displayable string (nano/vi style).
fn binary_to_display_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        match b {
            b'\n' => out.push('\n'),
            b'\r' => out.push_str("^M"),
            b'\t' => out.push_str("^I"),
            0..=31 => {
                // ^@ through ^_
                out.push('^');
                out.push((b + b'@') as char);
            }
            127 => out.push_str("^?"),
            128..=255 => {
                let low = b & 0x7f;
                out.push_str("M-");
                match low {
                    b'\r' => out.push_str("^M"),
                    b'\t' => out.push_str("^I"),
                    0..=31 => {
                        out.push('^');
                        out.push((low + b'@') as char);
                    }
                    127 => out.push_str("^?"),
                    _ => out.push(low as char),
                }
            }
            _ => out.push(b as char),
        }
    }
    out
}

/// Emit a Kitty graphics protocol image sequence to stdout.
fn emit_kitty_image(png_data: &[u8], x: u16, y: u16, cols: u16, rows: u16) {
    use std::io::Write as _;
    let b64 = base64_encode(png_data);
    let b64_bytes = b64.as_bytes();
    let chunk_size = 4096;
    let mut out = io::stdout();

    // Move cursor to position (1-indexed)
    let _ = write!(out, "\x1b[{};{}H", y + 1, x + 1);

    let total_chunks = (b64_bytes.len() + chunk_size - 1).max(1) / chunk_size;
    if total_chunks == 0 {
        return;
    }

    for (i, chunk) in b64_bytes.chunks(chunk_size).enumerate() {
        let chunk_str = std::str::from_utf8(chunk).unwrap_or("");
        if i == 0 && total_chunks == 1 {
            // Only chunk
            let _ = write!(
                out,
                "\x1b_Ga=T,f=100,q=2,c={},r={},m=0;{}\x1b\\",
                cols, rows, chunk_str
            );
        } else if i == 0 {
            // First of many
            let _ = write!(
                out,
                "\x1b_Ga=T,f=100,q=2,c={},r={},m=1;{}\x1b\\",
                cols, rows, chunk_str
            );
        } else if i == total_chunks - 1 {
            // Last chunk
            let _ = write!(out, "\x1b_Gm=0;{}\x1b\\", chunk_str);
        } else {
            // Middle chunk
            let _ = write!(out, "\x1b_Gm=1;{}\x1b\\", chunk_str);
        }
    }

    let _ = out.flush();
}

// ---------------------------------------------------------------------------
// Editor state
// ---------------------------------------------------------------------------

struct UndoEntry {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

struct Editor {
    file_path: String,
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    scroll_row: usize,
    scroll_col: usize,
    modified: bool,
    lang: Option<LangConfig>,
    highlights: Vec<LineHighlights>,
    undo_stack: Vec<UndoEntry>,
    // Inner viewport dimensions set each frame
    inner_h: u16,
    inner_w: u16,
    // Quit state
    quit_confirm: bool,
    /// True when running embedded inside aide (AIDE_EMBEDDED=1).
    /// Suppresses aide-editor's own scrollbars so aide can render its own.
    embedded: bool,
    /// Syntax highlight theme name (from AIDE_THEME env var).
    theme: String,
    // Text selection state (shared component)
    selection: SelectionState,
    // File watching
    file_mtime: Option<std::time::SystemTime>,
    external_modified: bool,
    override_confirm: bool,
    // Frame counter for periodic file-watch checks
    frame_count: u32,
    // Binary file state
    is_binary: bool,
    binary_warning_dismissed: bool,
    is_image: bool,
    show_image_preview: bool,
    preview_png: Option<Vec<u8>>,
    image_needs_render: bool,
    last_image_area: Rect,
}

impl Editor {
    fn open(path: &str) -> Self {
        let raw_bytes = std::fs::read(path).unwrap_or_default();
        let is_binary = detect_binary(&raw_bytes);
        let is_image = is_binary && detect_image(&raw_bytes, path);
        let preview_png = if is_image {
            prepare_preview_png(&raw_bytes, path)
        } else {
            None
        };
        let kitty = supports_kitty_graphics();
        let show_image_preview = is_image && kitty && preview_png.is_some();

        let content = if is_binary {
            binary_to_display_string(&raw_bytes)
        } else {
            String::from_utf8_lossy(&raw_bytes).to_string()
        };

        let lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            let mut v: Vec<String> = content.lines().map(|l| l.replace('\r', "")).collect();
            if content.ends_with('\n') {
                v.push(String::new());
            }
            if v.is_empty() {
                v.push(String::new());
            }
            v
        };

        let mut lang = LangConfig::from_path(path);
        let source = lines.join("\n");
        let init_theme = std::env::var("AIDE_THEME").unwrap_or_else(|_| "github-dark".to_string());
        let highlights = lang
            .as_mut()
            .map(|l| compute_highlights(&source, l, &init_theme))
            .unwrap_or_default();
        let file_mtime = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());

        Editor {
            file_path: path.to_string(),
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            modified: false,
            lang,
            highlights,
            undo_stack: Vec::new(),
            inner_h: 24,
            inner_w: 80,
            quit_confirm: false,
            embedded: std::env::var("AIDE_EMBEDDED").is_ok(),
            theme: std::env::var("AIDE_THEME").unwrap_or_else(|_| "github-dark".to_string()),
            selection: SelectionState::new(),
            file_mtime,
            external_modified: false,
            override_confirm: false,
            frame_count: 0,
            is_binary,
            binary_warning_dismissed: false,
            is_image,
            show_image_preview,
            preview_png,
            image_needs_render: show_image_preview,
            last_image_area: Rect::default(),
        }
    }

    fn line_count(&self) -> usize {
        self.lines.len()
    }
    fn current_line_len(&self) -> usize {
        self.line_char_len(self.cursor_row)
    }
    fn line_char_len(&self, row: usize) -> usize {
        self.lines.get(row).map(|l| l.chars().count()).unwrap_or(0)
    }
    fn max_line_width(&self) -> usize {
        self.lines
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0)
    }

    fn clamp_col(&mut self) {
        let max = self.current_line_len();
        if self.cursor_col > max {
            self.cursor_col = max;
        }
    }

    fn ensure_cursor_visible(&mut self) {
        let h = self.inner_h as usize;
        let w = self.inner_w as usize;
        let content_w = w.saturating_sub(GUTTER as usize);

        if self.cursor_row < self.scroll_row {
            self.scroll_row = self.cursor_row;
        } else if self.cursor_row >= self.scroll_row + h {
            self.scroll_row = self.cursor_row + 1 - h;
        }
        // Never scroll past the last page
        self.scroll_row = self.scroll_row.min(self.max_scroll_row());

        if self.cursor_col < self.scroll_col {
            self.scroll_col = self.cursor_col;
        } else if self.cursor_col >= self.scroll_col + content_w {
            self.scroll_col = self.cursor_col + 1 - content_w;
        }
    }

    fn push_undo(&mut self) {
        if self
            .undo_stack
            .last()
            .map(|e: &UndoEntry| e.lines == self.lines)
            .unwrap_or(false)
        {
            return;
        }
        self.undo_stack.push(UndoEntry {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
        });
        if self.undo_stack.len() > 200 {
            self.undo_stack.remove(0);
        }
    }

    fn undo(&mut self) {
        if let Some(entry) = self.undo_stack.pop() {
            self.lines = entry.lines;
            self.cursor_row = entry.cursor_row;
            self.cursor_col = entry.cursor_col;
            self.modified = true;
            self.rehighlight();
        }
    }

    fn rehighlight(&mut self) {
        if let Some(ref mut lang) = self.lang {
            let source = self.lines.join("\n");
            let theme = self.theme.clone();
            self.highlights = compute_highlights(&source, lang, &theme);
        }
    }

    fn save(&mut self) -> Result<(), io::Error> {
        let content = self.lines.join("\n");
        std::fs::write(&self.file_path, content.as_bytes())?;
        self.modified = false;
        Ok(())
    }

    // --- Navigation ---

    fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.clamp_col();
        }
    }

    fn move_down(&mut self) {
        if self.cursor_row + 1 < self.line_count() {
            self.cursor_row += 1;
            self.clamp_col();
        }
    }

    fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.current_line_len();
        }
    }

    fn move_right(&mut self) {
        let len = self.current_line_len();
        if self.cursor_col < len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.line_count() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    fn move_word_left(&mut self) {
        if self.cursor_col == 0 {
            if self.cursor_row > 0 {
                self.cursor_row -= 1;
                self.cursor_col = self.current_line_len();
            }
            return;
        }
        let line = &self.lines[self.cursor_row];
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor_col;
        // Skip whitespace backwards
        while col > 0 && chars[col - 1].is_whitespace() {
            col -= 1;
        }
        // Skip word chars backwards
        while col > 0 && !chars[col - 1].is_whitespace() {
            col -= 1;
        }
        self.cursor_col = col;
    }

    fn move_word_right(&mut self) {
        let len = self.current_line_len();
        if self.cursor_col == len {
            if self.cursor_row + 1 < self.line_count() {
                self.cursor_row += 1;
                self.cursor_col = 0;
            }
            return;
        }
        let line = &self.lines[self.cursor_row];
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor_col;
        while col < chars.len() && !chars[col].is_whitespace() {
            col += 1;
        }
        while col < chars.len() && chars[col].is_whitespace() {
            col += 1;
        }
        self.cursor_col = col;
    }

    fn max_scroll_row(&self) -> usize {
        self.line_count().saturating_sub(self.inner_h as usize)
    }

    fn page_up(&mut self) {
        let jump = (self.inner_h as usize).saturating_sub(1).max(1);
        self.cursor_row = self.cursor_row.saturating_sub(jump);
        self.scroll_row = self.scroll_row.saturating_sub(jump);
        self.clamp_col();
    }

    fn page_down(&mut self) {
        let jump = (self.inner_h as usize).saturating_sub(1).max(1);
        let max_cursor = self.line_count().saturating_sub(1);
        let max_scroll = self.max_scroll_row();
        self.cursor_row = (self.cursor_row + jump).min(max_cursor);
        self.scroll_row = (self.scroll_row + jump).min(max_scroll);
        self.clamp_col();
    }

    // --- Editing ---

    /// Convert cursor_col (char index) to byte index in the line.
    fn col_to_byte(&self, row: usize, col: usize) -> usize {
        self.lines
            .get(row)
            .map(|l| l.char_indices().nth(col).map(|(i, _)| i).unwrap_or(l.len()))
            .unwrap_or(0)
    }

    fn insert_char(&mut self, ch: char) {
        self.push_undo();
        let byte = self.col_to_byte(self.cursor_row, self.cursor_col);
        self.lines[self.cursor_row].insert(byte, ch);
        self.cursor_col += 1;
        self.modified = true;
        self.rehighlight();
    }

    fn insert_newline(&mut self) {
        self.push_undo();
        let byte = self.col_to_byte(self.cursor_row, self.cursor_col);
        let rest = self.lines[self.cursor_row].split_off(byte);
        // Auto-indent: copy leading whitespace from current line
        let indent: String = self.lines[self.cursor_row]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let new_line = format!("{}{}", indent, rest);
        self.lines.insert(self.cursor_row + 1, new_line);
        self.cursor_row += 1;
        self.cursor_col = indent.chars().count();
        self.modified = true;
        self.rehighlight();
    }

    fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.push_undo();
            let byte_end = self.col_to_byte(self.cursor_row, self.cursor_col);
            let byte_start = self.col_to_byte(self.cursor_row, self.cursor_col - 1);
            self.lines[self.cursor_row].drain(byte_start..byte_end);
            self.cursor_col -= 1;
            self.modified = true;
            self.rehighlight();
        } else if self.cursor_row > 0 {
            self.push_undo();
            let line = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.current_line_len();
            self.lines[self.cursor_row].push_str(&line);
            self.modified = true;
            self.rehighlight();
        }
    }

    fn delete_char(&mut self) {
        let len = self.current_line_len();
        if self.cursor_col < len {
            self.push_undo();
            let byte_start = self.col_to_byte(self.cursor_row, self.cursor_col);
            let byte_end = self.col_to_byte(self.cursor_row, self.cursor_col + 1);
            self.lines[self.cursor_row].drain(byte_start..byte_end);
            self.modified = true;
            self.rehighlight();
        } else if self.cursor_row + 1 < self.line_count() {
            self.push_undo();
            let next = self.lines.remove(self.cursor_row + 1);
            self.lines[self.cursor_row].push_str(&next);
            self.modified = true;
            self.rehighlight();
        }
    }

    fn delete_word_back(&mut self) {
        if self.cursor_col == 0 {
            self.backspace();
            return;
        }
        self.push_undo();
        let line = &self.lines[self.cursor_row];
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor_col;
        while col > 0 && chars[col - 1].is_whitespace() {
            col -= 1;
        }
        while col > 0 && !chars[col - 1].is_whitespace() {
            col -= 1;
        }
        let byte_start = self.col_to_byte(self.cursor_row, col);
        let byte_end = self.col_to_byte(self.cursor_row, self.cursor_col);
        self.lines[self.cursor_row].drain(byte_start..byte_end);
        self.cursor_col = col;
        self.modified = true;
        self.rehighlight();
    }

    fn delete_word_forward(&mut self) {
        let len = self.current_line_len();
        if self.cursor_col >= len {
            self.delete_char();
            return;
        }
        self.push_undo();
        let chars: Vec<char> = self.lines[self.cursor_row].chars().collect();
        let mut col = self.cursor_col;
        while col < chars.len() && chars[col].is_whitespace() {
            col += 1;
        }
        while col < chars.len() && !chars[col].is_whitespace() {
            col += 1;
        }
        let byte_start = self.col_to_byte(self.cursor_row, self.cursor_col);
        let byte_end = self.col_to_byte(self.cursor_row, col);
        self.lines[self.cursor_row].drain(byte_start..byte_end);
        self.modified = true;
        self.rehighlight();
    }

    fn kill_to_eol(&mut self) {
        let len = self.current_line_len();
        if self.cursor_col >= len {
            // At end of line — join with next
            if self.cursor_row + 1 < self.lines.len() {
                self.push_undo();
                let next = self.lines.remove(self.cursor_row + 1);
                self.lines[self.cursor_row].push_str(&next);
                self.modified = true;
                self.rehighlight();
            }
            return;
        }
        self.push_undo();
        let byte_end = self.col_to_byte(self.cursor_row, len);
        let byte_start = self.col_to_byte(self.cursor_row, self.cursor_col);
        self.lines[self.cursor_row].drain(byte_start..byte_end);
        self.modified = true;
        self.rehighlight();
    }

    fn duplicate_line(&mut self) {
        self.push_undo();
        let line = self.lines[self.cursor_row].clone();
        self.lines.insert(self.cursor_row + 1, line);
        self.cursor_row += 1;
        self.modified = true;
        self.rehighlight();
    }

    // --- Selection ---

    /// Returns normalised (start_row, start_col, end_row, end_col) if a selection exists.
    fn selection_bounds(&self) -> Option<(usize, usize, usize, usize)> {
        self.selection.bounds()
    }

    /// Returns the (char_start, char_end) within the given row that is selected.
    /// Returns None if there is no selection or this row is not covered.
    fn selection_char_range(&self, row: usize) -> Option<(usize, usize)> {
        let (sr, sc, er, ec) = self.selection_bounds()?;
        let line_len = self.line_char_len(row);
        if row < sr || row > er {
            return None;
        }
        let start = if row == sr { sc } else { 0 };
        let end = if row == er { ec } else { line_len };
        if start == end {
            return None;
        }
        Some((start, end))
    }

    fn clear_selection(&mut self) {
        self.selection.clear();
    }

    // --- File watching ---

    fn check_external_modification(&mut self) {
        let current_mtime = std::fs::metadata(&self.file_path)
            .ok()
            .and_then(|m| m.modified().ok());
        if current_mtime != self.file_mtime {
            if !self.modified {
                // Reload
                let content = std::fs::read_to_string(&self.file_path).unwrap_or_default();
                let new_lines: Vec<String> = if content.is_empty() {
                    vec![String::new()]
                } else {
                    let mut v: Vec<String> = content.lines().map(|l| l.replace('\r', "")).collect();
                    if content.ends_with('\n') {
                        v.push(String::new());
                    }
                    if v.is_empty() {
                        v.push(String::new());
                    }
                    v
                };
                self.lines = new_lines;
                self.cursor_row = self.cursor_row.min(self.lines.len().saturating_sub(1));
                self.cursor_col = self.cursor_col.min(self.line_char_len(self.cursor_row));
                self.rehighlight();
                self.file_mtime = current_mtime;
            } else {
                self.external_modified = true;
                self.file_mtime = current_mtime;
            }
        }
    }

    // --- Rendering ---

    fn draw(&mut self, frame: &mut Frame) {
        let size = frame.area();
        // Layout rows (bottom-up): status bar + separator/h-scrollbar row
        // When a binary warning is shown, add 1 extra row above the status bar.
        let binary_warn_row: u16 = if self.is_binary && !self.binary_warning_dismissed {
            1
        } else {
            0
        };
        let bottom_rows: u16 = BOTTOM_ROWS + binary_warn_row;
        // Rightmost column reserved for vertical scrollbar (suppressed when embedded)
        let v_scroll_w: u16 = if self.embedded { 0 } else { 1 };

        let content_h = size.height.saturating_sub(bottom_rows);
        let content_w = size.width.saturating_sub(v_scroll_w);

        self.inner_h = content_h;
        self.inner_w = content_w;

        let content_area = Rect::new(size.x, size.y, content_w, content_h);
        let vscroll_area = Rect::new(size.x + content_w, size.y, v_scroll_w, content_h);
        // Binary warning bar sits just above the separator (when shown)
        let warn_area = Rect::new(size.x, size.y + content_h, size.width, binary_warn_row);
        let sep_area = Rect::new(size.x, size.y + content_h + binary_warn_row, size.width, 1);
        let status_area = Rect::new(
            size.x,
            size.y + content_h + binary_warn_row + 1,
            size.width,
            1,
        );

        // Draw binary warning banner
        if binary_warn_row > 0 {
            let warn_text =
                if self.is_image && self.preview_png.is_some() && supports_kitty_graphics() {
                    " Binary file (image). Enter to preview, Esc to close "
                } else if self.is_image {
                    " Binary file (image, no preview available). Enter to view, Esc to close "
                } else {
                    " Binary file. Enter to view, Esc to close "
                };
            frame.render_widget(
                Paragraph::new(warn_text).style(
                    Style::default()
                        .bg(Color::Rgb(180, 100, 0))
                        .fg(Color::White),
                ),
                warn_area,
            );
        }

        let text_w = content_w.saturating_sub(GUTTER);
        let visible_start = self.scroll_row;
        let visible_end = (visible_start + content_h as usize).min(self.line_count());

        let mut gutter_lines: Vec<Line> = Vec::new();
        let mut content_lines: Vec<Line> = Vec::new();

        for row in visible_start..visible_end {
            gutter_lines.push(Line::from(Span::styled(
                format!("{:>4} ", row + 1),
                Style::default().fg(if row == self.cursor_row {
                    Color::Rgb(150, 150, 150)
                } else {
                    Color::Rgb(70, 70, 80)
                }),
            )));

            let line = &self.lines[row];
            let line_chars: Vec<char> = line.chars().collect();
            let visible_chars: Vec<char> = line_chars
                .iter()
                .skip(self.scroll_col)
                .take(text_w as usize)
                .cloned()
                .collect();

            let sel_range = self.selection_char_range(row);
            let spans = if self.highlights.is_empty() || row >= self.highlights.len() {
                build_line_spans(&visible_chars, &[], self.scroll_col, sel_range)
            } else {
                build_line_spans(
                    &visible_chars,
                    &self.highlights[row],
                    self.scroll_col,
                    sel_range,
                )
            };

            let line_style = if row == self.cursor_row {
                Style::default().bg(Color::Indexed(236))
            } else {
                Style::default()
            };
            content_lines.push(Line::from(spans).style(line_style));
        }

        // Render gutter
        let gutter_area = Rect::new(content_area.x, content_area.y, GUTTER, content_h);
        frame.render_widget(Paragraph::new(gutter_lines), gutter_area);

        // Render content
        let text_area = Rect::new(content_area.x + GUTTER, content_area.y, text_w, content_h);
        frame.render_widget(Paragraph::new(content_lines), text_area);

        // Image preview placeholder (fills content area when image preview is active)
        if self.is_binary && self.binary_warning_dismissed && self.show_image_preview {
            // Fill the content area with spaces so ratatui owns the cells
            let blank_lines: Vec<Line> = (0..content_h)
                .map(|_| Line::from(" ".repeat(content_w as usize)))
                .collect();
            frame.render_widget(Paragraph::new(blank_lines), content_area);
            // Show a dim centered label
            let label = " [Image Preview \u{2014} ^B to view binary data] ";
            let lx = content_area.x
                + (content_w as usize).saturating_sub(label.chars().count()) as u16 / 2;
            let ly = content_area.y + content_h / 2;
            if content_h > 0 && content_w > 0 {
                frame.render_widget(
                    Paragraph::new(label).style(Style::default().fg(Color::Rgb(80, 80, 100))),
                    Rect::new(
                        lx,
                        ly,
                        label.chars().count().min(content_w as usize) as u16,
                        1,
                    ),
                );
            }
            self.last_image_area = content_area;
        }

        // Cursor — only visible when within the current viewport (not in image preview mode)
        if !(self.is_binary && self.binary_warning_dismissed && self.show_image_preview) {
            let crow_s = self.cursor_row as isize - self.scroll_row as isize;
            let ccol_s = self.cursor_col as isize - self.scroll_col as isize;
            if crow_s >= 0 && crow_s < content_h as isize && ccol_s >= 0 && ccol_s < text_w as isize
            {
                let cx = text_area.x + ccol_s as u16;
                let cy = text_area.y + crow_s as u16;
                let buf = frame.buffer_mut();
                if let Some(cell) = buf.cell_mut(Position { x: cx, y: cy }) {
                    if cell.modifier.contains(Modifier::REVERSED) {
                        cell.modifier.remove(Modifier::REVERSED);
                    } else {
                        cell.modifier.insert(Modifier::REVERSED);
                    }
                }
            }
        }

        // Vertical scrollbar (only when not embedded — aide renders its own)
        if !self.embedded {
            let v_scrollable = self.line_count().saturating_sub(content_h as usize);
            if v_scrollable > 0 && vscroll_area.width > 0 && vscroll_area.height > 0 {
                let track_h = vscroll_area.height as usize;
                let thumb_size = ((track_h as f64 * track_h as f64)
                    / (v_scrollable + track_h) as f64)
                    .ceil()
                    .max(1.0)
                    .min(track_h as f64) as usize;
                let scrollable = track_h.saturating_sub(thumb_size);
                let thumb_pos = if v_scrollable > 0 {
                    ((self.scroll_row as f64 / v_scrollable as f64) * scrollable as f64).round()
                        as usize
                } else {
                    0
                };
                let total_lines = self.line_count();
                let cursor_track_pos =
                    ((self.cursor_row as f64 / total_lines as f64) * track_h as f64) as usize;
                let sel_track_v = self.selection.bounds().map(|(sr, _, er, _)| {
                    let s = ((sr as f64 / total_lines as f64) * track_h as f64) as usize;
                    let e = ((er as f64 / total_lines as f64) * track_h as f64) as usize;
                    (s, e)
                });
                let buf = frame.buffer_mut();
                let bar_x = vscroll_area.x;
                for i in 0..track_h {
                    let y = vscroll_area.y + i as u16;
                    if y >= vscroll_area.y + vscroll_area.height {
                        break;
                    }
                    let is_thumb = i >= thumb_pos && i < thumb_pos + thumb_size;
                    let highlighted = i == cursor_track_pos
                        || sel_track_v.map(|(s, e)| i >= s && i <= e).unwrap_or(false);
                    let (ch, fg) = if highlighted {
                        ("│", Color::Rgb(220, 180, 60))
                    } else if is_thumb {
                        ("┃", Color::Rgb(100, 100, 180))
                    } else {
                        ("│", Color::Rgb(45, 45, 65))
                    };
                    if let Some(cell) = buf.cell_mut((bar_x, y)) {
                        cell.set_symbol(ch);
                        cell.set_style(Style::default().fg(fg));
                    }
                }
            }
        }

        // Separator line (always drawn) + horizontal scrollbar overlay (standalone only)
        let max_w = self.max_line_width();
        let h_scrollable = max_w.saturating_sub(text_w as usize);
        let sep_fg = Color::Rgb(55, 55, 75);
        // Draw the separator as a line of ━ chars (heavy horizontal)
        let sep_line = "\u{2501}".repeat(size.width as usize);
        frame.render_widget(
            Paragraph::new(sep_line).style(Style::default().fg(sep_fg)),
            sep_area,
        );
        if !self.embedded && h_scrollable > 0 && sep_area.width > 0 {
            let track_w = sep_area.width as usize;
            let thumb_size = ((track_w as f64 * track_w as f64) / (h_scrollable + track_w) as f64)
                .ceil()
                .max(1.0)
                .min(track_w as f64) as usize;
            let scrollable = track_w.saturating_sub(thumb_size);
            let thumb_pos = if h_scrollable > 0 {
                ((self.scroll_col as f64 / h_scrollable as f64) * scrollable as f64).round()
                    as usize
            } else {
                0
            };
            let total_cols = h_scrollable + track_w;
            let cursor_track_col =
                ((self.cursor_col as f64 / total_cols as f64) * track_w as f64) as usize;
            let sel_track_h = self.selection.bounds().map(|(_, sc, _, ec)| {
                let s = ((sc as f64 / total_cols as f64) * track_w as f64) as usize;
                let e = ((ec as f64 / total_cols as f64) * track_w as f64) as usize;
                (s, e)
            });
            let buf = frame.buffer_mut();
            let bar_y = sep_area.y;
            for i in 0..track_w {
                let x = sep_area.x + i as u16;
                if x >= sep_area.x + sep_area.width {
                    break;
                }
                let is_thumb = i >= thumb_pos && i < thumb_pos + thumb_size;
                let highlighted = i == cursor_track_col
                    || sel_track_h.map(|(s, e)| i >= s && i <= e).unwrap_or(false);
                let (ch, fg) = if highlighted {
                    ("\u{2501}", Color::Rgb(220, 180, 60)) // ━ yellow for cursor/selection
                } else if is_thumb {
                    ("\u{2501}", Color::Rgb(120, 120, 200)) // ━ bright accent for thumb
                } else {
                    ("\u{2501}", sep_fg) // ━ dim for track
                };
                if let Some(cell) = buf.cell_mut((x, bar_y)) {
                    cell.set_symbol(ch);
                    cell.set_style(Style::default().fg(fg));
                }
            }
        }

        // Join corner: ╯ rounds off where the vertical scrollbar meets the separator
        if !self.embedded && v_scroll_w > 0 {
            let buf = frame.buffer_mut();
            let cx = vscroll_area.x;
            let cy = sep_area.y;
            if let Some(cell) = buf.cell_mut((cx, cy)) {
                cell.set_symbol("╯");
                cell.set_style(Style::default().fg(sep_fg));
            }
        }

        // Status bar — multi-span, transparent background
        let lang_name = self.lang.as_ref().map(|l| l.name()).unwrap_or("");
        let ln = self.cursor_row + 1;
        let col = self.cursor_col + 1;

        let dim_gray = Style::default().fg(Color::Rgb(100, 100, 120));
        let bright_w = Style::default().fg(Color::Rgb(220, 220, 230));
        let cyan_s = Style::default().fg(Color::Rgb(80, 200, 200));
        let yellow_s = Style::default().fg(Color::Rgb(220, 180, 50));
        let orange_s = Style::default().fg(Color::Rgb(230, 150, 30));
        let red_s = Style::default().fg(Color::Rgb(220, 80, 80));

        // Selection info for status bar
        let pos_str = if let Some((sr, sc, er, ec)) = self.selection_bounds() {
            let sel_lines = er.saturating_sub(sr) + 1;
            if sel_lines > 1 {
                // Multi-line selection: "Ln start–end  Col start:end (N lines)"
                format!(
                    "Ln {}–{}  Col {}:{} ({} lines)",
                    sr + 1,
                    er + 1,
                    sc + 1,
                    ec + 1,
                    sel_lines
                )
            } else {
                // Single-line selection: "Ln N  Col start:end (N sel)"
                let n_sel = ec.saturating_sub(sc);
                format!("Ln {}  Col {}:{} ({} sel)", sr + 1, sc + 1, ec + 1, n_sel)
            }
        } else {
            format!("Ln {}  Col {}", ln, col)
        };

        // Build left spans
        let mut left_spans: Vec<Span> = Vec::new();
        left_spans.push(Span::raw(" "));
        // BINARY label
        if self.is_binary {
            left_spans.push(Span::styled(
                " BINARY ",
                Style::default()
                    .bg(Color::Rgb(180, 100, 0))
                    .fg(Color::White),
            ));
            left_spans.push(Span::raw("  "));
        }
        if !self.embedded {
            let display_path = if let Some(home) = std::env::var_os("HOME") {
                let home_str = home.to_string_lossy();
                if self.file_path.starts_with(home_str.as_ref()) {
                    format!("~{}", &self.file_path[home_str.len()..])
                } else {
                    self.file_path.clone()
                }
            } else {
                self.file_path.clone()
            };
            left_spans.push(Span::styled(display_path, dim_gray));
            left_spans.push(Span::raw("  "));
        }
        if !lang_name.is_empty() {
            left_spans.push(Span::styled(lang_name.to_string(), bright_w));
            left_spans.push(Span::raw("  "));
        }
        left_spans.push(Span::styled(pos_str, cyan_s));
        if self.modified {
            left_spans.push(Span::raw(" "));
            left_spans.push(Span::styled("●", yellow_s));
        }
        if self.external_modified {
            left_spans.push(Span::styled("  ⚠ external changes", orange_s));
        }
        if self.override_confirm {
            left_spans.push(Span::styled("  ^S to override", red_s));
        }

        // Build right hint
        let binary_hint = if self.is_binary && self.binary_warning_dismissed {
            if self.show_image_preview {
                " ^B binary "
            } else if self.is_image && self.preview_png.is_some() && supports_kitty_graphics() {
                " ^B preview "
            } else {
                ""
            }
        } else {
            ""
        };
        let base_right: &str = if self.override_confirm {
            "^S override  Esc cancel "
        } else if self.quit_confirm {
            "^S save  ^Q force quit "
        } else {
            "^S save  ^Q quit "
        };
        let right_str = format!("{}{}", binary_hint, base_right);
        let right_span = Span::styled(right_str.clone(), dim_gray);

        // Compute left width
        let left_width: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();
        let right_width = right_str.chars().count();
        let total_width = size.width as usize;
        let pad = total_width.saturating_sub(left_width + right_width);

        let mut status_spans = left_spans;
        status_spans.push(Span::raw(" ".repeat(pad)));
        status_spans.push(right_span);

        frame.render_widget(Paragraph::new(Line::from(status_spans)), status_area);
    }
}

// ---------------------------------------------------------------------------
// Build line spans from highlights
// ---------------------------------------------------------------------------

fn build_line_spans<'a>(
    visible_chars: &[char],
    hl: &'a [(Range<usize>, Color)],
    scroll_col: usize,
    sel_range: Option<(usize, usize)>,
) -> Vec<Span<'a>> {
    let default_color = Color::Rgb(171, 178, 191);
    let n = visible_chars.len();

    if n == 0 {
        return vec![];
    }

    let mut fg_colors: Vec<Color> = vec![default_color; n];
    let mut bg_colors: Vec<Option<Color>> = vec![None; n];

    for (byte_range, color) in hl {
        let char_start = byte_range.start.saturating_sub(scroll_col);
        let char_end = byte_range.end.saturating_sub(scroll_col);
        for i in char_start..char_end.min(n) {
            fg_colors[i] = *color;
        }
    }

    // Apply selection background — selection range is in document char coords;
    // visible_chars[i] = line_chars[scroll_col + i]
    if let Some((sel_start, sel_end)) = sel_range {
        // Convert to visible-char indices
        let vs = sel_start.saturating_sub(scroll_col);
        let ve = sel_end.saturating_sub(scroll_col).min(n);
        for i in vs..ve {
            bg_colors[i] = Some(selection::SELECTION_BG);
        }
    }

    // Compress into spans — boundary when either fg or bg changes
    let mut spans: Vec<Span<'a>> = Vec::new();
    let mut start = 0;
    while start < n {
        let fg = fg_colors[start];
        let bg = bg_colors[start];
        let mut end = start + 1;
        while end < n && fg_colors[end] == fg && bg_colors[end] == bg {
            end += 1;
        }
        let text: String = visible_chars[start..end].iter().collect();
        let style = if let Some(bg_col) = bg {
            Style::default().fg(fg).bg(bg_col)
        } else {
            Style::default().fg(fg)
        };
        spans.push(Span::styled(text, style));
        start = end;
    }
    spans
}

// ---------------------------------------------------------------------------
// Event handling
// ---------------------------------------------------------------------------

enum Action {
    Continue,
    Quit,
}

fn mouse_to_doc_pos(editor: &Editor, col: usize, row: usize) -> (usize, usize) {
    const GUTTER: usize = 5;
    let target_row = (editor.scroll_row + row).min(editor.line_count().saturating_sub(1));
    let target_col = if col >= GUTTER {
        (editor.scroll_col + (col - GUTTER)).min(editor.line_char_len(target_row))
    } else {
        0
    };
    (target_row, target_col)
}

fn handle_mouse(editor: &mut Editor, me: MouseEvent) {
    match me.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let col = me.column as usize;
            let row = me.row as usize;
            let content_h = editor.inner_h as usize;
            if row < content_h {
                let (target_row, target_col) = mouse_to_doc_pos(editor, col, row);
                editor.cursor_row = target_row;
                editor.cursor_col = target_col;
                editor.selection.mouse_down(target_row, target_col);
                editor.quit_confirm = false;
                editor.ensure_cursor_visible();
            }
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            let col = me.column as usize;
            let row = me.row as usize;
            let content_h = editor.inner_h as usize;
            // Auto-scroll when dragging near or past the edges (only while selecting).
            // Speed scales with distance from the edge.
            if editor.selection.dragging {
                let zone = 3usize;
                if row < zone {
                    let speed = (zone - row).max(1);
                    editor.scroll_row = editor.scroll_row.saturating_sub(speed);
                } else if row >= content_h.saturating_sub(zone) {
                    let dist = row.saturating_sub(content_h.saturating_sub(zone + 1)) + 1;
                    let speed = dist.max(1);
                    let max_scroll = editor.line_count().saturating_sub(content_h);
                    editor.scroll_row = (editor.scroll_row + speed).min(max_scroll);
                }
            }
            let row_clamped = row.min(content_h.saturating_sub(1));
            let (target_row, target_col) = mouse_to_doc_pos(editor, col, row_clamped);
            editor.selection.mouse_drag(target_row, target_col);
        }
        MouseEventKind::Up(MouseButton::Left) => {
            let col = me.column as usize;
            let row = me.row as usize;
            let content_h = editor.inner_h as usize;
            let row_clamped = row.min(content_h.saturating_sub(1));
            let (target_row, target_col) = mouse_to_doc_pos(editor, col, row_clamped);
            editor.selection.mouse_up(target_row, target_col);
        }
        // Scroll moves the viewport only — cursor stays where it is
        MouseEventKind::ScrollUp => {
            editor.scroll_row = editor.scroll_row.saturating_sub(1);
        }
        MouseEventKind::ScrollDown => {
            let max_scroll = editor.line_count().saturating_sub(editor.inner_h as usize);
            editor.scroll_row = (editor.scroll_row + 1).min(max_scroll);
        }
        _ => {}
    }
}

fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    let mut chunks = data.chunks_exact(3);
    for chunk in chunks.by_ref() {
        let b = ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | (chunk[2] as u32);
        out.push(ALPHABET[((b >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((b >> 12) & 0x3f) as usize] as char);
        out.push(ALPHABET[((b >> 6) & 0x3f) as usize] as char);
        out.push(ALPHABET[(b & 0x3f) as usize] as char);
    }
    match chunks.remainder() {
        [a] => {
            let b = (*a as u32) << 16;
            out.push(ALPHABET[((b >> 18) & 0x3f) as usize] as char);
            out.push(ALPHABET[((b >> 12) & 0x3f) as usize] as char);
            out.push('=');
            out.push('=');
        }
        [a, b] => {
            let v = ((*a as u32) << 16) | ((*b as u32) << 8);
            out.push(ALPHABET[((v >> 18) & 0x3f) as usize] as char);
            out.push(ALPHABET[((v >> 12) & 0x3f) as usize] as char);
            out.push(ALPHABET[((v >> 6) & 0x3f) as usize] as char);
            out.push('=');
        }
        _ => {}
    }
    out
}

fn handle_key(editor: &mut Editor, key: KeyEvent) -> Action {
    // Handle binary warning prompt — intercept all keys until dismissed
    if editor.is_binary && !editor.binary_warning_dismissed {
        match key.code {
            KeyCode::Enter => {
                editor.binary_warning_dismissed = true;
                editor.image_needs_render = editor.show_image_preview;
                return Action::Continue;
            }
            KeyCode::Esc => {
                return Action::Quit;
            }
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return Action::Quit;
            }
            _ => return Action::Continue,
        }
    }

    // Reset quit confirm on any key other than Ctrl+Q/X
    let is_ctrl_q = key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL);
    let is_ctrl_x = key.code == KeyCode::Char('x') && key.modifiers.contains(KeyModifiers::CONTROL);
    let is_ctrl_c = matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        && key.modifiers.contains(KeyModifiers::CONTROL);
    let is_ctrl_a = key.code == KeyCode::Char('a') && key.modifiers.contains(KeyModifiers::CONTROL);
    let is_esc = key.code == KeyCode::Esc;
    if !is_ctrl_q && !is_ctrl_x {
        editor.quit_confirm = false;
    }

    // Clear selection on all keys except Ctrl+C and Ctrl+A (select all)
    if !is_ctrl_c && !is_ctrl_a {
        editor.clear_selection();
    }

    match (key.code, key.modifiers) {
        // Quit
        (KeyCode::Char('q'), m) | (KeyCode::Char('x'), m) if m.contains(KeyModifiers::CONTROL) => {
            if editor.modified && !editor.quit_confirm {
                editor.quit_confirm = true;
            } else {
                return Action::Quit;
            }
        }
        // Escape — cancel override confirm
        (KeyCode::Esc, _) => {
            let _ = is_esc; // suppress unused warning
            editor.override_confirm = false;
        }
        // Save (with external-modification guard) — no-op for binary files
        (KeyCode::Char('s'), m) if m.contains(KeyModifiers::CONTROL) => {
            if editor.is_binary {
                // Don't save binary files — would corrupt them
            } else if editor.external_modified && !editor.override_confirm {
                editor.override_confirm = true;
            } else {
                let _ = editor.save();
                editor.quit_confirm = false;
                editor.external_modified = false;
                editor.override_confirm = false;
            }
        }
        // Ctrl+B — toggle between image preview and binary text view
        (KeyCode::Char('b'), m) if m.contains(KeyModifiers::CONTROL) => {
            if editor.is_image && editor.preview_png.is_some() && supports_kitty_graphics() {
                editor.show_image_preview = !editor.show_image_preview;
                editor.image_needs_render = editor.show_image_preview;
            }
        }
        // Copy selection via OSC 52 (Ctrl+C or Ctrl+Shift+C)
        (KeyCode::Char('c') | KeyCode::Char('C'), m) if m.contains(KeyModifiers::CONTROL) => {
            if let Some((sr, sc, er, ec)) = editor.selection_bounds() {
                let text = selection::extract_from_lines(&editor.lines, sr, sc, er, ec);
                selection::copy_to_clipboard(&text);
            }
        }
        // Undo
        (KeyCode::Char('z'), m) if m.contains(KeyModifiers::CONTROL) => {
            editor.undo();
        }
        // Navigation
        (KeyCode::Up, _) => editor.move_up(),
        (KeyCode::Down, _) => editor.move_down(),
        (KeyCode::Left, m) if m.contains(KeyModifiers::CONTROL) => editor.move_word_left(),
        (KeyCode::Right, m) if m.contains(KeyModifiers::CONTROL) => editor.move_word_right(),
        (KeyCode::Left, _) => editor.move_left(),
        (KeyCode::Right, _) => editor.move_right(),
        (KeyCode::Home, m) if m.contains(KeyModifiers::CONTROL) => {
            editor.cursor_row = 0;
            editor.cursor_col = 0;
        }
        (KeyCode::End, m) if m.contains(KeyModifiers::CONTROL) => {
            editor.cursor_row = editor.line_count().saturating_sub(1);
            editor.cursor_col = editor.current_line_len();
        }
        (KeyCode::Home, _) => editor.cursor_col = 0,
        (KeyCode::End, _) => editor.cursor_col = editor.current_line_len(),
        (KeyCode::PageUp, _) => editor.page_up(),
        (KeyCode::PageDown, _) => editor.page_down(),
        // Editing
        (KeyCode::Enter, _) => editor.insert_newline(),
        // Backspace variants — handle both enhanced (Backspace+CTRL) and legacy (\x17 = Ctrl+W)
        (KeyCode::Backspace, m)
            if m.contains(KeyModifiers::CONTROL) || m.contains(KeyModifiers::ALT) =>
        {
            editor.delete_word_back()
        }
        (KeyCode::Backspace, _) => editor.backspace(),
        (KeyCode::Char('w'), m) if m.contains(KeyModifiers::CONTROL) => editor.delete_word_back(),
        (KeyCode::Char('h'), m) if m.contains(KeyModifiers::CONTROL) => editor.backspace(),
        // Delete forward variants
        (KeyCode::Delete, m) if m.contains(KeyModifiers::CONTROL) => editor.delete_word_forward(),
        (KeyCode::Delete, _) => editor.delete_char(),
        // Ctrl+K — kill to end of line
        (KeyCode::Char('k'), m) if m.contains(KeyModifiers::CONTROL) => editor.kill_to_eol(),
        // Ctrl+D — duplicate line
        (KeyCode::Char('d'), m) if m.contains(KeyModifiers::CONTROL) => editor.duplicate_line(),
        // Ctrl+A — select all
        (KeyCode::Char('a'), m) if m.contains(KeyModifiers::CONTROL) => {
            let last_row = editor.lines.len().saturating_sub(1);
            let last_col = editor.lines[last_row].chars().count();
            editor.selection.mouse_down(0, 0);
            editor.selection.mouse_drag(last_row, last_col);
        }
        (KeyCode::Char(c), m) if m == KeyModifiers::NONE || m == KeyModifiers::SHIFT => {
            editor.insert_char(c);
        }
        (KeyCode::Tab, _) => {
            // Insert spaces (respect tab size)
            for _ in 0..4 {
                editor.insert_char(' ');
            }
        }
        _ => {}
    }
    Action::Continue
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn parse_cursor_style(s: &str) -> crossterm::cursor::SetCursorStyle {
    use crossterm::cursor::SetCursorStyle::*;
    match s.trim().to_lowercase().as_str() {
        "block" | "steady_block" => SteadyBlock,
        "blinking_block" => BlinkingBlock,
        "underline" | "underscore" | "steady_underline" => SteadyUnderScore,
        "blinking_underline" | "blinking_underscore" => BlinkingUnderScore,
        "bar" | "line" | "steady_bar" | "steady_line" => SteadyBar,
        "blinking_bar" | "blinking_line" => BlinkingBar,
        _ => DefaultUserShape,
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let file_path = args.get(1).cloned().unwrap_or_else(|| {
        eprintln!("Usage: aide-editor <file>");
        std::process::exit(1);
    });

    let mut editor = Editor::open(&file_path);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
        crossterm::event::EnableFocusChange,
    )?;
    // Apply cursor shape from config (env var set by aide, or read directly in standalone).
    {
        let shape_str = std::env::var("AIDE_CURSOR_SHAPE").unwrap_or_else(|_| {
            // Standalone: try to read from aide config file directly.
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let cfg_path = std::path::Path::new(&home)
                .join(".config")
                .join("aide")
                .join("config.toml");
            std::fs::read_to_string(&cfg_path)
                .ok()
                .and_then(|s| {
                    s.lines()
                        .find(|l| l.starts_with("cursor_shape"))
                        .and_then(|l| l.splitn(2, '=').nth(1))
                        .map(|v| v.trim().trim_matches('"').to_string())
                })
                .unwrap_or_else(|| "default".to_string())
        });
        let style = parse_cursor_style(&shape_str);
        let _ = execute!(stdout, style);
    }
    // Enable keyboard enhancement in standalone mode so Ctrl+Backspace etc. are disambiguated.
    // Skip when embedded — the PTY layer handles forwarding.
    if !editor.embedded {
        let _ = execute!(
            stdout,
            PushKeyboardEnhancementFlags(KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES)
        );
    }
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    loop {
        editor.frame_count = editor.frame_count.wrapping_add(1);
        if editor.frame_count % 20 == 0 {
            editor.check_external_modification();
        }

        terminal.draw(|f| editor.draw(f))?;

        // Emit Kitty image if needed (after draw, so last_image_area is up to date)
        if editor.is_image
            && editor.binary_warning_dismissed
            && editor.show_image_preview
            && editor.image_needs_render
            && editor.last_image_area.width > 0
            && editor.last_image_area.height > 0
        {
            if let Some(ref png) = editor.preview_png {
                let area = editor.last_image_area;
                emit_kitty_image(png, area.x, area.y, area.width, area.height);
                editor.image_needs_render = false;
            }
        }

        // Window title
        {
            use std::io::Write as _;
            let title = if !editor.embedded {
                let p = if let Some(home) = std::env::var_os("HOME") {
                    let h = home.to_string_lossy().to_string();
                    if editor.file_path.starts_with(&h) {
                        format!("~{}", &editor.file_path[h.len()..])
                    } else {
                        editor.file_path.clone()
                    }
                } else {
                    editor.file_path.clone()
                };
                Some(p)
            } else {
                None
            };
            if let Some(t) = title {
                let _ = write!(io::stdout(), "\x1b]2;{}\x07", t);
                let _ = io::stdout().flush();
            }
        }

        // Report scroll + layout state to aide via OSC window title (embedded only).
        // "aide:{scroll_row}/{total_lines}/{view_h}/{scroll_col}/{max_col}/{gutter_w}/{content_h}"
        if editor.embedded {
            use std::io::Write as _;
            let mut out = io::stdout();
            let _ = write!(
                out,
                "\x1b]2;aide:{}/{}/{}/{}/{}/{}/{}/{}/{}\x07",
                editor.scroll_row,
                editor.line_count(),
                editor.inner_h,
                editor.scroll_col,
                editor.max_line_width(),
                GUTTER,
                editor.inner_h,
                editor.cursor_row,
                editor.cursor_col,
            );
            // Report selected text + bounds to aide via OSC 7734:
            // "\x1b]7734;<base64text>;<sr>/<sc>/<er>/<ec>\x07"
            // Empty payload means no selection.
            let payload = if let Some((sr, sc, er, ec)) = editor.selection.bounds() {
                let text = selection::extract_from_lines(&editor.lines, sr, sc, er, ec);
                format!(
                    "{};{}/{}/{}/{}",
                    selection::base64_encode(text.as_bytes()),
                    sr,
                    sc,
                    er,
                    ec
                )
            } else {
                String::new()
            };
            let _ = write!(out, "\x1b]7734;{}\x07", payload);
            let _ = out.flush();
        }

        // Drain all pending events so queued scroll/key events don't trickle
        // across frames and cause the "scroll a bit, then more" jitter.
        let mut should_quit = false;
        if event::poll(Duration::from_millis(16))? {
            loop {
                let ev = event::read()?;
                let mut needs_cursor_check = false;
                match ev {
                    Event::Key(k) if k.kind == crossterm::event::KeyEventKind::Press => {
                        if let Action::Quit = handle_key(&mut editor, k) {
                            should_quit = true;
                        }
                        needs_cursor_check = true;
                    }
                    Event::Mouse(me) => {
                        handle_mouse(&mut editor, me);
                    }
                    Event::Resize(_, _) => {
                        needs_cursor_check = true;
                        if editor.show_image_preview {
                            editor.image_needs_render = true;
                        }
                    }
                    Event::FocusLost => {
                        editor.selection.clear();
                    }
                    Event::Paste(text) => {
                        if let Some(theme_name) = text.strip_prefix("aide-theme:") {
                            editor.theme = theme_name.trim().to_string();
                            editor.rehighlight();
                        } else {
                            for c in text.chars() {
                                editor.insert_char(c);
                            }
                            needs_cursor_check = true;
                        }
                    }
                    _ => {}
                }
                if needs_cursor_check {
                    editor.ensure_cursor_visible();
                }
                if should_quit || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        if should_quit {
            break;
        }
    }

    // Cleanup
    if !editor.embedded {
        let _ = execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags);
    }
    let _ = execute!(
        terminal.backend_mut(),
        crossterm::cursor::SetCursorStyle::DefaultUserShape
    );
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste,
    )?;
    terminal.show_cursor()?;

    Ok(())
}
