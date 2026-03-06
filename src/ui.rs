use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs},
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

    if app.creating_file.is_some() {
        render_create_file_popup(f, app, area);
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
    let focused = matches!(app.focus, Focus::Editor);

    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    // ── Таббар ───────────────────────────────────────────────────────────────
    let tabs_area = Rect { height: 1, ..inner };
    let rest = Rect { y: inner.y + 1, height: inner.height - 1, ..inner };

    let tab_titles: Vec<Line> = app.editors.iter().map(|ed| {
        let label = if ed.modified {
            format!(" {} ● ", ed.filename())
        } else {
            format!(" {} ", ed.filename())
        };
        Line::from(label)
    }).collect();

    let tabs = Tabs::new(tab_titles)
        .select(idx)
        .style(Style::default().fg(Color::DarkGray))
        .highlight_style(
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        )
        .divider("│");
    f.render_widget(tabs, tabs_area);

    // ── Разбиваем остаток на контент + статусбар ──────────────────────────────
    let content_area = Rect { height: rest.height - 1, ..rest };
    let status_area = Rect {
        y: rest.y + rest.height - 1,
        height: 1,
        ..rest
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

    // Подсветка: инкрементальный пересчёт начиная с dirty_from_line.
    // Note: dirty_from_line is taken before highlight_from. If highlight_from
    // were to panic, the marker would be lost and subsequent frames would not
    // re-highlight. In practice the only panic path is "no themes loaded" which
    // cannot happen after successful initialisation.
    if let Some(from) = app.editors[idx].dirty_from_line.take() {
        let cache = app.highlight_caches
            .entry(path.clone())
            .or_insert_with(|| app.highlighter.new_cache(&path));
        app.highlighter.highlight_from(&app.editors[idx].lines, from, cache);
    }
    let highlighted = match app.highlight_caches.get(&path) {
        Some(h) => h,
        None => return,
    };
    // Cache must cover at least scroll_line lines; zip will silently drop
    // visible lines if the cache is shorter (should not happen in normal flow).
    debug_assert!(
        highlighted.spans.len() >= scroll_line || highlighted.spans.is_empty(),
        "highlight cache shorter than scroll_line"
    );
    let highlight_start = scroll_line;

    // Получаем выделение для рендеринга
    let selection = app.editors[idx].selection.clone();

    // Рендерим строки
    for (vis_i, (line_str, hi_spans)) in visible_lines
        .iter()
        .zip(highlighted.spans.iter().skip(highlight_start))
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
    let cur_x = text_area.x + cursor.col.saturating_sub(scroll_col) as u16;
    let cur_y = content_area.y + cursor.line.saturating_sub(scroll_line) as u16;
    if focused {
        if cur_x < text_area.x + text_w
            && cur_y < content_area.y + content_area.height
        {
            f.set_cursor_position((cur_x, cur_y));
        }
    }

    // Completion popup
    if focused {
        render_completion_popup(f, app, cur_x, cur_y, area);
    }

    // Статус-бар
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("txt");
    let is_modified = app.editors[idx].modified;
    let status = format!(
        " {} | Ln {} Col {} | {} | {}",
        if is_modified { "●" } else { "·" },
        cursor.line + 1,
        cursor.col + 1,
        ext,
        app.status_msg
    );
    let status_widget = Paragraph::new(status)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));
    f.render_widget(status_widget, status_area);
}

// ── Create file popup ─────────────────────────────────────────────────────────

