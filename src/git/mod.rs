use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};

/// All git state gathered in one background refresh.
#[derive(Clone, Default)]
pub struct GitSnapshot {
    pub status: String,
    pub log: String,
    pub branch: String,
    pub remote_branch: String,
    pub upstream: Option<(usize, usize)>,
    pub diff_stats: Option<(usize, usize)>,
    pub file_stats: HashMap<String, (usize, usize)>,
    pub log_has_more: bool,
}

/// Handle to a background git worker. Call `request_refresh()` to trigger,
/// poll `take_snapshot()` each frame to pick up results.
pub struct GitWorker {
    shared: Arc<Mutex<WorkerState>>,
}

struct WorkerState {
    /// Latest completed snapshot (taken by main thread).
    snapshot: Option<GitSnapshot>,
    /// Pending request: (directory, log_limit).
    pending: Option<(String, usize)>,
}

impl GitWorker {
    pub fn new() -> Self {
        let shared = Arc::new(Mutex::new(WorkerState {
            snapshot: None,
            pending: None,
        }));

        let worker_shared = shared.clone();
        std::thread::spawn(move || worker_loop(worker_shared));

        Self { shared }
    }

    /// Request a refresh. Coalesces — only the latest request is kept.
    pub fn request_refresh(&self, directory: &str, log_limit: usize) {
        if let Ok(mut state) = self.shared.lock() {
            state.pending = Some((directory.to_string(), log_limit));
        }
    }

    /// Run `git fetch` in the background, then queue a snapshot refresh.
    /// Used when the branch changes so upstream counts reflect the new remote state.
    pub fn fetch_and_refresh(&self, directory: &str, log_limit: usize) {
        let shared = self.shared.clone();
        let dir = directory.to_string();
        std::thread::spawn(move || {
            let _ = Command::new("git")
                .args(["fetch"])
                .current_dir(&dir)
                .output();
            if let Ok(mut state) = shared.lock() {
                state.pending = Some((dir, log_limit));
            }
        });
    }

    /// Take the latest completed snapshot (if any). Returns None if no
    /// new data since last call.
    pub fn take_snapshot(&self) -> Option<GitSnapshot> {
        if let Ok(mut state) = self.shared.lock() {
            state.snapshot.take()
        } else {
            None
        }
    }
}

