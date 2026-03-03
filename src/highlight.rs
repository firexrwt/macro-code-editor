use std::path::Path;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;

pub struct Highlighter {
    ss: SyntaxSet,
    ts: ThemeSet,
    pub theme_name: String,
}

impl Highlighter {
    pub fn new(theme_name: &str) -> Self {
        Self {
            ss: SyntaxSet::load_defaults_newlines(),
            ts: ThemeSet::load_defaults(),
            theme_name: theme_name.to_string(),
        }
    }

    /// Подсвечивает весь файл за один проход (сохраняет состояние парсера между строками)
    pub fn highlight_file(&self, lines: &[String], path: &Path) -> Vec<Vec<(Style, String)>> {
        let syntax = self
            .ss
            .find_syntax_for_file(path)
            .ok()
            .flatten()
            .unwrap_or_else(|| self.ss.find_syntax_plain_text());

        let theme = self
            .ts
            .themes
            .get(&self.theme_name)
            .or_else(|| self.ts.themes.values().next())
            .expect("no themes loaded");

        let mut h = HighlightLines::new(syntax, theme);

        lines
            .iter()
            .map(|line| {
                // syntect ожидает строку с \n для корректной работы парсера
                let with_newline = format!("{line}\n");
                h.highlight_line(&with_newline, &self.ss)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(style, text)| {
                        // обрезаем \n которую добавили сами
                        (style, text.trim_end_matches('\n').to_string())
                    })
                    .collect()
            })
            .collect()
    }

    /// Доступные темы для конфига
    #[allow(dead_code)]
    pub fn theme_names(&self) -> Vec<&str> {
        self.ts.themes.keys().map(|s| s.as_str()).collect()
    }
}

/// Конвертация syntect RGB → ratatui Color
pub fn to_ratatui_color(c: syntect::highlighting::Color) -> ratatui::style::Color {
    ratatui::style::Color::Rgb(c.r, c.g, c.b)
}
