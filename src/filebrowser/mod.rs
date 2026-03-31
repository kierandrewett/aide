use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
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
        if self.root.exists() {
            self.load_dir(&self.root.clone(), 0);
        }
    }

    fn load_dir(&mut self, dir: &Path, depth: usize) {
        let mut entries: Vec<(String, PathBuf, bool)> = Vec::new();

        if let Ok(read_dir) = std::fs::read_dir(dir) {
            for entry in read_dir.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    if name.starts_with('.') {
                        continue;
                    }
                    let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                    entries.push((name.to_string(), entry.path(), is_dir));
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

        for (name, path, is_dir) in entries {
            let expanded = false;
            self.entries.push(FileEntry {
                name,
                path,
                is_dir,
                depth,
                expanded,
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
            let mut remove_start = self.selected + 1;
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
                let mut raw: Vec<(String, PathBuf, bool)> = Vec::new();
                for entry in read_dir.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.starts_with('.') {
                            continue;
                        }
                        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                        raw.push((name.to_string(), entry.path(), is_dir));
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

                for (name, path, is_dir) in raw {
                    children.push(FileEntry {
                        name,
                        path,
                        is_dir,
                        depth: depth + 1,
                        expanded: false,
                    });
                }
            }

            // Insert children
            for (i, child) in children.into_iter().enumerate() {
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

    pub fn selected_path(&self) -> Option<&Path> {
        self.entries.get(self.selected).map(|e| e.path.as_path())
    }
}
