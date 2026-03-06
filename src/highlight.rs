use std::path::Path;
use syntect::highlighting::{
    Highlighter as SyntectHighlighter, HighlightIterator, HighlightState, Style, ThemeSet,
};
use syntect::parsing::{ParseState, ScopeStack, SyntaxSet};

pub struct Highlighter {
    ss: SyntaxSet,
    ts: ThemeSet,
    pub theme_name: String,
}

/// Per-file incremental highlight cache.
/// Stores the highlighted spans and parser states for each line.
pub struct HighlightCache {
    /// Highlighted spans for each line.
    pub spans: Vec<Vec<(Style, String)>>,
    /// Parser state before line 0 (initial state for this syntax).
    initial_parse: ParseState,
    initial_highlight: HighlightState,
    /// Parser state after line i — used to resume from line i+1.
    parse_states: Vec<ParseState>,
    highlight_states: Vec<HighlightState>,
}

impl Highlighter {
    pub fn new(theme_name: &str) -> Self {
        Self {
            ss: SyntaxSet::load_defaults_newlines(),
            ts: ThemeSet::load_defaults(),
            theme_name: theme_name.to_string(),
        }
    }

    /// Create an empty cache for the given file path (determines syntax).
    pub fn new_cache(&self, path: &Path) -> HighlightCache {
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

        let initial_parse = ParseState::new(syntax);
        let h = SyntectHighlighter::new(theme);
        let initial_highlight = HighlightState::new(&h, ScopeStack::new());

        HighlightCache {
            spans: Vec::new(),
            initial_parse,
            initial_highlight,
            parse_states: Vec::new(),
            highlight_states: Vec::new(),
        }
    }

    /// Re-highlight lines starting from `from_line`, updating `cache` in place.
    /// Lines before `from_line` are left untouched.
    /// Stops early when the parser state after a line matches the cached state
    /// (meaning the rest of the file is unaffected).
    pub fn highlight_from(
        &self,
        lines: &[String],
        from_line: usize,
        cache: &mut HighlightCache,
    ) {
        // Early exit is only safe when no lines were inserted or deleted.
        let line_count_unchanged = lines.len() == cache.spans.len();

        let theme = self
            .ts
            .themes
            .get(&self.theme_name)
            .or_else(|| self.ts.themes.values().next())
            .expect("no themes loaded");
        let h = SyntectHighlighter::new(theme);

        // Resume from the state just before from_line
        let (mut parse_state, mut highlight_state) = if from_line == 0 {
            (cache.initial_parse.clone(), cache.initial_highlight.clone())
        } else {
            (
                cache.parse_states[from_line - 1].clone(),
                cache.highlight_states[from_line - 1].clone(),
            )
        };

        for i in from_line..lines.len() {
            let line_nl = format!("{}\n", lines[i]);
            let ops = parse_state
                .parse_line(&line_nl, &self.ss)
                .unwrap_or_default();

            let spans: Vec<(Style, String)> = {
                let iter = HighlightIterator::new(&mut highlight_state, &ops, &line_nl, &h);
                iter.map(|(style, text)| (style, text.trim_end_matches('\n').to_string()))
                    .collect()
            };

            // Update or extend the spans cache
            if i < cache.spans.len() {
                cache.spans[i] = spans;
            } else {
                cache.spans.push(spans);
            }

            let new_parse = parse_state.clone();

            // Early exit: if no lines were inserted/deleted and the parse state
            // after this line matches the cached one, the rest is unaffected.
            if line_count_unchanged
                && i < cache.parse_states.len()
                && new_parse == cache.parse_states[i]
            {
                // Truncate to current line count (handles line deletions)
                cache.spans.truncate(lines.len());
                cache.parse_states.truncate(lines.len());
                cache.highlight_states.truncate(lines.len());
                return;
            }

            if i < cache.parse_states.len() {
                cache.parse_states[i] = new_parse;
                cache.highlight_states[i] = highlight_state.clone();
            } else {
                cache.parse_states.push(new_parse);
                cache.highlight_states.push(highlight_state.clone());
            }
        }

        // Truncate to current line count (handles line deletions)
        cache.spans.truncate(lines.len());
        cache.parse_states.truncate(lines.len());
        cache.highlight_states.truncate(lines.len());
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
