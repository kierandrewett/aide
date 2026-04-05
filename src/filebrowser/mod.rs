use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// True when the entry is a symbolic link (is_dir already follows the target).
    pub is_symlink: bool,
    pub depth: usize,
    pub expanded: bool,
    /// Git status: None = clean, Some('A') = added, Some('M') = modified, Some('D') = deleted, Some('?') = untracked
    pub git_status: Option<char>,
    /// Whether this file/folder is matched by .gitignore
    pub is_ignored: bool,
}

pub struct FileBrowser {
    pub root: PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected: usize,
    pub scroll_offset: u16,
}

impl FileBrowser {
    pub fn new() -> Self {
        Self {
            root: PathBuf::new(),
            entries: Vec::new(),
            selected: 0,
            scroll_offset: 0,
        }
    }

    pub fn set_root(&mut self, path: &str) {
        let new_root = PathBuf::from(path);
        if new_root != self.root {
            self.root = new_root;
            self.refresh();
        }
    }

    pub fn refresh(&mut self) {
        self.entries.clear();
        self.selected = 0;
        self.scroll_offset = 0;
        if self.root.exists() {
            self.load_dir(&self.root.clone(), 0);
            self.update_ignored();
        }
    }

    /// Query git to find ignored files and mark them.
    fn update_ignored(&mut self) {
        let ignored = git_ignored_paths(&self.root);
        for entry in &mut self.entries {
            if let Ok(rel) = entry.path.strip_prefix(&self.root) {
                let rel_str = rel.to_string_lossy().to_string();
                // Check exact match or if any parent is ignored
                entry.is_ignored =
                    ignored.contains(&rel_str) || ignored.contains(&format!("{}/", rel_str));
            }
        }
    }

    /// Update git status indicators on all entries.
    pub fn update_git_status(&mut self, git_status_output: &str) {
        // Parse git status --short output to map filenames to statuses
        let mut status_map = std::collections::HashMap::new();
        for line in git_status_output.lines() {
            if line.starts_with("##") || line.trim().is_empty() {
                continue;
            }
            if line.len() < 4 {
                continue;
            }
            let (idx, wt) = (
                line.chars().next().unwrap_or(' '),
                line.chars().nth(1).unwrap_or(' '),
            );
            let filename = &line[3..];

            let status = match (idx, wt) {
                ('?', '?') => '?',
                ('A', _) | (_, 'A') => 'A',
                ('D', _) | (_, 'D') => 'D',
                ('M', _) | (_, 'M') => 'M',
                ('R', _) => 'M', // treat rename as modified
                _ => 'M',
            };
            // Handle renames: "old -> new"
            let fname = if let Some(arrow) = filename.find(" -> ") {
                &filename[arrow + 4..]
            } else {
                filename
            };
            status_map.insert(fname.to_string(), status);

            // Also mark parent directories
            let p = Path::new(fname);
            let mut parent = p.parent();
            while let Some(pp) = parent {
                if pp.as_os_str().is_empty() {
                    break;
                }
                let ps = pp.to_string_lossy().to_string();
                status_map.entry(ps).or_insert(status);
                parent = pp.parent();
            }
        }

        for entry in &mut self.entries {
            // Get relative path from root
            if let Ok(rel) = entry.path.strip_prefix(&self.root) {
                let rel_str = rel.to_string_lossy().to_string();
                entry.git_status = status_map.get(&rel_str).copied();
            }
        }
    }

