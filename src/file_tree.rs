use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub is_expanded: bool,
}

pub struct FileTree {
    pub root: PathBuf,
    pub entries: Vec<TreeEntry>,
    pub selected: usize,
    pub scroll: usize,
}

impl FileTree {
    pub fn new(root: PathBuf) -> Self {
        let mut tree = Self {
            root: root.clone(),
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
        };
        tree.rebuild();
        tree
    }

    #[allow(dead_code)]
    pub fn selected_entry(&self) -> Option<&TreeEntry> {
        self.entries.get(self.selected)
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

    /// Enter на выбранном элементе: раскрыть директорию или вернуть путь файла
    pub fn activate(&mut self) -> Option<PathBuf> {
        let entry = self.entries.get(self.selected)?.clone();
        if entry.is_dir {
            self.entries[self.selected].is_expanded = !entry.is_expanded;
            self.rebuild_keeping_state();
            None
        } else {
            Some(entry.path)
        }
    }

    /// Клик по строке (y — позиция в видимой области дерева)
    pub fn click_row(&mut self, y: usize) -> Option<PathBuf> {
        let idx = self.scroll + y;
        if idx >= self.entries.len() {
            return None;
        }
        self.selected = idx;
        self.activate()
    }

    pub fn scroll_to_selected(&mut self, view_height: usize) {
        if view_height == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        }
        if self.selected >= self.scroll + view_height {
            self.scroll = self.selected.saturating_sub(view_height - 1);
        }
    }

    fn rebuild(&mut self) {
        self.entries.clear();
        collect_dir(&self.root, 0, &mut self.entries);
    }

    /// Перестроить с сохранением состояния раскрытых папок
    fn rebuild_keeping_state(&mut self) {
        let expanded: Vec<PathBuf> = self
            .entries
            .iter()
            .filter(|e| e.is_dir && e.is_expanded)
            .map(|e| e.path.clone())
            .collect();

        self.entries.clear();
        collect_dir_with_expanded(&self.root, 0, &expanded, &mut self.entries);
    }
}

fn collect_dir(dir: &std::path::Path, depth: usize, out: &mut Vec<TreeEntry>) {
    let mut children = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect::<Vec<_>>(),
        Err(_) => return,
    };

    children.sort_by(|a, b| {
        let a_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let b_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
        match (a_dir, b_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.file_name().cmp(&b.file_name()),
        }
    });

    for child in children {
        let name = child.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && depth > 0 {
            continue; // скрываем dotfiles в поддиректориях
        }
        let path = child.path();
        let is_dir = child.file_type().map(|t| t.is_dir()).unwrap_or(false);
        out.push(TreeEntry { path, name, depth, is_dir, is_expanded: false });
    }
}

fn collect_dir_with_expanded(
    dir: &std::path::Path,
    depth: usize,
    expanded: &[PathBuf],
    out: &mut Vec<TreeEntry>,
) {
    let mut children = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect::<Vec<_>>(),
        Err(_) => return,
    };

    children.sort_by(|a, b| {
        let a_dir = a.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let b_dir = b.file_type().map(|t| t.is_dir()).unwrap_or(false);
        match (a_dir, b_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.file_name().cmp(&b.file_name()),
        }
    });

    for child in children {
        let name = child.file_name().to_string_lossy().to_string();
        if name.starts_with('.') && depth > 0 {
            continue;
        }
        let path = child.path();
        let is_dir = child.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let is_expanded = is_dir && expanded.contains(&path);

        out.push(TreeEntry {
            path: path.clone(),
            name,
            depth,
            is_dir,
            is_expanded,
        });

        if is_expanded {
            collect_dir_with_expanded(&path, depth + 1, expanded, out);
        }
    }
}