fn render_create_file_popup(f: &mut Frame, app: &App, screen: Rect) {
    let input = match &app.creating_file {
        Some(s) => s,
        None => return,
    };

    let popup_w = 50u16.min(screen.width.saturating_sub(4));
    let popup_h = 5u16;
    let popup_x = screen.x + (screen.width.saturating_sub(popup_w)) / 2;
    let popup_y = screen.y + (screen.height.saturating_sub(popup_h)) / 2;
    let popup_rect = Rect { x: popup_x, y: popup_y, width: popup_w, height: popup_h };

    let block = Block::default()
        .title(" New file (Enter to create, Esc to cancel) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .style(Style::default().bg(Color::DarkGray));
    let inner = block.inner(popup_rect);

    f.render_widget(Clear, popup_rect);
    f.render_widget(block, popup_rect);

    // Hint line
    let hint_area = Rect { y: inner.y + inner.height.saturating_sub(1), height: 1, ..inner };
    let input_area = Rect { height: 1, ..inner };

    let display = format!("{}_", input); // underscore as cursor
    f.render_widget(
        Paragraph::new(display).style(Style::default().fg(Color::White)),
        input_area,
    );
    f.render_widget(
        Paragraph::new("  relative path from project root, e.g. src/foo.rs")
            .style(Style::default().fg(Color::DarkGray)),
        hint_area,
    );
}

// ── Completion popup ──────────────────────────────────────────────────────────

fn render_completion_popup(f: &mut Frame, app: &App, cur_x: u16, cur_y: u16, screen: Rect) {
    let Some(comp) = &app.completion else { return };
    if comp.filtered.is_empty() { return }

    let count = comp.filtered.len().min(8) as u16;
    let max_w = comp.filtered.iter().map(|i| i.label.len()).max().unwrap_or(4) as u16;
    let popup_w = (max_w + 2).min(40).max(10);
    let popup_h = count + 2; // borders

    // Position below cursor; flip up if near bottom
    let popup_y = if cur_y + 1 + popup_h <= screen.bottom() {
        cur_y + 1
    } else {
        cur_y.saturating_sub(popup_h)
    };
    let popup_x = cur_x.min(screen.right().saturating_sub(popup_w));

    let popup_rect = Rect {
        x: popup_x,
        y: popup_y,
        width: popup_w,
        height: popup_h,
    };

    let selected = comp.selected.min(comp.filtered.len().saturating_sub(1));
    let items: Vec<ListItem> = comp
        .filtered
        .iter()
        .take(8)
        .enumerate()
        .map(|(i, item)| {
            let style = if i == selected {
                Style::default()
                    .bg(Color::Blue)
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            };
            ListItem::new(item.label.clone()).style(style)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .style(Style::default().bg(Color::DarkGray));

    let inner = block.inner(popup_rect);

    f.render_widget(Clear, popup_rect);
    f.render_widget(block, popup_rect);
    f.render_widget(List::new(items), inner);
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
    // Determine selection range on this line
    let sel_range: Option<(usize, usize)> = selection.as_ref().and_then(|sel| {
        let (start, end) = sel.normalized();
        if abs_line < start.line || abs_line > end.line {
            return None;
        }
        let s = if abs_line == start.line { start.col } else { 0 };
        let e = if abs_line == end.line { end.col } else { usize::MAX };
        Some((s, e))
    });

    // Build merged spans: group consecutive chars that share the same ratatui Style
    let mut result: Vec<Span<'static>> = Vec::new();
    let mut current_style: Option<Style> = None;
    let mut current_text = String::new();
    let mut col = 0usize;

    for (hi_style, text) in spans {
        let fg = to_ratatui_color(hi_style.foreground);
        for ch in text.chars() {
            if col >= scroll_col && col < scroll_col + view_cols {
                let selected = sel_range.map(|(s, e)| col >= s && col < e).unwrap_or(false);
                let style = if selected {
                    Style::default().bg(Color::LightBlue).fg(Color::Black)
                } else {
                    Style::default().fg(fg)
                };

                if Some(style) == current_style {
                    current_text.push(ch);
                } else {
                    if !current_text.is_empty() {
                        result.push(Span::styled(
                            current_text.clone(),
                            current_style.unwrap(),
                        ));
                        current_text.clear();
                    }
                    current_style = Some(style);
                    current_text.push(ch);
                }
            }
            col += 1;
            if col >= scroll_col + view_cols {
                break;
            }
        }
        if col >= scroll_col + view_cols {
            break;
        }
    }
    if !current_text.is_empty() {
        result.push(Span::styled(current_text, current_style.unwrap()));
    }

    Line::from(result)
}
