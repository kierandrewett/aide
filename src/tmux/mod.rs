use anyhow::{Context, Result};
use std::process::Command;

/// Create a new detached tmux session with the given name and working directory.
pub fn create_session(name: &str, directory: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["new-session", "-d", "-s", name, "-c", directory])
        .status()
        .context("Failed to create tmux session")?;
    if !status.success() {
        anyhow::bail!("tmux new-session failed for '{}'", name);
    }
    Ok(())
}

/// Send a command to a tmux session.
pub fn run_command(session_name: &str, command: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["send-keys", "-t", session_name, command, "C-m"])
        .status()
        .context("Failed to send keys to tmux session")?;
    if !status.success() {
        anyhow::bail!("tmux send-keys failed for '{}'", session_name);
    }
    Ok(())
}

/// Capture the current pane output from a tmux session.
pub fn capture_pane(session_name: &str) -> Result<String> {
    let output = Command::new("tmux")
        .args(["capture-pane", "-pt", session_name, "-S", "-200"])
        .output()
        .context("Failed to capture tmux pane")?;
    if !output.status.success() {
        anyhow::bail!("tmux capture-pane failed for '{}'", session_name);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// List all existing tmux sessions, returning their names.
pub fn list_sessions() -> Result<Vec<String>> {
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let names = String::from_utf8_lossy(&out.stdout)
                .lines()
                .map(|s| s.to_string())
                .collect();
            Ok(names)
        }
        _ => Ok(Vec::new()),
    }
}

/// Kill a tmux session by name.
pub fn kill_session(session_name: &str) -> Result<()> {
    let status = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .status()
        .context("Failed to kill tmux session")?;
    if !status.success() {
        anyhow::bail!("tmux kill-session failed for '{}'", session_name);
    }
    Ok(())
}