    fn load_dir(&mut self, dir: &Path, depth: usize) {
        let mut entries: Vec<(String, PathBuf, bool, bool)> = Vec::new();

        if let Ok(read_dir) = std::fs::read_dir(dir) {
            for entry in read_dir.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name == ".git" {
                        continue;
                    }
                    let ft = entry.file_type().ok();
                    let is_symlink = ft.as_ref().map(|t| t.is_symlink()).unwrap_or(false);
                    // Follow symlinks for is_dir: use metadata (follows symlinks)
                    let is_dir = if is_symlink {
                        std::fs::metadata(entry.path()).map(|m| m.is_dir()).unwrap_or(false)
                    } else {
                        ft.map(|t| t.is_dir()).unwrap_or(false)
                    };
                    entries.push((name.to_string(), entry.path(), is_dir, is_symlink));
                }
            }
        }

        // Sort: directories first, then alphabetical
        entries.sort_by(|a, b| {
            if a.2 == b.2 {
                a.0.to_lowercase().cmp(&b.0.to_lowercase())
            } else if a.2 {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            }
        });

        for (name, path, is_dir, is_symlink) in entries {
            self.entries.push(FileEntry {
                name,
                path,
                is_dir,
                is_symlink,
                depth,
                expanded: false,
                git_status: None,
                is_ignored: false,
            });
        }
    }

    pub fn toggle_expand(&mut self) {
        if self.selected >= self.entries.len() {
            return;
        }
        let entry = &self.entries[self.selected];
        if !entry.is_dir {
            return;
        }

        let was_expanded = entry.expanded;
        let path = entry.path.clone();
        let depth = entry.depth;

        if was_expanded {
            // Collapse: remove all children
            self.entries[self.selected].expanded = false;
            let remove_start = self.selected + 1;
            let mut remove_end = remove_start;
            while remove_end < self.entries.len() && self.entries[remove_end].depth > depth {
                remove_end += 1;
            }
            self.entries.drain(remove_start..remove_end);
        } else {
            // Expand: insert children after this entry
            self.entries[self.selected].expanded = true;
            let insert_pos = self.selected + 1;
            let mut children = Vec::new();

            if let Ok(read_dir) = std::fs::read_dir(&path) {
                let mut raw: Vec<(String, PathBuf, bool, bool)> = Vec::new();
                for entry in read_dir.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name == ".git" {
                            continue;
                        }
                        let ft = entry.file_type().ok();
                        let is_symlink = ft.as_ref().map(|t| t.is_symlink()).unwrap_or(false);
                        let is_dir = if is_symlink {
                            std::fs::metadata(entry.path()).map(|m| m.is_dir()).unwrap_or(false)
                        } else {
                            ft.map(|t| t.is_dir()).unwrap_or(false)
                        };
                        raw.push((name.to_string(), entry.path(), is_dir, is_symlink));
                    }
                }
                raw.sort_by(|a, b| {
                    if a.2 == b.2 {
                        a.0.to_lowercase().cmp(&b.0.to_lowercase())
                    } else if a.2 {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    }
                });

                for (name, path, is_dir, is_symlink) in raw {
                    children.push(FileEntry {
                        name,
                        path,
                        is_dir,
                        is_symlink,
                        depth: depth + 1,
                        expanded: false,
                        git_status: None,
                        is_ignored: false,
                    });
                }
            }

            let ignored = git_ignored_paths(&self.root);
            for (i, mut child) in children.into_iter().enumerate() {
                if let Ok(rel) = child.path.strip_prefix(&self.root) {
                    let rel_str = rel.to_string_lossy().to_string();
                    child.is_ignored =
                        ignored.contains(&rel_str) || ignored.contains(&format!("{}/", rel_str));
                }
                self.entries.insert(insert_pos + i, child);
            }
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    #[allow(dead_code)]
    pub fn selected_path(&self) -> Option<&Path> {
        self.entries.get(self.selected).map(|e| e.path.as_path())
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.entries.get(self.selected)
    }
}

/// Get set of relative paths that are ignored by git.
fn git_ignored_paths(root: &Path) -> HashSet<String> {
    let mut ignored = HashSet::new();
    // git ls-files --others --ignored --exclude-standard --directory
    // gives us ignored files/dirs relative to the repo root
    let output = Command::new("git")
        .args([
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
        ])
        .current_dir(root)
        .output();
    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            let trimmed = line.trim_end_matches('/');
            if !trimmed.is_empty() {
                ignored.insert(trimmed.to_string());
                // Also add with trailing slash for directory matching
                ignored.insert(format!("{}/", trimmed));
            }
        }
    }
    ignored
}