fn worker_loop(shared: Arc<Mutex<WorkerState>>) {
    loop {
        // Check for pending work
        let work = {
            let mut state = match shared.lock() {
                Ok(s) => s,
                Err(_) => break,
            };
            state.pending.take()
        };

        if let Some((dir, log_limit)) = work {
            let snap = gather_snapshot(&dir, log_limit);
            if let Ok(mut state) = shared.lock() {
                state.snapshot = Some(snap);
            }
        } else {
            // No work — sleep briefly to avoid busy-waiting
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

/// Walk an untracked directory recursively and collect repo-relative file paths.
fn walk_untracked_dir(abs_dir: &std::path::Path, repo_prefix: &str, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(abs_dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut children: Vec<std::path::PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    children.sort();
    for child in children {
        let name = match child.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let rel = format!("{}{}", repo_prefix, name);
        if child.is_dir() {
            walk_untracked_dir(&child, &format!("{}/", rel), out);
        } else {
            out.push(rel);
        }
    }
}

/// Post-process `git status --short --branch` output: expand lines like
/// `?? dirname/` into one `?? dirname/file` line per file in that directory.
fn expand_untracked_dirs(status: &str, repo_root: &str) -> String {
    let root = std::path::Path::new(repo_root);
    let mut result = String::new();
    for line in status.lines() {
        // Branch header lines start with '#' — pass through unchanged.
        if line.starts_with('#') || line.len() < 4 {
            result.push_str(line);
            result.push('\n');
            continue;
        }
        let xy = &line[..2];
        let path = &line[3..];
        // Only expand untracked directories (ends with '/')
        if xy == "??" && path.ends_with('/') {
            let abs_dir = root.join(path);
            let mut files = Vec::new();
            walk_untracked_dir(&abs_dir, path, &mut files);
            if files.is_empty() {
                // Empty dir — keep the original line
                result.push_str(line);
                result.push('\n');
            } else {
                for f in files {
                    result.push_str("?? ");
                    result.push_str(&f);
                    result.push('\n');
                }
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }
    result
}

/// Sort status file lines alphabetically by path, keeping branch header lines first.
fn sort_status_lines(status: &str) -> String {
    let mut headers = Vec::new();
    let mut files = Vec::new();
    for line in status.lines() {
        if line.starts_with('#') || line.is_empty() {
            headers.push(line);
        } else {
            files.push(line);
        }
    }
    files.sort_by(|a, b| {
        let pa = if a.len() >= 3 { &a[3..] } else { "" };
        let pb = if b.len() >= 3 { &b[3..] } else { "" };
        pa.cmp(pb)
    });
    let mut out = String::new();
    for l in headers { out.push_str(l); out.push('\n'); }
    for l in files { out.push_str(l); out.push('\n'); }
    out
}

/// Gather all git info in one go. Runs on background thread.
fn gather_snapshot(directory: &str, log_limit: usize) -> GitSnapshot {
    let mut snap = GitSnapshot::default();

    // status + branch (single command)
    if let Ok(output) = Command::new("git")
        .args(["status", "--short", "--branch"])
        .current_dir(directory)
        .output()
    {
        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        let expanded = expand_untracked_dirs(&raw, directory);
        snap.status = sort_status_lines(&expanded);
    }

    // branch name
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(directory)
        .output()
    {
        snap.branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    }

    // remote tracking branch
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"])
        .current_dir(directory)
        .output()
    {
        if output.status.success() {
            snap.remote_branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
    }

    // upstream counts
    if let Ok(output) = Command::new("git")
        .args(["rev-list", "--left-right", "--count", "@{upstream}...HEAD"])
        .current_dir(directory)
        .output()
    {
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout);
            let parts: Vec<&str> = text.trim().split('\t').collect();
            if parts.len() == 2 {
                let behind = parts[0].parse().unwrap_or(0);
                let ahead = parts[1].parse().unwrap_or(0);
                snap.upstream = Some((behind, ahead));
            }
        }
    }

    // Combine diff stats: working tree + staged in one pass each
    let mut total_added = 0usize;
    let mut total_deleted = 0usize;
    let mut file_stats: HashMap<String, (usize, usize)> = HashMap::new();

    for extra_args in [&["--numstat"][..], &["--numstat", "--cached"][..]] {
        let mut args = vec!["diff"];
        args.extend_from_slice(extra_args);
        if let Ok(output) = Command::new("git")
            .args(&args)
            .current_dir(directory)
            .output()
        {
            if output.status.success() {
                for line in String::from_utf8_lossy(&output.stdout).lines() {
                    let parts: Vec<&str> = line.split('\t').collect();
                    if parts.len() >= 3 {
                        let added = parts[0].parse::<usize>().unwrap_or(0);
                        let deleted = parts[1].parse::<usize>().unwrap_or(0);
                        total_added += added;
                        total_deleted += deleted;
                        // Normalize rename notation: numstat uses "old => new" but
                        // git status uses "old -> new", so we index by both forms.
                        let raw = parts[2];
                        let key = raw.replace(" => ", " -> ");
                        let entry = file_stats.entry(key).or_insert((0, 0));
                        entry.0 += added;
                        entry.1 += deleted;
                    }
                }
            }
        }
    }

    if total_added > 0 || total_deleted > 0 {
        snap.diff_stats = Some((total_added, total_deleted));
    }
    snap.file_stats = file_stats;

    // log
    if let Ok(output) = Command::new("git")
        .args([
            "log",
            "--oneline",
            "--graph",
            "--decorate=short",
            "--format=%h %d %s (%cr)",
            &format!("-{}", log_limit),
        ])
        .current_dir(directory)
        .output()
    {
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        snap.log_has_more = text.lines().count() >= log_limit;
        snap.log = text;
    }

    snap
}

/// A file changed in a commit.
#[derive(Clone, Debug)]
pub struct CommitFile {
    /// Single-char status: A, M, D, R, C, T, U…
    pub status: char,
    /// Repo-relative path of the file (new path for renames).
    pub path: String,
}

/// Fetch the list of files changed by `hash` in `directory`.
pub fn fetch_commit_files(directory: &str, hash: &str) -> Vec<CommitFile> {
    let output = match Command::new("git")
        .args([
            "diff-tree",
            "--no-commit-id",
            "-r",
            "--name-status",
            "--diff-filter=ACDMRT",
            hash,
        ])
        .current_dir(directory)
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, '\t').collect();
            if parts.len() < 2 {
                return None;
            }
            let status = parts[0].chars().next().unwrap_or('M');
            // Renames / copies: "R100\told.rs\tnew.rs" — use the new path
            let path = if parts.len() == 3 && matches!(status, 'R' | 'C') {
                parts[2].to_string()
            } else {
                parts[1].to_string()
            };
            Some(CommitFile { status, path })
        })
        .collect()
}

/// List local branch names.
pub fn list_branches(directory: &str) -> Vec<String> {
    let output = match Command::new("git")
        .args(["branch", "--format=%(refname:short)"])
        .current_dir(directory)
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}
