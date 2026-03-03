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
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    Tree,
    Editor,
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

        // Tab — переключить фокус между деревом и редактором
        if matches!(key.code, Tab) && !ctrl && !shift {
            if self.active_editor.is_some() {
                self.focus = match self.focus {
                    Focus::Tree => Focus::Editor,
                    Focus::Editor => Focus::Tree,
                };
            }
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
            Tab => ed.insert_tab(self.config.tab_size),

            // Ctrl+A — выделить всё
            Char('a') if ctrl => ed.select_all(),

            // Ctrl+Shift+C / Ctrl+C — копировать
            Char('c') | Char('C') if ctrl => { ed.copy(); }

            // Ctrl+Shift+X / Ctrl+X — вырезать
            Char('x') | Char('X') if ctrl => { ed.cut(); }

            // Ctrl+V — вставить
            Char('v') if ctrl => ed.paste(),

            // Обычный ввод
            Char(c) if !ctrl && !key.modifiers.contains(Km::ALT) => ed.insert_char(c),

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
                    // Клик в редактор
                    self.focus = Focus::Editor;
                    if let Some(idx) = self.active_editor {
                        let line_num_w: u16 = if self.config.line_numbers { 5 } else { 0 };
                        let editor_x = x.saturating_sub(tree_w + 1 + line_num_w) as usize;
                        let editor_y = (y as usize).saturating_sub(1); // рамка
                        self.editors[idx].click(editor_x, editor_y);
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

        match Editor::open(path) {
            Ok(editor) => {
                self.editors.push(editor);
                self.active_editor = Some(self.editors.len() - 1);
                self.focus = Focus::Editor;
                self.status_msg = format!("Opened: {}", path.display());
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
}