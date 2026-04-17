use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use similar::{ChangeTag, TextDiff};

use super::chat::CODE_BG;
use super::palette::{INFO_COLOR, LIGHT, MUTED};
use crate::app::rendering::{
    detect_code_language_from_path, tokenize_code_line_with_language, CodeLanguage, CodeTokenKind,
};

const STRUCTURED_DIFF_CACHE_LIMIT: usize = 128;
static STRUCTURED_DIFF_CACHE: LazyLock<Mutex<HashMap<String, Vec<Line<'static>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Copy)]
struct StructuredDiffTheme {
    border: Color,
    base_bg: Color,
    meta_bg: Color,
    hunk_bg: Color,
    add_bg: Color,
    add_word_bg: Color,
    remove_bg: Color,
    remove_word_bg: Color,
    line_no: Color,
    meta: Color,
    hunk: Color,
    file: Color,
    add_marker: Color,
    remove_marker: Color,
    context_marker: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StructuredDiffLineKind {
    Meta,
    Hunk,
    Added,
    Removed,
    Context,
    Plain,
}

#[derive(Debug, Clone)]
struct StructuredDiffLine {
    kind: StructuredDiffLineKind,
    raw: String,
    body: String,
    line_number: Option<usize>,
    language: CodeLanguage,
    word_segments: Option<Vec<StructuredDiffSegment>>,
}

#[derive(Debug, Clone)]
struct StructuredDiffState {
    current_language: CodeLanguage,
    old_line: Option<usize>,
    new_line: Option<usize>,
}

#[derive(Debug, Clone)]
struct StructuredDiffSegment {
    text: String,
    emphasized: bool,
}

impl Default for StructuredDiffState {
    fn default() -> Self {
        Self {
            current_language: CodeLanguage::Plain,
            old_line: None,
            new_line: None,
        }
    }
}

pub(super) fn render_structured_diff_block(code_block_lines: &[String]) -> Vec<Line<'static>> {
    let cache_key = format!("diff:{}", code_block_lines.join("\n"));
    if let Ok(cache) = STRUCTURED_DIFF_CACHE.lock() {
        if let Some(cached) = cache.get(&cache_key) {
            return cached.clone();
        }
    }

    let theme = structured_diff_theme();
    let mut parsed_lines = parse_structured_diff_lines(code_block_lines);
    apply_word_diff_pairs(&mut parsed_lines);
    let max_line_number = parsed_lines
        .iter()
        .filter_map(|line| line.line_number)
        .max()
        .unwrap_or(1);
    let gutter_digits = max_line_number.to_string().len();

    let rendered: Vec<Line<'static>> = parsed_lines
        .iter()
        .map(|line| render_structured_diff_line(line, gutter_digits, theme))
        .collect();

    if let Ok(mut cache) = STRUCTURED_DIFF_CACHE.lock() {
        if cache.len() >= STRUCTURED_DIFF_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(cache_key, rendered.clone());
    }

    rendered
}

fn structured_diff_theme() -> StructuredDiffTheme {
    StructuredDiffTheme {
        border: Color::Indexed(67),
        base_bg: CODE_BG,
        meta_bg: Color::Indexed(236),
        hunk_bg: Color::Indexed(237),
        add_bg: Color::Indexed(22),
        add_word_bg: Color::Indexed(28),
        remove_bg: Color::Indexed(52),
        remove_word_bg: Color::Indexed(88),
        line_no: Color::Indexed(244),
        meta: Color::Indexed(180),
        hunk: INFO_COLOR,
        file: Color::Indexed(223),
        add_marker: Color::Indexed(114),
        remove_marker: Color::Indexed(174),
        context_marker: Color::Indexed(242),
    }
}

