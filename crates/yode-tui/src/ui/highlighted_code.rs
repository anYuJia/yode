use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use crate::app::rendering::{tokenize_code_line_with_language, CodeLanguage, CodeTokenKind};

const HIGHLIGHTED_CODE_CACHE_LIMIT: usize = 128;
static HIGHLIGHTED_CODE_CACHE: LazyLock<Mutex<HashMap<String, Vec<Line<'static>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) fn render_highlighted_code_block(
    code_block_lines: &[String],
    language: CodeLanguage,
    border: Color,
    background: Color,
) -> Vec<Line<'static>> {
    let cache_key = format!(
        "code:{language:?}:{border:?}:{background:?}:{}",
        code_block_lines.join("\n")
    );
    if let Ok(cache) = HIGHLIGHTED_CODE_CACHE.lock() {
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
    }

    let gutter_digits = code_block_lines.len().max(1).to_string().len();

    let rendered: Vec<Line<'static>> = code_block_lines
        .iter()
        .enumerate()
        .map(|(index, line)| render_highlighted_code_line(line, index + 1, gutter_digits, language, border, background))
        .collect();

    if let Ok(mut cache) = HIGHLIGHTED_CODE_CACHE.lock() {
        if cache.len() >= HIGHLIGHTED_CODE_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(cache_key, rendered.clone());
    }

    rendered
}

fn render_highlighted_code_line(
    line: &str,
    line_number: usize,
    gutter_digits: usize,
    language: CodeLanguage,
    border: Color,
    background: Color,
) -> Line<'static> {
    let mut spans = vec![
        Span::styled("  │ ", Style::default().fg(border).bg(background)),
        Span::styled(
            format!(" {:>width$} ", line_number, width = gutter_digits),
            Style::default().fg(Color::Indexed(244)).bg(background),
        ),
    ];

    for token in tokenize_code_line_with_language(line, language) {
        spans.push(Span::styled(
            token.text,
            Style::default()
                .fg(highlighted_code_token_color(token.kind, language))
                .bg(background),
        ));
    }

    if line.is_empty() {
        spans.push(Span::styled(" ", Style::default().bg(background)));
    }

    Line::from(spans)
}

fn highlighted_code_token_color(kind: CodeTokenKind, language: CodeLanguage) -> Color {
    match kind {
        CodeTokenKind::Plain => Color::Indexed(255),
        CodeTokenKind::String => Color::Indexed(180),
        CodeTokenKind::Number => Color::Indexed(151),
        CodeTokenKind::Keyword => match language {
            CodeLanguage::Shell => Color::Indexed(79),
            CodeLanguage::Rust | CodeLanguage::Python => Color::Indexed(111),
            _ => Color::Indexed(111),
        },
        CodeTokenKind::Comment => Color::Indexed(245),
        CodeTokenKind::Decorator => match language {
            CodeLanguage::Rust => Color::Indexed(179),
            _ => Color::Indexed(116),
        },
        CodeTokenKind::Operator => Color::Indexed(110),
        CodeTokenKind::Property => Color::Indexed(153),
        CodeTokenKind::Variable => Color::Indexed(215),
        CodeTokenKind::DiffAdded => Color::Indexed(114),
        CodeTokenKind::DiffRemoved => Color::Indexed(174),
        CodeTokenKind::DiffHunk => Color::Indexed(110),
        CodeTokenKind::DiffMeta => Color::Indexed(180),
        CodeTokenKind::DiffFile => Color::Indexed(223),
        CodeTokenKind::DiffLineNumber => Color::Indexed(153),
        CodeTokenKind::ShellPrompt => Color::Indexed(109),
        CodeTokenKind::ShellCommand => Color::Indexed(222),
        CodeTokenKind::ShellFlag => Color::Indexed(111),
        CodeTokenKind::ShellPath => Color::Indexed(153),
        CodeTokenKind::ShellInfo => Color::Indexed(153),
        CodeTokenKind::ShellSuccess => Color::Indexed(114),
        CodeTokenKind::ShellWarning => Color::Indexed(179),
        CodeTokenKind::ShellOutput => Color::Indexed(246),
        CodeTokenKind::ShellError => Color::Indexed(210),
    }
}
