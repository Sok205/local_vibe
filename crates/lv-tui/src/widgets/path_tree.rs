use std::path::{Path, PathBuf};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Block,
};

use super::selectable_list::{Item, SelectableList};

#[derive(Debug, Clone)]
pub enum EntryKind {
    Parent,
    Dir(PathBuf),
    File(PathBuf),
    Denied,
}

#[derive(Debug, Clone)]
pub struct PathEntry {
    pub name: String,
    pub kind: EntryKind,
}

/// Directory walker widget: wraps a `SelectableList<PathEntry>` and keeps the
/// current directory in sync. Files are listed but rendered dim and skipped
/// when moving the cursor.
pub struct PathTree {
    current: PathBuf,
    list: SelectableList<PathEntry>,
}

impl PathTree {
    pub fn new(start: &Path) -> std::io::Result<Self> {
        let mut tree = Self {
            current: start.to_path_buf(),
            list: SelectableList::new(Vec::new()),
        };
        tree.refresh()?;
        Ok(tree)
    }

    pub fn current(&self) -> &Path {
        &self.current
    }

    pub fn list_mut(&mut self) -> &mut SelectableList<PathEntry> {
        &mut self.list
    }

    pub fn list(&self) -> &SelectableList<PathEntry> {
        &self.list
    }

    pub fn selected(&self) -> Option<&PathEntry> {
        self.list.selected_meta()
    }

    /// Descend if the selection is a directory, ascend if it's `..`. No-op
    /// otherwise.
    pub fn activate(&mut self) -> std::io::Result<()> {
        match self.selected().cloned() {
            Some(PathEntry { kind: EntryKind::Parent, .. }) => {
                if let Some(parent) = self.current.parent() {
                    self.current = parent.to_path_buf();
                    self.refresh()?;
                }
            }
            Some(PathEntry { kind: EntryKind::Dir(p), .. }) => {
                self.current = p;
                self.refresh()?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Returns the selected directory path if the current cursor is on one.
    pub fn selected_dir(&self) -> Option<PathBuf> {
        match self.selected()? {
            PathEntry { kind: EntryKind::Dir(p), .. } => Some(p.clone()),
            PathEntry { kind: EntryKind::Parent, .. } => self.current.parent().map(|p| p.to_path_buf()),
            _ => None,
        }
    }

    /// Directory the user's "pick-current" keybind should commit to.
    pub fn pick_current_dir(&self) -> PathBuf {
        self.current.clone()
    }

    pub fn refresh(&mut self) -> std::io::Result<()> {
        let mut entries: Vec<PathEntry> = vec![PathEntry {
            name: "..".to_string(),
            kind: EntryKind::Parent,
        }];
        match std::fs::read_dir(&self.current) {
            Ok(rd) => {
                let mut collected: Vec<PathEntry> = rd
                    .flatten()
                    .filter_map(|e| {
                        let file_type = e.file_type().ok()?;
                        let name = e.file_name().to_string_lossy().into_owned();
                        if name.starts_with('.') {
                            return None;
                        }
                        let path = e.path();
                        if file_type.is_dir() {
                            Some(PathEntry { name, kind: EntryKind::Dir(path) })
                        } else if file_type.is_file() {
                            Some(PathEntry { name, kind: EntryKind::File(path) })
                        } else {
                            None
                        }
                    })
                    .collect();
                collected.sort_by(|a, b| {
                    let a_dir = matches!(a.kind, EntryKind::Dir(_));
                    let b_dir = matches!(b.kind, EntryKind::Dir(_));
                    b_dir.cmp(&a_dir).then_with(|| a.name.cmp(&b.name))
                });
                entries.extend(collected);
            }
            Err(_) => {
                entries.push(PathEntry {
                    name: "(permission denied)".to_string(),
                    kind: EntryKind::Denied,
                });
            }
        }
        let items: Vec<Item<PathEntry>> = entries
            .into_iter()
            .map(|e| {
                let line = match &e.kind {
                    EntryKind::Parent => Line::from(Span::styled(
                        "..".to_string(),
                        Style::default().fg(Color::Gray),
                    )),
                    EntryKind::Dir(_) => Line::from(Span::styled(
                        format!("{}/", e.name),
                        Style::default().fg(Color::Cyan),
                    )),
                    EntryKind::File(_) => Line::from(Span::styled(
                        e.name.clone(),
                        Style::default().fg(Color::DarkGray),
                    )),
                    EntryKind::Denied => Line::from(Span::styled(
                        e.name.clone(),
                        Style::default().fg(Color::Red),
                    )),
                };
                Item::new(line, e)
            })
            .collect();
        self.list.replace_items(items);
        Ok(())
    }

    pub fn draw(&mut self, frame: &mut Frame, area: Rect, block: Block<'_>) {
        self.list.draw(frame, area, block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn lists_only_directories_and_files_no_hidden() {
        let td = tempdir().unwrap();
        std::fs::create_dir(td.path().join("src")).unwrap();
        std::fs::create_dir(td.path().join("docs")).unwrap();
        std::fs::write(td.path().join("Cargo.toml"), "").unwrap();
        std::fs::write(td.path().join(".hidden"), "").unwrap();

        let tree = PathTree::new(td.path()).unwrap();
        let items = tree.list().items();
        let names: Vec<&str> = items.iter().map(|i| i.meta.name.as_str()).collect();
        assert_eq!(names[0], "..");
        assert!(names.contains(&"src"));
        assert!(names.contains(&"docs"));
        assert!(names.contains(&"Cargo.toml"));
        assert!(!names.contains(&".hidden"));
    }

    #[test]
    fn directories_sort_before_files_alphabetical() {
        let td = tempdir().unwrap();
        std::fs::write(td.path().join("zfile.txt"), "").unwrap();
        std::fs::create_dir(td.path().join("alpha")).unwrap();
        std::fs::create_dir(td.path().join("beta")).unwrap();

        let tree = PathTree::new(td.path()).unwrap();
        let items = tree.list().items();
        let names: Vec<&str> = items.iter().map(|i| i.meta.name.as_str()).collect();
        assert_eq!(names, vec!["..", "alpha", "beta", "zfile.txt"]);
    }

    #[test]
    fn ascend_via_parent_entry() {
        let td = tempdir().unwrap();
        let inner = td.path().join("child");
        std::fs::create_dir(&inner).unwrap();
        let mut tree = PathTree::new(&inner).unwrap();
        assert_eq!(tree.current(), inner.as_path());
        // Parent entry is index 0 — already selected
        tree.activate().unwrap();
        assert_eq!(tree.current(), td.path());
    }
}
