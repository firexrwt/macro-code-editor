use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Focus};
use crate::highlight::to_ratatui_color;

// ── Точка входа рендеринга ────────────────────────────────────────────────────

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    if app.active_editor.is_none() {
        render_tree_fullscreen(f, area, app);
    } else {
        let tree_w = app.config.tree_width.min(area.width.saturating_sub(20));
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(tree_w), Constraint::Min(0)])
            .split(area);

        render_tree_panel(f, chunks[0], app);
        render_editor_panel(f, chunks[1], app);
    }
}

// ── Дерево файлов ─────────────────────────────────────────────────────────────

fn tree_block<'a>(title: &'a str, focused: bool) -> Block<'a> {
    let style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(style)
}

fn render_tree_fullscreen(f: &mut Frame, area: Rect, app: &mut App) {
    let name = app
        .file_tree
        .root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Files");

    let block = tree_block(name, true);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Нижняя строка — статус / подсказка
    let hint_area = Rect { y: inner.y + inner.height.saturating_sub(1), height: 1, ..inner };
    let list_area = Rect { height: inner.height.saturating_sub(1), ..inner };

    draw_tree_list(f, list_area, app);

    let hint = Paragraph::new(" ↑↓/Enter: navigate · Tab: focus · Ctrl+Q: quit")
        .style(Style::default().fg(Color::DarkGray));
    f.render_widget(hint, hint_area);
}

