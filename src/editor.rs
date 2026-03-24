use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

impl Pos {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

#[derive(Debug, Clone)]
pub struct Selection {
    pub anchor: Pos,
    pub cursor: Pos,
}

impl Selection {
    /// Returns (start, end) in normalized (left-to-right) order.
    pub fn normalized(&self) -> (Pos, Pos) {
        let (a, b) = (self.anchor, self.cursor);
        if a.line < b.line || (a.line == b.line && a.col <= b.col) {
            (a, b)
        } else {
            (b, a)
        }
    }
}

pub struct Editor {
    pub path: PathBuf,
    pub lines: Vec<String>,
    pub cursor: Pos,
    pub scroll: Pos,
    pub selection: Option<Selection>,
    pub modified: bool,
    pub clipboard: Option<String>,
    /// First line that needs re-highlighting. None = cache is clean.
    pub dirty_from_line: Option<usize>,
}

impl Editor {
    /// Create an editor from a pre-built list of lines (used in tests).
    #[cfg(test)]
    pub fn from_lines(path: PathBuf, lines: Vec<String>) -> Self {
        let lines = if lines.is_empty() { vec![String::new()] } else { lines };
        Self {
            path,
            lines,
            cursor: Pos::new(0, 0),
            scroll: Pos::new(0, 0),
            selection: None,
            modified: false,
            clipboard: None,
            dirty_from_line: Some(0),
        }
    }

    pub fn open(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let lines = if lines.is_empty() { vec![String::new()] } else { lines };

        Ok(Self {
            path: path.to_path_buf(),
            lines,
            cursor: Pos::new(0, 0),
            scroll: Pos::new(0, 0),
            selection: None,
            modified: false,
            clipboard: None,
            dirty_from_line: Some(0),
        })
    }

    pub fn save(&mut self) -> Result<()> {
        let content = self.lines.join("\n");
        std::fs::write(&self.path, content)?;
        self.modified = false;
        Ok(())
    }

    pub fn filename(&self) -> &str {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("untitled")
    }

    #[allow(dead_code)]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn line_char_count(&self, line: usize) -> usize {
        self.lines[line].chars().count()
    }

    fn char_to_byte(&self, line: usize, col: usize) -> usize {
        self.lines[line]
            .char_indices()
            .nth(col)
            .map(|(i, _)| i)
            .unwrap_or(self.lines[line].len())
    }

    fn clamp_col(&self, line: usize, col: usize) -> usize {
        col.min(self.line_char_count(line))
    }

    fn mark_dirty(&mut self, line: usize) {
        self.dirty_from_line = Some(match self.dirty_from_line {
            Some(existing) => existing.min(line),
            None => line,
        });
    }

    // ── Text input ───────────────────────────────────────────────────────────

    pub fn insert_char(&mut self, c: char) {
        if self.selection.is_some() {
            self.delete_selection();
        }
        let byte = self.char_to_byte(self.cursor.line, self.cursor.col);
        self.lines[self.cursor.line].insert(byte, c);
        self.cursor.col += 1;
        self.modified = true;
        self.mark_dirty(self.cursor.line);
    }

    pub fn insert_newline(&mut self) {
        if self.selection.is_some() {
            self.delete_selection();
        }
        // Carry the leading whitespace of the current line to the new line
        let indent: String = self.lines[self.cursor.line]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        let byte = self.char_to_byte(self.cursor.line, self.cursor.col);
        let rest = self.lines[self.cursor.line][byte..].to_string();
        self.lines[self.cursor.line].truncate(byte);
        let indent_len = indent.chars().count();
        self.lines.insert(self.cursor.line + 1, format!("{}{}", indent, rest));
        let dirty_line = self.cursor.line;
        self.cursor.line += 1;
        self.cursor.col = indent_len;
        self.modified = true;
        self.mark_dirty(dirty_line);
    }

    pub fn insert_tab(&mut self, tab_size: usize) {
        self.mark_dirty(self.cursor.line);
        for _ in 0..tab_size {
            let byte = self.char_to_byte(self.cursor.line, self.cursor.col);
            self.lines[self.cursor.line].insert(byte, ' ');
            self.cursor.col += 1;
        }
        self.modified = true;
    }

    // ── Deletion ─────────────────────────────────────────────────────────────

