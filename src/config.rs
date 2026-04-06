use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub command: String,
    pub projects_dir: String,
    /// The editor command launched in the file viewer pane.
    /// Receives the file path as its last argument.
    /// Set to "nano", "vim", "hx", etc. to use a different editor.
    pub editor_command: String,
    /// Show Nerd Font icons in the file browser and status bar.
    /// Requires a Nerd Font to be configured in your terminal emulator.
    /// Works in Ghostty and Termius (desktop) when a Nerd Font is active.
    pub icons: bool,
    /// Syntax highlighting theme for aide-editor.
    /// Options: "github-dark", "one-dark", "dracula", "nord", "monokai", "solarized-dark"
    pub editor_theme: String,
    /// Cursor shape. Options: "block", "underline", "bar" (alias "line").
    /// Prefix with "blinking_" for a blinking variant, e.g. "blinking_bar".
    /// Use "default" to leave the terminal's configured cursor unchanged.
    pub cursor_shape: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            command: "$SHELL".to_string(),
            projects_dir: "$HOME/dev".to_string(),
            editor_command: "aide-editor".to_string(),
            icons: true,
            editor_theme: "github-dark".to_string(),
            cursor_shape: "default".to_string(),
        }
    }
}

/// Parse a cursor_shape string into a crossterm `SetCursorStyle`.
pub fn parse_cursor_style(s: &str) -> crossterm::cursor::SetCursorStyle {
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

/// Resolve environment variables in a string (e.g. "$SHELL" -> "/bin/zsh").
fn resolve_env_vars(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find('$') {
        let rest = &result[start + 1..];
        let end = rest
            .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .unwrap_or(rest.len());
        let var_name = &rest[..end];
        if var_name.is_empty() {
            break;
        }
        let value = std::env::var(var_name).unwrap_or_default();
        result = format!("{}{}{}", &result[..start], value, &rest[end..]);
    }
    result
}

impl Config {
    /// Load config from `~/.config/aide/config.toml`, creating a default if missing.
    pub fn load() -> Result<Self> {
        let path = config_path();

        if !path.exists() {
            let config = Config::default();
            config.save()?;
            return Ok(config.resolve());
        }

        let contents = std::fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&contents)?;
        Ok(config.resolve())
    }

    /// Resolve environment variables in all string fields.
    fn resolve(mut self) -> Self {
        self.command = resolve_env_vars(&self.command);
        self.projects_dir = resolve_env_vars(&self.projects_dir);
        self
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        std::fs::write(&path, contents)?;
        Ok(())
    }
}

fn config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("aide")
        .join("config.toml")
}