fn render_tree_panel(f: &mut Frame, area: Rect, app: &mut App) {
    let name = app
        .file_tree
        .root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Files");

    let focused = matches!(app.focus, Focus::Tree);
    let block = tree_block(name, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    draw_tree_list(f, inner, app);
}

fn draw_tree_list(f: &mut Frame, area: Rect, app: &mut App) {
    let view_h = area.height as usize;
    app.file_tree.scroll_to_selected(view_h);

    let items: Vec<ListItem> = app
        .file_tree
        .entries
        .iter()
        .skip(app.file_tree.scroll)
        .take(view_h)
        .enumerate()
        .map(|(i, entry)| {
            let is_sel = app.file_tree.scroll + i == app.file_tree.selected;
            let indent = "  ".repeat(entry.depth);
            let icon = if entry.is_dir {
                if entry.is_expanded { "▾ " } else { "▸ " }
            } else {
                "  "
            };
            let label = format!("{indent}{icon}{}", entry.name);

            let style = if is_sel {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else if entry.is_dir {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(label).style(style)
        })
        .collect();

    f.render_widget(List::new(items), area);
}

// ── Редактор ──────────────────────────────────────────────────────────────────

fn render_editor_panel(f: &mut Frame, area: Rect, app: &mut App) {
    let Some(idx) = app.active_editor else { return };

    let filename = app.editors[idx].filename().to_string();
    let modified = app.editors[idx].modified;
    let focused = matches!(app.focus, Focus::Editor);

    let title = format!(
        " {}{} ",
        filename,
        if modified { " ●" } else { "" }
    );
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Разбиваем inner на контент + статусбар
    if inner.height < 2 {
        return;
    }
    let content_area = Rect { height: inner.height - 1, ..inner };
    let status_area = Rect {
        y: inner.y + inner.height - 1,
        height: 1,
        ..inner
    };

    let line_num_w: u16 = if app.config.line_numbers { 5 } else { 0 };
    let text_x = content_area.x + line_num_w;
    let text_w = content_area.width.saturating_sub(line_num_w);
    let text_area = Rect { x: text_x, width: text_w, ..content_area };

    let view_lines = content_area.height as usize;
    let view_cols = text_w as usize;

    // scroll_to_cursor требует мутабельный доступ к редактору
    app.editors[idx].scroll_to_cursor(view_lines, view_cols);

    let scroll_line = app.editors[idx].scroll.line;
    let scroll_col = app.editors[idx].scroll.col;
    let cursor = app.editors[idx].cursor;
    let path = app.editors[idx].path.clone();

    // Получаем строки для отображения
    let visible_lines: Vec<String> = app.editors[idx]
        .lines
        .iter()
        .skip(scroll_line)
        .take(view_lines)
        .cloned()
        .collect();

    // Подсветка всего файла за один проход (сохраняем состояние парсера)
    // Оптимизация: подсвечиваем только видимые строки + немного выше для контекста
    let highlight_start = scroll_line;
    let all_lines = &app.editors[idx].lines;
    let highlighted = app.highlighter.highlight_file(all_lines, &path);

    // Получаем выделение для рендеринга
    let selection = app.editors[idx].selection.clone();

    // Рендерим строки
    for (vis_i, (line_str, hi_spans)) in visible_lines
        .iter()
        .zip(highlighted.iter().skip(highlight_start))
        .enumerate()
    {
        let abs_line = scroll_line + vis_i;

        // Номер строки
        if app.config.line_numbers {
            let num_text = format!("{:>4} ", abs_line + 1);
            let num_style = if abs_line == cursor.line {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let num_area = Rect {
                x: content_area.x,
                y: content_area.y + vis_i as u16,
                width: line_num_w,
                height: 1,
            };
            f.render_widget(Paragraph::new(num_text).style(num_style), num_area);
        }

        // Текстовая строка
        let line_area = Rect {
            y: text_area.y + vis_i as u16,
            height: 1,
            ..text_area
        };

        let line_widget = build_highlighted_line(
            line_str,
            hi_spans,
            scroll_col,
            view_cols,
            abs_line,
            &selection,
        );
        f.render_widget(Paragraph::new(line_widget), line_area);
    }

    // Позиция курсора в терминале
    if focused {
        let cur_x = text_area.x + cursor.col.saturating_sub(scroll_col) as u16;
        let cur_y = content_area.y + cursor.line.saturating_sub(scroll_line) as u16;
        if cur_x < text_area.x + text_w
            && cur_y < content_area.y + content_area.height
        {
            f.set_cursor_position((cur_x, cur_y));
        }
    }

    // Статус-бар
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("txt");
    let status = format!(
        " {} | Ln {} Col {} | {} | {}",
        if modified { "●" } else { "·" },
        cursor.line + 1,
        cursor.col + 1,
        ext,
        app.status_msg
    );
    let status_widget = Paragraph::new(status)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(status_widget, status_area);
}

// ── Построение строки с подсветкой ───────────────────────────────────────────

fn build_highlighted_line<'a>(
    _raw: &str,
    spans: &[(syntect::highlighting::Style, String)],
    scroll_col: usize,
    view_cols: usize,
    abs_line: usize,
    selection: &Option<crate::editor::Selection>,
) -> Line<'a> {
    // Собираем плоский список (col, char, fg_color)
    let mut chars: Vec<(usize, char, ratatui::style::Color)> = Vec::new();
    let mut col = 0usize;
    for (style, text) in spans {
        let fg = to_ratatui_color(style.foreground);
        for c in text.chars() {
            chars.push((col, c, fg));
            col += 1;
        }
    }

    // Определяем диапазон выделения на этой строке
    let sel_range: Option<(usize, usize)> = selection.as_ref().and_then(|sel| {
        let (start, end) = sel.normalized();
        if abs_line < start.line || abs_line > end.line {
            return None;
        }
        let s = if abs_line == start.line { start.col } else { 0 };
        let e = if abs_line == end.line {
            end.col
        } else {
            usize::MAX
        };
        Some((s, e))
    });

    // Берём только видимые колонки и строим Span-ы
    let mut result: Vec<Span<'static>> = Vec::new();
    let visible: Vec<_> = chars
        .iter()
        .filter(|(c, _, _)| *c >= scroll_col && *c < scroll_col + view_cols)
        .collect();

    for (col_idx, ch, fg) in visible {
        let selected = sel_range
            .map(|(s, e)| *col_idx >= s && *col_idx < e)
            .unwrap_or(false);

        let style = if selected {
            Style::default().bg(Color::LightBlue).fg(Color::Black)
        } else {
            Style::default().fg(*fg)
        };
        result.push(Span::styled(ch.to_string(), style));
    }

    Line::from(result)
}