fn parse_structured_diff_lines(lines: &[String]) -> Vec<StructuredDiffLine> {
    let mut state = StructuredDiffState::default();
    let mut parsed = Vec::with_capacity(lines.len());

    for line in lines {
        if let Some(path) = diff_language_path(line) {
            state.current_language = detect_code_language_from_path(&path);
        }

        let parsed_line = if let Some((old_start, new_start)) = parse_diff_hunk_header(line) {
            state.old_line = Some(old_start);
            state.new_line = Some(new_start);
            StructuredDiffLine {
                kind: StructuredDiffLineKind::Hunk,
                raw: line.clone(),
                body: line.clone(),
                line_number: None,
                language: CodeLanguage::Diff,
                word_segments: None,
            }
        } else if is_diff_meta_line(line) {
            StructuredDiffLine {
                kind: StructuredDiffLineKind::Meta,
                raw: line.clone(),
                body: line.clone(),
                line_number: None,
                language: CodeLanguage::Diff,
                word_segments: None,
            }
        } else if line.starts_with('+') && !line.starts_with("+++") {
            let line_number = state.new_line;
            if let Some(current) = state.new_line.as_mut() {
                *current += 1;
            }
            StructuredDiffLine {
                kind: StructuredDiffLineKind::Added,
                raw: line.clone(),
                body: line[1..].to_string(),
                line_number,
                language: state.current_language,
                word_segments: None,
            }
        } else if line.starts_with('-') && !line.starts_with("---") {
            let line_number = state.old_line;
            if let Some(current) = state.old_line.as_mut() {
                *current += 1;
            }
            StructuredDiffLine {
                kind: StructuredDiffLineKind::Removed,
                raw: line.clone(),
                body: line[1..].to_string(),
                line_number,
                language: state.current_language,
                word_segments: None,
            }
        } else if let Some(body) = line.strip_prefix(' ') {
            let line_number = state.new_line;
            if let Some(current) = state.old_line.as_mut() {
                *current += 1;
            }
            if let Some(current) = state.new_line.as_mut() {
                *current += 1;
            }
            StructuredDiffLine {
                kind: StructuredDiffLineKind::Context,
                raw: line.clone(),
                body: body.to_string(),
                line_number,
                language: state.current_language,
                word_segments: None,
            }
        } else {
            StructuredDiffLine {
                kind: StructuredDiffLineKind::Plain,
                raw: line.clone(),
                body: line.clone(),
                line_number: None,
                language: CodeLanguage::Diff,
                word_segments: None,
            }
        };

        parsed.push(parsed_line);
    }

    parsed
}

fn apply_word_diff_pairs(lines: &mut [StructuredDiffLine]) {
    let mut index = 0;
    while index < lines.len() {
        if lines[index].kind != StructuredDiffLineKind::Removed {
            index += 1;
            continue;
        }

        let remove_start = index;
        while index < lines.len() && lines[index].kind == StructuredDiffLineKind::Removed {
            index += 1;
        }
        let add_start = index;
        while index < lines.len() && lines[index].kind == StructuredDiffLineKind::Added {
            index += 1;
        }

        let remove_count = add_start - remove_start;
        let add_count = index - add_start;
        let pair_count = remove_count.min(add_count);
        if pair_count == 0 {
            continue;
        }

        for offset in 0..pair_count {
            let old_body = lines[remove_start + offset].body.clone();
            let new_body = lines[add_start + offset].body.clone();
            let diff = TextDiff::from_words(&old_body, &new_body);

            let mut removed_segments = Vec::new();
            let mut added_segments = Vec::new();
            for change in diff.iter_all_changes() {
                match change.tag() {
                    ChangeTag::Equal => {
                        let text = change.to_string();
                        removed_segments.push(StructuredDiffSegment {
                            text: text.clone(),
                            emphasized: false,
                        });
                        added_segments.push(StructuredDiffSegment {
                            text,
                            emphasized: false,
                        });
                    }
                    ChangeTag::Delete => removed_segments.push(StructuredDiffSegment {
                        text: change.to_string(),
                        emphasized: true,
                    }),
                    ChangeTag::Insert => added_segments.push(StructuredDiffSegment {
                        text: change.to_string(),
                        emphasized: true,
                    }),
                }
            }

            lines[remove_start + offset].word_segments = Some(removed_segments);
            lines[add_start + offset].word_segments = Some(added_segments);
        }
    }
}

