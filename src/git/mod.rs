use anyhow::{Context, Result};
use std::process::Command;

/// Get short git status output for a directory.
pub fn status_short(directory: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["status", "--short", "--branch"])
        .current_dir(directory)
        .output()
        .context("Failed to run git status")?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Get git log (oneline graph, last 20 commits) for a directory.
pub fn log_oneline(directory: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["log", "--oneline", "--graph", "-20"])
        .current_dir(directory)
        .output()
        .context("Failed to run git log")?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Get the current branch name.
pub fn current_branch(directory: &str) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(directory)
        .output()
        .context("Failed to get current branch")?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get working tree diff stats (additions, deletions).
pub fn diff_stats(directory: &str) -> Option<(usize, usize)> {
    let output = Command::new("git")
        .args(["diff", "--numstat"])
        .current_dir(directory)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut added = 0usize;
    let mut deleted = 0usize;
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            added += parts[0].parse::<usize>().unwrap_or(0);
            deleted += parts[1].parse::<usize>().unwrap_or(0);
        }
    }
    // Also include staged changes
    let staged = Command::new("git")
        .args(["diff", "--numstat", "--cached"])
        .current_dir(directory)
        .output()
        .ok()?;
    let staged_text = String::from_utf8_lossy(&staged.stdout);
    for line in staged_text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 {
            added += parts[0].parse::<usize>().unwrap_or(0);
            deleted += parts[1].parse::<usize>().unwrap_or(0);
        }
    }

    Some((added, deleted))
}

/// Get push/pull counts relative to upstream.
/// Returns (behind, ahead) or None if no upstream.
pub fn upstream_counts(directory: &str) -> Option<(usize, usize)> {
    let output = Command::new("git")
        .args(["rev-list", "--left-right", "--count", "@{upstream}...HEAD"])
        .current_dir(directory)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = text.trim().split('\t').collect();
    if parts.len() == 2 {
        let behind = parts[0].parse().unwrap_or(0);
        let ahead = parts[1].parse().unwrap_or(0);
        Some((behind, ahead))
    } else {
        None
    }
}