    pub fn backspace(&mut self) {
        if self.selection.is_some() {
            self.delete_selection();
            return;
        }
        // Delete both brackets/quotes if cursor sits between a matching pair
        if self.cursor.col > 0 {
            let chars: Vec<char> = self.lines[self.cursor.line].chars().collect();
            if let (Some(&p), Some(&n)) = (
                chars.get(self.cursor.col - 1),
                chars.get(self.cursor.col),
            ) {
                if is_matching_pair(p, n) {
                    let byte_n = self.char_to_byte(self.cursor.line, self.cursor.col);
                    self.lines[self.cursor.line].remove(byte_n);
                    self.cursor.col -= 1;
                    let byte_p = self.char_to_byte(self.cursor.line, self.cursor.col);
                    self.lines[self.cursor.line].remove(byte_p);
                    self.modified = true;
                    self.mark_dirty(self.cursor.line);
                    return;
                }
            }
        }
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
            let byte = self.char_to_byte(self.cursor.line, self.cursor.col);
            self.lines[self.cursor.line].remove(byte);
            self.modified = true;
            self.mark_dirty(self.cursor.line);
        } else if self.cursor.line > 0 {
            let current = self.lines.remove(self.cursor.line);
            self.cursor.line -= 1;
            let prev_len = self.line_char_count(self.cursor.line);
            self.lines[self.cursor.line].push_str(&current);
            self.cursor.col = prev_len;
            self.modified = true;
            self.mark_dirty(self.cursor.line);
        }
    }

    pub fn delete_key(&mut self) {
        if self.selection.is_some() {
            self.delete_selection();
            return;
        }
        let line_len = self.line_char_count(self.cursor.line);
        if self.cursor.col < line_len {
            let byte = self.char_to_byte(self.cursor.line, self.cursor.col);
            self.lines[self.cursor.line].remove(byte);
            self.modified = true;
            self.mark_dirty(self.cursor.line);
        } else if self.cursor.line + 1 < self.lines.len() {
            let next = self.lines.remove(self.cursor.line + 1);
            self.lines[self.cursor.line].push_str(&next);
            self.modified = true;
            self.mark_dirty(self.cursor.line);
        }
    }

    // ── Cursor movement ──────────────────────────────────────────────────────

    pub fn move_left(&mut self, selecting: bool) {
        self.update_selection(selecting);
        if self.cursor.col > 0 {
            self.cursor.col -= 1;
        } else if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.line_char_count(self.cursor.line);
        }
        if selecting {
            self.extend_selection();
        } else {
            self.selection = None;
        }
    }

    pub fn move_right(&mut self, selecting: bool) {
        self.update_selection(selecting);
        let len = self.line_char_count(self.cursor.line);
        if self.cursor.col < len {
            self.cursor.col += 1;
        } else if self.cursor.line + 1 < self.lines.len() {
            self.cursor.line += 1;
            self.cursor.col = 0;
        }
        if selecting {
            self.extend_selection();
        } else {
            self.selection = None;
        }
    }

    pub fn move_up(&mut self, selecting: bool) {
        self.update_selection(selecting);
        if self.cursor.line > 0 {
            self.cursor.line -= 1;
            self.cursor.col = self.clamp_col(self.cursor.line, self.cursor.col);
        }
        if selecting {
            self.extend_selection();
        } else {
            self.selection = None;
        }
    }

    pub fn move_down(&mut self, selecting: bool) {
        self.update_selection(selecting);
        if self.cursor.line + 1 < self.lines.len() {
            self.cursor.line += 1;
            self.cursor.col = self.clamp_col(self.cursor.line, self.cursor.col);
        }
        if selecting {
            self.extend_selection();
        } else {
            self.selection = None;
        }
    }

    pub fn move_home(&mut self, selecting: bool) {
        self.update_selection(selecting);
        self.cursor.col = 0;
        if selecting { self.extend_selection(); } else { self.selection = None; }
    }

    pub fn move_end(&mut self, selecting: bool) {
        self.update_selection(selecting);
        self.cursor.col = self.line_char_count(self.cursor.line);
        if selecting { self.extend_selection(); } else { self.selection = None; }
    }

    pub fn page_up(&mut self, lines: usize) {
        self.selection = None;
        self.cursor.line = self.cursor.line.saturating_sub(lines);
        self.cursor.col = self.clamp_col(self.cursor.line, self.cursor.col);
    }

    pub fn page_down(&mut self, lines: usize) {
        self.selection = None;
        self.cursor.line = (self.cursor.line + lines).min(self.lines.len().saturating_sub(1));
        self.cursor.col = self.clamp_col(self.cursor.line, self.cursor.col);
    }

    // ── Selection ────────────────────────────────────────────────────────────

    fn update_selection(&mut self, selecting: bool) {
        if selecting && self.selection.is_none() {
            self.selection = Some(Selection {
                anchor: self.cursor,
                cursor: self.cursor,
            });
        }
    }

    fn extend_selection(&mut self) {
        if let Some(sel) = &mut self.selection {
            sel.cursor = self.cursor;
        }
    }

    pub fn select_all(&mut self) {
        let last_line = self.lines.len().saturating_sub(1);
        let last_col = self.line_char_count(last_line);
        self.cursor = Pos::new(last_line, last_col);
        self.selection = Some(Selection {
            anchor: Pos::new(0, 0),
            cursor: self.cursor,
        });
    }

    // ── Clipboard ────────────────────────────────────────────────────────────

    pub fn copy(&mut self) -> Option<String> {
        let sel = self.selection.as_ref()?;
        let (start, end) = sel.normalized();
        let text = self.extract_text(start, end);
        self.clipboard = Some(text.clone());
        Some(text)
    }

    pub fn cut(&mut self) -> Option<String> {
        let text = self.copy()?;
        self.delete_selection();
        Some(text)
    }

    pub fn paste(&mut self) {
        if let Some(text) = self.clipboard.clone() {
            if self.selection.is_some() {
                self.delete_selection();
            }
            for (i, chunk) in text.split('\n').enumerate() {
                if i > 0 {
                    self.insert_newline();
                }
                for c in chunk.chars() {
                    self.insert_char(c);
                }
            }
        }
    }

    // ── Internal helpers ─────────────────────────────────────────────────────

    fn extract_text(&self, start: Pos, end: Pos) -> String {
        if start.line == end.line {
            let s = self.char_to_byte(start.line, start.col);
            let e = self.char_to_byte(end.line, end.col);
            return self.lines[start.line][s..e].to_string();
        }
        let mut out = String::new();
        let s = self.char_to_byte(start.line, start.col);
        out.push_str(&self.lines[start.line][s..]);
        for l in start.line + 1..end.line {
            out.push('\n');
            out.push_str(&self.lines[l]);
        }
        let e = self.char_to_byte(end.line, end.col);
        out.push('\n');
        out.push_str(&self.lines[end.line][..e]);
        out
    }

    fn delete_selection(&mut self) {
        let Some(sel) = self.selection.take() else { return };
        let (start, end) = sel.normalized();

        if start.line == end.line {
            let s = self.char_to_byte(start.line, start.col);
            let e = self.char_to_byte(end.line, end.col);
            self.lines[start.line].drain(s..e);
        } else {
            let tail = {
                let e = self.char_to_byte(end.line, end.col);
                self.lines[end.line][e..].to_string()
            };
            let s = self.char_to_byte(start.line, start.col);
            self.lines[start.line].truncate(s);
            self.lines[start.line].push_str(&tail);
            self.lines.drain(start.line + 1..=end.line);
        }
        self.cursor = start;
        self.modified = true;
        self.mark_dirty(start.line);
    }

    // ── Scrolling ────────────────────────────────────────────────────────────

    pub fn scroll_to_cursor(&mut self, view_lines: usize, view_cols: usize) {
        if self.cursor.line < self.scroll.line {
            self.scroll.line = self.cursor.line;
        }
        if self.cursor.line >= self.scroll.line + view_lines {
            self.scroll.line = self.cursor.line.saturating_sub(view_lines.saturating_sub(1));
        }
        if self.cursor.col < self.scroll.col {
            self.scroll.col = self.cursor.col;
        }
        if self.cursor.col >= self.scroll.col + view_cols {
            self.scroll.col = self.cursor.col.saturating_sub(view_cols.saturating_sub(1));
        }
    }

    /// Column where the current word starts (alphanumeric / underscore chars before cursor).
    pub fn word_start_col(&self) -> usize {
        let chars: Vec<char> = self.lines[self.cursor.line].chars().collect();
        let mut col = self.cursor.col;
        while col > 0 && (chars[col - 1].is_alphanumeric() || chars[col - 1] == '_') {
            col -= 1;
        }
        col
    }

    /// Replace text from `from_col` to current cursor col on the same line, then insert `new_text`.
    pub fn replace_word(&mut self, from_col: usize, new_text: &str) {
        let line = &self.lines[self.cursor.line];
        let char_indices: Vec<(usize, char)> = line.char_indices().collect();
        let from_byte = char_indices.get(from_col).map(|(b, _)| *b).unwrap_or(line.len());
        let to_byte = char_indices
            .get(self.cursor.col)
            .map(|(b, _)| *b)
            .unwrap_or(line.len());
        let before = line[..from_byte].to_string();
        let after = line[to_byte..].to_string();
        self.lines[self.cursor.line] = format!("{}{}{}", before, new_text, after);
        self.cursor.col = from_col + new_text.chars().count();
        self.modified = true;
        self.mark_dirty(self.cursor.line);
    }

    /// Insert an opening bracket and its closing pair, placing the cursor between them.
    pub fn insert_pair(&mut self, open: char, close: char) {
        if self.selection.is_some() {
            self.delete_selection();
        }
        let byte = self.char_to_byte(self.cursor.line, self.cursor.col);
        // Insert close first so the open inserted at the same offset pushes it right
        self.lines[self.cursor.line].insert(byte, close);
        self.lines[self.cursor.line].insert(byte, open);
        self.cursor.col += 1;
        self.modified = true;
        self.mark_dirty(self.cursor.line);
    }

    /// Returns the character immediately after the cursor (on the same line).
    pub fn char_at_cursor(&self) -> Option<char> {
        self.lines[self.cursor.line].chars().nth(self.cursor.col)
    }

    /// Collect words from the buffer that start with `prefix` (case-insensitive).
    /// Used as a fallback when no LSP is available.
    pub fn buffer_word_completions(&self, prefix: &str) -> Vec<String> {
        if prefix.len() < 2 {
            return Vec::new();
        }
        let prefix_lower = prefix.to_lowercase();
        let mut seen = std::collections::HashSet::new();
        let mut results = Vec::new();
        for line in &self.lines {
            for word in line.split(|c: char| !c.is_alphanumeric() && c != '_') {
                if word.len() <= prefix.len() {
                    continue;
                }
                if word.to_lowercase().starts_with(&prefix_lower)
                    && seen.insert(word.to_string())
                {
                    results.push(word.to_string());
                }
            }
        }
        results.sort();
        results.truncate(20);
        results
    }

    /// Move cursor to a mouse click position (accounting for scroll offset).
    pub fn click(&mut self, text_x: usize, text_y: usize) {
        self.selection = None;
        let line = (self.scroll.line + text_y).min(self.lines.len().saturating_sub(1));
        let col = (self.scroll.col + text_x).min(self.line_char_count(line));
        self.cursor = Pos::new(line, col);
    }

    /// Extend selection while the mouse button is held (drag).
    /// The anchor is the position where the Down click occurred.
    pub fn drag(&mut self, text_x: usize, text_y: usize) {
        let anchor = match &self.selection {
            Some(sel) => sel.anchor,
            None => self.cursor,
        };
        let line = (self.scroll.line + text_y).min(self.lines.len().saturating_sub(1));
        let col = (self.scroll.col + text_x).min(self.line_char_count(line));
        self.cursor = Pos::new(line, col);
        self.selection = Some(Selection { anchor, cursor: self.cursor });
    }

    /// Move cursor left by `n` columns (horizontal mouse scroll).
    pub fn scroll_left_cols(&mut self, n: usize) {
        self.selection = None;
        self.cursor.col = self.cursor.col.saturating_sub(n);
    }

    /// Move cursor right by `n` columns (horizontal mouse scroll).
    pub fn scroll_right_cols(&mut self, n: usize) {
        self.selection = None;
        let max_col = self.line_char_count(self.cursor.line);
        self.cursor.col = (self.cursor.col + n).min(max_col);
    }
}