fn render_structured_diff_line(
    line: &StructuredDiffLine,
    gutter_digits: usize,
    theme: StructuredDiffTheme,
) -> Line<'static> {
    let background = diff_line_background(line.kind, theme);
    let mut spans = vec![Span::styled(
        "  │ ",
        Style::default().fg(theme.border).bg(background),
    )];
    spans.push(render_structured_diff_gutter(line, gutter_digits, theme));

    match line.kind {
        StructuredDiffLineKind::Meta | StructuredDiffLineKind::Hunk | StructuredDiffLineKind::Plain => {
            for token in tokenize_code_line_with_language(&line.raw, CodeLanguage::Diff) {
                spans.push(Span::styled(
                    token.text,
                    Style::default()
                        .fg(structured_diff_token_color(token.kind, CodeLanguage::Diff, theme))
                        .bg(background),
                ));
            }
        }
        StructuredDiffLineKind::Added | StructuredDiffLineKind::Context => {
            if let Some(segments) = &line.word_segments {
                for segment in segments {
                    let segment_bg = if segment.emphasized {
                        theme.add_word_bg
                    } else {
                        background
                    };
                    for token in tokenize_code_line_with_language(&segment.text, line.language) {
                        spans.push(Span::styled(
                            token.text,
                            Style::default()
                                .fg(structured_diff_token_color(token.kind, line.language, theme))
                                .bg(segment_bg),
                        ));
                    }
                }
            } else {
                for token in tokenize_code_line_with_language(&line.body, line.language) {
                    spans.push(Span::styled(
                        token.text,
                        Style::default()
                            .fg(structured_diff_token_color(token.kind, line.language, theme))
                            .bg(background),
                    ));
                }
            }
        }
        StructuredDiffLineKind::Removed => {
            if let Some(segments) = &line.word_segments {
                for segment in segments {
                    spans.push(Span::styled(
                        segment.text.clone(),
                        Style::default()
                            .fg(LIGHT)
                            .bg(if segment.emphasized {
                                theme.remove_word_bg
                            } else {
                                background
                            }),
                    ));
                }
            } else {
                spans.push(Span::styled(
                    line.body.clone(),
                    Style::default().fg(LIGHT).bg(background),
                ));
            }
        }
    }

    if line.body.is_empty() && !matches!(line.kind, StructuredDiffLineKind::Meta | StructuredDiffLineKind::Hunk | StructuredDiffLineKind::Plain) {
        spans.push(Span::styled(" ", Style::default().bg(background)));
    }

    Line::from(spans)
}

fn render_structured_diff_gutter(
    line: &StructuredDiffLine,
    gutter_digits: usize,
    theme: StructuredDiffTheme,
) -> Span<'static> {
    let (marker, marker_color) = match line.kind {
        StructuredDiffLineKind::Meta => ("~", theme.meta),
        StructuredDiffLineKind::Hunk => ("@", theme.hunk),
        StructuredDiffLineKind::Added => ("+", theme.add_marker),
        StructuredDiffLineKind::Removed => ("-", theme.remove_marker),
        StructuredDiffLineKind::Context => (" ", theme.context_marker),
        StructuredDiffLineKind::Plain => (" ", theme.line_no),
    };

    let number = line
        .line_number
        .map(|line_no| line_no.to_string())
        .unwrap_or_default();

    Span::styled(
        format!("{} {:>width$} ", marker, number, width = gutter_digits),
        Style::default()
            .fg(marker_color)
            .bg(diff_line_background(line.kind, theme)),
    )
}

fn diff_line_background(kind: StructuredDiffLineKind, theme: StructuredDiffTheme) -> Color {
    match kind {
        StructuredDiffLineKind::Meta => theme.meta_bg,
        StructuredDiffLineKind::Hunk => theme.hunk_bg,
        StructuredDiffLineKind::Added => theme.add_bg,
        StructuredDiffLineKind::Removed => theme.remove_bg,
        StructuredDiffLineKind::Context | StructuredDiffLineKind::Plain => theme.base_bg,
    }
}

