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

/// Gather all git info in one go. Runs on background thread.
fn gather_snapshot(directory: &str, log_limit: usize) -> GitSnapshot {
    let mut snap = GitSnapshot::default();

    // status + branch (single command)
    if let Ok(output) = Command::new("git")
        .args(["status", "--short", "--branch"])
        .current_dir(directory)
        .output()
    {
        snap.status = String::from_utf8_lossy(&output.stdout).to_string();
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
            snap.remote_branch = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_string();
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
                        let entry = file_stats.entry(parts[2].to_string()).or_insert((0, 0));
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