fn is_matching_pair(open: char, close: char) -> bool {
    matches!(
        (open, close),
        ('(', ')') | ('[', ']') | ('{', '}') | ('"', '"') | ('\'', '\'') | ('`', '`')
    )
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(content: &str) -> Editor {
        let lines = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(|l| l.to_string()).collect()
        };
        Editor::from_lines(PathBuf::from("test.txt"), lines)
    }

    // ── Input ────────────────────────────────────────────────────────────────

    #[test]
    fn insert_chars() {
        let mut e = ed("");
        e.insert_char('h');
        e.insert_char('i');
        assert_eq!(e.lines[0], "hi");
        assert_eq!(e.cursor.col, 2);
    }

    #[test]
    fn insert_char_mid_line() {
        let mut e = ed("ac");
        e.cursor.col = 1;
        e.insert_char('b');
        assert_eq!(e.lines[0], "abc");
    }

    #[test]
    fn insert_newline_splits() {
        let mut e = ed("hello world");
        e.cursor.col = 5;
        e.insert_newline();
        assert_eq!(e.lines[0], "hello");
        assert_eq!(e.lines[1], " world");
        assert_eq!(e.cursor, Pos::new(1, 0));
    }

    #[test]
    fn insert_newline_at_start() {
        let mut e = ed("hello");
        e.cursor.col = 0;
        e.insert_newline();
        assert_eq!(e.lines[0], "");
        assert_eq!(e.lines[1], "hello");
    }

    #[test]
    fn insert_tab_inserts_spaces() {
        let mut e = ed("");
        e.insert_tab(4);
        assert_eq!(e.lines[0], "    ");
        assert_eq!(e.cursor.col, 4);
    }

    // ── Backspace / Delete ────────────────────────────────────────────────────

    #[test]
    fn backspace_removes_char() {
        let mut e = ed("hello");
        e.cursor.col = 5;
        e.backspace();
        assert_eq!(e.lines[0], "hell");
        assert_eq!(e.cursor.col, 4);
    }

    #[test]
    fn backspace_joins_lines() {
        let mut e = ed("foo\nbar");
        e.cursor = Pos::new(1, 0);
        e.backspace();
        assert_eq!(e.lines.len(), 1);
        assert_eq!(e.lines[0], "foobar");
        assert_eq!(e.cursor.col, 3);
    }

    #[test]
    fn backspace_at_very_start_noop() {
        let mut e = ed("hello");
        e.backspace();
        assert_eq!(e.lines[0], "hello");
        assert!(!e.modified);
    }

    #[test]
    fn delete_key_removes_char() {
        let mut e = ed("hello");
        e.cursor.col = 0;
        e.delete_key();
        assert_eq!(e.lines[0], "ello");
    }

    #[test]
    fn delete_key_joins_lines() {
        let mut e = ed("foo\nbar");
        e.cursor = Pos::new(0, 3);
        e.delete_key();
        assert_eq!(e.lines.len(), 1);
        assert_eq!(e.lines[0], "foobar");
    }

    // ── Cursor movement ───────────────────────────────────────────────────────

    #[test]
    fn move_right_wraps() {
        let mut e = ed("ab\ncd");
        e.cursor = Pos::new(0, 2);
        e.move_right(false);
        assert_eq!(e.cursor, Pos::new(1, 0));
    }

    #[test]
    fn move_left_wraps() {
        let mut e = ed("ab\ncd");
        e.cursor = Pos::new(1, 0);
        e.move_left(false);
        assert_eq!(e.cursor, Pos::new(0, 2));
    }

    #[test]
    fn move_down_clamps_col() {
        let mut e = ed("hello\nhi");
        e.cursor.col = 5;
        e.move_down(false);
        assert_eq!(e.cursor.col, 2);
    }

    #[test]
    fn home_end() {
        let mut e = ed("hello world");
        e.cursor.col = 5;
        e.move_home(false);
        assert_eq!(e.cursor.col, 0);
        e.move_end(false);
        assert_eq!(e.cursor.col, 11);
    }

    #[test]
    fn page_up_down_clamp() {
        let mut e = ed("a\nb\nc");
        e.cursor.line = 2;
        e.page_up(100);
        assert_eq!(e.cursor.line, 0);
        e.page_down(100);
        assert_eq!(e.cursor.line, 2);
    }

    // ── Selection ─────────────────────────────────────────────────────────────

    #[test]
    fn shift_right_creates_selection() {
        let mut e = ed("hello");
        e.move_right(true);
        e.move_right(true);
        let (start, end) = e.selection.as_ref().unwrap().normalized();
        assert_eq!(start.col, 0);
        assert_eq!(end.col, 2);
    }

    #[test]
    fn select_all_covers_content() {
        let mut e = ed("foo\nbar\nbaz");
        e.select_all();
        let (start, end) = e.selection.as_ref().unwrap().normalized();
        assert_eq!(start, Pos::new(0, 0));
        assert_eq!(end, Pos::new(2, 3));
    }

    // ── Clipboard ─────────────────────────────────────────────────────────────

    #[test]
    fn copy_single_line() {
        let mut e = ed("hello world");
        e.cursor.col = 0;
        for _ in 0..5 { e.move_right(true); }
        assert_eq!(e.copy().unwrap(), "hello");
        assert!(e.selection.is_some()); // selection is preserved after copy
    }

    #[test]
    fn cut_removes_selection() {
        let mut e = ed("hello world");
        e.cursor.col = 0;
        for _ in 0..5 { e.move_right(true); }
        assert_eq!(e.cut().unwrap(), "hello");
        assert_eq!(e.lines[0], " world");
        assert_eq!(e.cursor.col, 0);
    }

    #[test]
    fn paste_inserts_text() {
        let mut e = ed("world");
        e.clipboard = Some("hello ".to_string());
        e.cursor.col = 0;
        e.paste();
        assert_eq!(e.lines[0], "hello world");
    }

    #[test]
    fn copy_multiline() {
        let mut e = ed("foo\nbar\nbaz");
        e.cursor = Pos::new(0, 0);
        e.move_down(true);
        e.move_end(true);
        assert_eq!(e.copy().unwrap(), "foo\nbar");
    }

    #[test]
    fn select_all_then_cut_clears() {
        let mut e = ed("foo\nbar\nbaz");
        e.select_all();
        e.cut();
        assert_eq!(e.lines.len(), 1);
        assert_eq!(e.lines[0], "");
    }

    // ── dirty_from_line ───────────────────────────────────────────────────────

    #[test]
    fn dirty_from_line_tracks_minimum() {
        let mut e = ed("a\nb\nc");
        e.dirty_from_line = None;
        e.cursor = Pos::new(2, 0);
        e.insert_char('x');
        assert_eq!(e.dirty_from_line, Some(2));
        e.cursor = Pos::new(1, 0);
        e.insert_char('y');
        assert_eq!(e.dirty_from_line, Some(1));
    }

    #[test]
    fn insert_newline_marks_dirty_at_current_line() {
        let mut e = ed("hello world");
        e.cursor.col = 5;
        e.insert_newline();
        assert_eq!(e.dirty_from_line, Some(0));
    }

    #[test]
    fn backspace_join_marks_dirty_at_joined_line() {
        let mut e = ed("foo\nbar");
        e.cursor = Pos::new(1, 0);
        e.dirty_from_line = None;
        e.backspace();
        assert_eq!(e.dirty_from_line, Some(0));
    }

    // ── Modified flag ─────────────────────────────────────────────────────────

    #[test]
    fn modified_set_on_edit() {
        let mut e = ed("hello");
        assert!(!e.modified);
        e.insert_char('!');
        assert!(e.modified);
    }

    #[test]
    fn modified_not_set_on_noop_backspace() {
        let mut e = ed("hello");
        e.backspace(); // cursor at col=0, nothing to delete
        assert!(!e.modified);
    }

    // ── Unicode ───────────────────────────────────────────────────────────────

    #[test]
    fn insert_unicode_chars() {
        let mut e = ed("");
        e.insert_char('п');
        e.insert_char('р');
        e.insert_char('и');
        assert_eq!(e.lines[0], "при");
        assert_eq!(e.cursor.col, 3);
    }

    #[test]
    fn backspace_unicode() {
        let mut e = ed("привет");
        e.cursor.col = 6;
        e.backspace();
        assert_eq!(e.lines[0], "приве");
    }
}
