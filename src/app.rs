use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers,
        MouseButton, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use crate::config::Config;
use crate::editor::Editor;
use crate::file_tree::FileTree;
use crate::highlight::Highlighter;
use crate::lsp::{file_uri, language_id, CompletionItem, LspClient};
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Tree,
    Editor,
}

pub struct CompletionState {
    pub all_items: Vec<CompletionItem>,
    /// Items filtered by current word prefix — this is what the popup shows.
    pub filtered: Vec<CompletionItem>,
    pub selected: usize,
    pub word_start: usize,
    pub trigger_line: usize,
}

pub struct App {
    pub config: Config,
    pub file_tree: FileTree,
    pub editors: Vec<Editor>,
    pub active_editor: Option<usize>,
    pub focus: Focus,
    pub highlighter: Highlighter,
    pub status_msg: String,
    pub should_quit: bool,
    /// Ожидаем второй Ctrl+Q для force-close несохранённого файла
    pub pending_force_close: bool,
    // ── Highlight cache (per editor index) ───────────────────────────────────
    pub highlight_caches: HashMap<usize, Vec<Vec<(syntect::highlighting::Style, String)>>>,
    // ── LSP ──────────────────────────────────────────────────────────────────
    pub lsp_clients: HashMap<String, LspClient>,
    pub completion: Option<CompletionState>,
    pub pending_completion_id: Option<i64>,
    doc_versions: HashMap<PathBuf, i64>,
}