fn structured_diff_token_color(
    kind: CodeTokenKind,
    language: CodeLanguage,
    theme: StructuredDiffTheme,
) -> Color {
    match kind {
        CodeTokenKind::Plain => LIGHT,
        CodeTokenKind::String => Color::Indexed(180),
        CodeTokenKind::Number => Color::Indexed(151),
        CodeTokenKind::Keyword => match language {
            CodeLanguage::Shell => Color::Indexed(79),
            CodeLanguage::Rust => Color::Indexed(111),
            CodeLanguage::Python => Color::Indexed(111),
            _ => Color::Indexed(111),
        },
        CodeTokenKind::Comment => MUTED,
        CodeTokenKind::Decorator => match language {
            CodeLanguage::Rust => Color::Indexed(179),
            _ => Color::Indexed(116),
        },
        CodeTokenKind::Operator => INFO_COLOR,
        CodeTokenKind::Property => Color::Indexed(153),
        CodeTokenKind::Variable => Color::Indexed(215),
        CodeTokenKind::DiffAdded => theme.add_marker,
        CodeTokenKind::DiffRemoved => theme.remove_marker,
        CodeTokenKind::DiffHunk => theme.hunk,
        CodeTokenKind::DiffMeta => theme.meta,
        CodeTokenKind::DiffFile => theme.file,
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

fn is_diff_meta_line(line: &str) -> bool {
    matches!(
        line,
        l if l.starts_with("diff ")
            || l.starts_with("index ")
            || l.starts_with("+++ ")
            || l.starts_with("--- ")
            || l.starts_with("rename from ")
            || l.starts_with("rename to ")
            || l.starts_with("copy from ")
            || l.starts_with("copy to ")
    )
}

fn diff_language_path(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix("diff --git ") {
        let mut parts = rest.split_whitespace();
        let _old = parts.next()?;
        let new = parts.next()?;
        return normalize_diff_path(new).map(ToString::to_string);
    }

    for prefix in ["+++ ", "--- ", "rename to ", "rename from ", "copy to ", "copy from "] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return normalize_diff_path(rest.trim()).map(ToString::to_string);
        }
    }

    None
}

fn normalize_diff_path(path: &str) -> Option<&str> {
    let trimmed = path.trim().trim_matches('"');
    if trimmed.is_empty() || trimmed == "/dev/null" {
        return None;
    }
    Some(
        trimmed
            .strip_prefix("a/")
            .or_else(|| trimmed.strip_prefix("b/"))
            .unwrap_or(trimmed),
    )
}

fn parse_diff_hunk_header(line: &str) -> Option<(usize, usize)> {
    let middle = line.strip_prefix("@@ ")?.split(" @@").next()?;
    let mut parts = middle.split_whitespace();
    let old = parts.next()?;
    let new = parts.next()?;
    Some((parse_hunk_start(old)?, parse_hunk_start(new)?))
}

fn parse_hunk_start(part: &str) -> Option<usize> {
    let digits = part
        .strip_prefix(['-', '+'])?
        .split(',')
        .next()?;
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::render_structured_diff_block;
    use ratatui::style::Color;

    #[test]
    fn structured_diff_emphasizes_word_level_changes() {
        let lines = render_structured_diff_block(&[
            "diff --git a/src/main.rs b/src/main.rs".to_string(),
            "@@ -1,1 +1,1 @@".to_string(),
            "-let old_value = 1;".to_string(),
            "+let new_value = 1;".to_string(),
        ]);

        let removed_line = lines
            .iter()
            .find(|line| line.to_string().contains("old_value"))
            .unwrap();
        assert!(removed_line
            .spans
            .iter()
            .any(|span| span.content.contains("old_value")
                && span.style.bg == Some(Color::Indexed(88))));

        let added_line = lines
            .iter()
            .find(|line| line.to_string().contains("new_value"))
            .unwrap();
        assert!(added_line
            .spans
            .iter()
            .any(|span| span.content.contains("new_value")
                && span.style.bg == Some(Color::Indexed(28))));
    }
}