impl App {
    pub fn new(path: PathBuf) -> Result<Self> {
        let config = Config::load().unwrap_or_default();
        let highlighter = Highlighter::new(&config.theme);

        let root = if path.is_dir() {
            path.clone()
        } else if path.is_file() {
            path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            })
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        };

        let file_tree = FileTree::new(root);

        let mut app = Self {
            config,
            file_tree,
            editors: Vec::new(),
            active_editor: None,
            focus: Focus::Tree,
            highlighter,
            status_msg: String::new(),
            should_quit: false,
            pending_force_close: false,
            highlight_caches: HashMap::new(),
            lsp_clients: HashMap::new(),
            completion: None,
            pending_completion_id: None,
            doc_versions: HashMap::new(),
        };

        if path.is_file() {
            let _ = app.open_file(&path);
        }

        Ok(app)
    }

    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let result = self.event_loop(&mut terminal);

        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        result
    }

    // ── Главный цикл ─────────────────────────────────────────────────────────

    fn event_loop(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<()> {
        loop {
            terminal.draw(|f| ui::render(f, self))?;

            if event::poll(std::time::Duration::from_millis(16))? {
                match event::read()? {
                    Event::Key(k) => self.handle_key(k),
                    Event::Mouse(m) => self.handle_mouse(m, terminal.size().unwrap_or_default()),
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            self.poll_lsp();

            if self.should_quit {
                break;
            }
        }
        Ok(())
    }

    // ── Клавиатура ───────────────────────────────────────────────────────────

    fn handle_key(&mut self, key: event::KeyEvent) {
        use KeyCode::*;
        use KeyModifiers as Km;

        let ctrl = key.modifiers.contains(Km::CONTROL);
        let shift = key.modifiers.contains(Km::SHIFT);

        // Любая клавиша кроме Ctrl+Q сбрасывает ожидание force-close
        let is_ctrl_q = ctrl && !shift && matches!(key.code, Char('q'));
        if !is_ctrl_q && self.pending_force_close {
            self.pending_force_close = false;
            self.status_msg.clear();
        }

        // ── Глобальные шорткаты ───────────────────────────────────────────────

        // Ctrl+Q — закрыть файл (или выйти если только дерево).
        // Если файл несохранён: первый раз показывает предупреждение,
        // второй Ctrl+Q подряд — закрывает без сохранения.
        if ctrl && !shift && matches!(key.code, Char('q')) {
            self.try_close_editor();
            return;
        }

        // Ctrl+S — сохранить
        if ctrl && !shift && matches!(key.code, Char('s')) {
            self.save_current();
            return;
        }

        // Escape из редактора — сначала закрыть completion, потом перейти в дерево
        if matches!(key.code, Esc) && self.focus == Focus::Editor {
            if self.completion.is_some() {
                self.completion = None;
                return;
            }
            self.focus = Focus::Tree;
            return;
        }

        // ── Делегируем по фокусу ─────────────────────────────────────────────
        match self.focus {
            Focus::Tree => self.handle_tree_key(key),
            Focus::Editor => self.handle_editor_key(key),
        }
    }

    fn handle_tree_key(&mut self, key: event::KeyEvent) {
        match key.code {
            KeyCode::Up => self.file_tree.move_up(),
            KeyCode::Down => self.file_tree.move_down(),
            KeyCode::Enter => {
                if let Some(path) = self.file_tree.activate() {
                    let _ = self.open_file(&path);
                }
            }
            _ => {}
        }
    }

    fn handle_editor_key(&mut self, key: event::KeyEvent) {
        use KeyCode::*;
        use KeyModifiers as Km;

        let ctrl = key.modifiers.contains(Km::CONTROL);
        let shift = key.modifiers.contains(Km::SHIFT);

        let Some(idx) = self.active_editor else { return };

        // ── Completion popup navigation ───────────────────────────────────────
        if self.completion.is_some() {
            match key.code {
                Enter => {
                    if let Some(comp) = self.completion.take() {
                        let sel = comp.selected.min(comp.filtered.len().saturating_sub(1));
                        if let Some(item) = comp.filtered.get(sel) {
                            let text = item.insert_text.clone();
                            self.editors[idx].replace_word(comp.word_start, &text);
                        }
                    }
                    return;
                }
                Tab | Down => {
                    if let Some(ref mut c) = self.completion {
                        let n = c.filtered.len();
                        if n > 0 { c.selected = (c.selected + 1) % n; }
                    }
                    return;
                }
                Up => {
                    if let Some(ref mut c) = self.completion {
                        let n = c.filtered.len();
                        if n > 0 {
                            c.selected = if c.selected == 0 { n - 1 } else { c.selected - 1 };
                        }
                    }
                    return;
                }
                Backspace => {
                    self.editors[idx].backspace();
                    self.update_completion_filter(idx);
                    return;
                }
                // Char: let fall through — handled below with auto-trigger
                Char(_) => {}
                // Esc handled globally; anything else dismisses
                _ => { self.completion = None; }
            }
        }

        // ── Tab: apply completion if open, else insert indent ─────────────────
        if matches!(key.code, Tab) && !ctrl && !shift {
            if let Some(comp) = self.completion.take() {
                let sel = comp.selected.min(comp.filtered.len().saturating_sub(1));
                if let Some(item) = comp.filtered.get(sel) {
                    let text = item.insert_text.clone();
                    self.editors[idx].replace_word(comp.word_start, &text);
                }
            } else {
                self.editors[idx].insert_tab(self.config.tab_size);
            }
            return;
        }

        // ── Char input: insert + auto-trigger completion ──────────────────────
        if let Char(c) = key.code {
            if !ctrl && !key.modifiers.contains(Km::ALT) {
                self.editors[idx].insert_char(c);
                if self.completion.is_some() {
                    self.update_completion_filter(idx);
                } else if self.config.auto_complete {
                    self.auto_trigger_completion(idx);
                }
                return;
            }
        }

        // ── Normal editing (movement, delete, ctrl shortcuts) ─────────────────
        let ed = &mut self.editors[idx];
        match key.code {
            Left => ed.move_left(shift),
            Right => ed.move_right(shift),
            Up => ed.move_up(shift),
            Down => ed.move_down(shift),
            Home => ed.move_home(shift),
            End => ed.move_end(shift),
            PageUp => ed.page_up(20),
            PageDown => ed.page_down(20),
            Backspace => ed.backspace(),
            Delete => ed.delete_key(),
            Enter => ed.insert_newline(),
            Char('a') if ctrl => ed.select_all(),
            Char('c') | Char('C') if ctrl => { ed.copy(); }
            Char('x') | Char('X') if ctrl => { ed.cut(); }
            Char('v') if ctrl => ed.paste(),
            _ => {}
        }
    }

    // ── Мышь ─────────────────────────────────────────────────────────────────

    fn handle_mouse(&mut self, mouse: event::MouseEvent, size: ratatui::layout::Size) {
        let tree_w = if self.active_editor.is_some() {
            self.config.tree_width
        } else {
            size.width
        };

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let x = mouse.column;
                let y = mouse.row;

                if self.active_editor.is_none() || x < tree_w {
                    // Клик в дерево (y - 1: учитываем рамку сверху)
                    self.focus = Focus::Tree;
                    let tree_y = (y as usize).saturating_sub(1);
                    if let Some(path) = self.file_tree.click_row(tree_y) {
                        let _ = self.open_file(&path);
                    }
                } else {
                    self.focus = Focus::Editor;
                    // y=1 (после рамки) — это строка таббара
                    if y == 1 && self.editors.len() > 1 {
                        // Определяем по какой вкладке кликнули
                        let rel_x = x.saturating_sub(tree_w + 1) as usize;
                        let mut offset = 0usize;
                        for (i, ed) in self.editors.iter().enumerate() {
                            let tab_w = ed.filename().len() + if ed.modified { 5 } else { 3 };
                            if rel_x < offset + tab_w {
                                self.active_editor = Some(i);
                                break;
                            }
                            offset += tab_w + 1; // +1 для разделителя │
                        }
                    } else {
                        // Клик в текст редактора
                        if let Some(idx) = self.active_editor {
                            let line_num_w: u16 = if self.config.line_numbers { 5 } else { 0 };
                            let editor_x = x.saturating_sub(tree_w + 1 + line_num_w) as usize;
                            let editor_y = (y as usize).saturating_sub(2); // рамка + таббар
                            self.editors[idx].click(editor_x, editor_y);
                        }
                    }
                }
            }

            MouseEventKind::ScrollUp => match self.focus {
                Focus::Tree => {
                    for _ in 0..3 { self.file_tree.move_up(); }
                }
                Focus::Editor => {
                    if let Some(idx) = self.active_editor {
                        for _ in 0..3 { self.editors[idx].move_up(false); }
                    }
                }
            },

            MouseEventKind::ScrollDown => match self.focus {
                Focus::Tree => {
                    for _ in 0..3 { self.file_tree.move_down(); }
                }
                Focus::Editor => {
                    if let Some(idx) = self.active_editor {
                        for _ in 0..3 { self.editors[idx].move_down(false); }
                    }
                }
            },

            _ => {}
        }
    }

    // ── Управление файлами ───────────────────────────────────────────────────

    pub fn open_file(&mut self, path: &Path) -> Result<()> {
        // Уже открыт?
        if let Some(idx) = self.editors.iter().position(|e| e.path == path) {
            self.active_editor = Some(idx);
            self.focus = Focus::Editor;
            return Ok(());
        }

        if self.editors.len() >= 8 {
            self.status_msg = "Max 8 files open. Close one with Ctrl+Q first.".to_string();
            return Ok(());
        }

        match Editor::open(path) {
            Ok(editor) => {
                self.editors.push(editor);
                let new_idx = self.editors.len() - 1;
                self.active_editor = Some(new_idx);
                self.focus = Focus::Editor;
                self.status_msg = format!("Opened: {}", path.display());
                self.ensure_lsp(new_idx);
            }
            Err(e) => {
                self.status_msg = format!("Error: {e}");
                return Err(e);
            }
        }
        Ok(())
    }

    fn try_close_editor(&mut self) {
        if let Some(idx) = self.active_editor {
            if self.editors[idx].modified {
                if self.pending_force_close {
                    // Второй Ctrl+Q — закрываем без сохранения
                    self.pending_force_close = false;
                    self.close_editor();
                } else {
                    // Первый Ctrl+Q — предупреждаем
                    self.pending_force_close = true;
                    self.status_msg =
                        "Unsaved changes! Ctrl+S to save, Ctrl+Q again to discard".to_string();
                }
                return;
            }
        }
        self.pending_force_close = false;
        self.close_editor();
    }

    fn close_editor(&mut self) {
        if let Some(idx) = self.active_editor {
            self.editors.remove(idx);
            self.highlight_caches.clear(); // indices shift after remove
            if self.editors.is_empty() {
                self.active_editor = None;
                self.focus = Focus::Tree;
            } else {
                self.active_editor = Some(idx.saturating_sub(1));
            }
            self.status_msg.clear();
        } else {
            self.should_quit = true;
        }
    }

    fn save_current(&mut self) {
        if let Some(idx) = self.active_editor {
            match self.editors[idx].save() {
                Ok(_) => {
                    let name = self.editors[idx].filename().to_string();
                    self.status_msg = format!("Saved: {name}");
                }
                Err(e) => self.status_msg = format!("Save error: {e}"),
            }
        }
    }

    // ── LSP ──────────────────────────────────────────────────────────────────

    /// Start the LSP server for `editors[idx]`'s language if not already running.
    fn ensure_lsp(&mut self, idx: usize) {
        let lang = match self.editors[idx]
            .path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(language_id)
        {
            Some(l) => l.to_string(),
            None => return,
        };
        if !self.lsp_clients.contains_key(&lang) {
            let root = self.file_tree.root.to_string_lossy().to_string();
            if let Some(client) = LspClient::start(&lang, &root) {
                self.lsp_clients.insert(lang, client);
            }
        }
    }

    /// Handle Tab key: request LSP completion if there's a word before the cursor,
    /// otherwise fall back to inserting spaces.
    /// Silently request LSP completion after a char is typed.
    /// Called automatically while typing if `config.auto_complete` is true.
    fn auto_trigger_completion(&mut self, idx: usize) {
        let ext_str = self.editors[idx].path.extension()
            .and_then(|e| e.to_str()).unwrap_or("").to_string();
        let Some(lang_str) = language_id(&ext_str) else { return };
        let lang = lang_str.to_string();

        let word_start = self.editors[idx].word_start_col();
        let cursor = self.editors[idx].cursor;
        if word_start == cursor.col { return; }

        if !self.lsp_clients.contains_key(&lang) {
            let root = self.file_tree.root.to_string_lossy().to_string();
            if let Some(client) = LspClient::start(&lang, &root) {
                self.lsp_clients.insert(lang.clone(), client);
            }
            return;
        }
        if !self.lsp_clients[&lang].initialized { return; }

        let uri = file_uri(&self.editors[idx].path);
        let content = self.editors[idx].lines.join("\n");
        let version = {
            let v = self.doc_versions.entry(self.editors[idx].path.clone()).or_insert(0);
            *v += 1;
            *v
        };
        let client = self.lsp_clients.get_mut(&lang).unwrap();
        client.ensure_open(&uri, &lang, &content);
        client.notify_change(&uri, version, &content);
        let id = client.request_completion(&uri, cursor.line as u32, cursor.col as u32);
        self.pending_completion_id = Some(id);
    }

    /// Recompute filtered list based on the word prefix at current cursor position.
    /// Dismisses completion if cursor moved off the trigger line or before word_start.
    fn update_completion_filter(&mut self, idx: usize) {
        // Determine if we should dismiss
        let should_dismiss = match &self.completion {
            None => return,
            Some(comp) => {
                let cur = self.editors[idx].cursor;
                cur.line != comp.trigger_line || cur.col < comp.word_start
            }
        };
        if should_dismiss { self.completion = None; return; }

        // Compute current prefix (owned String → no borrow conflict)
        let prefix = {
            let comp = self.completion.as_ref().unwrap();
            let cur = self.editors[idx].cursor;
            let line = &self.editors[idx].lines[cur.line];
            let start = line.char_indices().nth(comp.word_start).map(|(b, _)| b).unwrap_or(0);
            let end   = line.char_indices().nth(cur.col).map(|(b, _)| b).unwrap_or(line.len());
            line[start..end].to_lowercase()
        };

        // Compute filtered list (cloned → no long borrow)
        let new_filtered: Vec<CompletionItem> = {
            let comp = self.completion.as_ref().unwrap();
            comp.all_items.iter()
                .filter(|item| {
                    prefix.is_empty()
                        || item.label.to_lowercase().starts_with(&prefix)
                })
                .take(20)
                .cloned()
                .collect()
        };

        if new_filtered.is_empty() {
            self.completion = None;
        } else {
            let comp = self.completion.as_mut().unwrap();
            let sel = comp.selected.min(new_filtered.len() - 1);
            comp.filtered = new_filtered;
            comp.selected = sel;
        }
    }

    /// Poll active editor's LSP client for completion results.
    fn poll_lsp(&mut self) {
        let Some(idx) = self.active_editor else { return };

        let lang = match self.editors[idx]
            .path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(language_id)
        {
            Some(l) => l.to_string(),
            None => return,
        };

        let pending = self.pending_completion_id;
        let items = match self.lsp_clients.get_mut(&lang) {
            Some(client) => client.poll(pending),
            None => return,
        };

        if !items.is_empty() {
            let word_start = self.editors[idx].word_start_col();
            let trigger_line = self.editors[idx].cursor.line;
            self.completion = Some(CompletionState {
                filtered: items.clone(),
                all_items: items,
                selected: 0,
                word_start,
                trigger_line,
            });
            self.pending_completion_id = None;
            self.status_msg.clear();
            // Apply filter based on what user may have typed while waiting
            self.update_completion_filter(idx);
        }
    }
}