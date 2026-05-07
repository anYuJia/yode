use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{LazyLock, Mutex};

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use regex::Regex;

use super::chat::{CODE_BG, DIM, INLINE_CODE_BG, WHITE, YELLOW};
use super::chat_layout::{manual_wrap, osc8_close_sequence, visible_text_width};
use super::highlighted_code::render_highlighted_code_block;
use super::palette::{BORDER_MUTED, INFO_COLOR, LIGHT, MUTED, PANEL_ACCENT};
use super::structured_diff::render_structured_diff_block;
use crate::app::rendering::{
    parse_code_language, tokenize_code_line_with_language, CodeLanguage, CodeTokenKind,
};

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct MarkdownRenderOptions {
    pub max_width: Option<usize>,
    pub enable_hyperlinks: bool,
}

const MARKDOWN_BLOCK_CACHE_MAX: usize = 500;

#[derive(Default)]
struct MarkdownBlockCache {
    entries: HashMap<u64, Vec<MarkdownBlock>>,
    order: VecDeque<u64>,
}

static MARKDOWN_BLOCK_CACHE: LazyLock<Mutex<MarkdownBlockCache>> =
    LazyLock::new(|| Mutex::new(MarkdownBlockCache::default()));

static MD_SYNTAX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[#*`|>\-_~\[\]]|\n\n|^\d+\. |\n\d+\. |^• |^◦ |^▪ |\n• |\n◦ |\n▪ |│|┼").unwrap()
});
static ISSUE_REF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|[^\w./-])([A-Za-z0-9][\w-]*/[A-Za-z0-9][\w.-]*)#(\d+)\b").unwrap()
});
static URL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"https?://[^\s<>"']+"#).unwrap());

const TABLE_SAFETY_MARGIN: usize = 4;
const TABLE_MAX_ROW_LINES: usize = 1;

pub(super) fn render_markdown_with_options(
    text: &str,
    default_fg: Option<Color>,
    options: MarkdownRenderOptions,
) -> Vec<Line<'static>> {
    if !has_markdown_syntax(text) {
        return render_plain_text_lines(text, default_fg, options);
    }

    let mut lines = Vec::new();
    let blocks = cached_markdown_blocks(text);
    render_block_sequence(&mut lines, &blocks, default_fg, 0, true, &options);

    lines
}

pub(super) fn render_markdown_impl(text: &str, default_fg: Option<Color>) -> Vec<Line<'static>> {
    render_markdown_with_options(text, default_fg, MarkdownRenderOptions::default())
}

pub(crate) fn streaming_markdown_advance_stable_boundary(
    text: &str,
    current_stable_len: usize,
) -> usize {
    let stable_len = current_stable_len.min(text.len());
    stable_len + stable_boundary_from_complete_lines(&text[stable_len..])
}

pub(crate) fn render_markdown_ansi_with_options(
    text: &str,
    default_fg: Option<Color>,
    options: MarkdownRenderOptions,
) -> Vec<String> {
    render_markdown_with_options(text, default_fg, options)
        .into_iter()
        .map(|line| line_to_ansi_string(&line))
        .collect()
}

fn has_markdown_syntax(text: &str) -> bool {
    MD_SYNTAX_RE.is_match(text)
}

fn cached_markdown_blocks(text: &str) -> Vec<MarkdownBlock> {
    let key = hash_markdown_text(text);
    if let Ok(mut cache) = MARKDOWN_BLOCK_CACHE.lock() {
        if let Some(blocks) = cache.entries.get(&key).cloned() {
            cache.order.retain(|existing| *existing != key);
            cache.order.push_back(key);
            return blocks;
        }
    }

    let blocks = parse_markdown_blocks(text);

    if let Ok(mut cache) = MARKDOWN_BLOCK_CACHE.lock() {
        if cache.entries.len() >= MARKDOWN_BLOCK_CACHE_MAX {
            if let Some(oldest) = cache.order.pop_front() {
                cache.entries.remove(&oldest);
            }
        }
        cache.order.retain(|existing| *existing != key);
        cache.order.push_back(key);
        cache.entries.insert(key, blocks.clone());
    }

    blocks
}

fn hash_markdown_text(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn render_plain_text_lines(
    text: &str,
    default_fg: Option<Color>,
    options: MarkdownRenderOptions,
) -> Vec<Line<'static>> {
    let style = default_fg
        .map(|fg| Style::default().fg(fg))
        .unwrap_or_default();
    let mut lines = if text.is_empty() {
        vec![Line::from("")]
    } else {
        text.split('\n')
            .map(|line| {
                if line.trim().is_empty() {
                    Line::from("")
                } else {
                    let mut spans = Vec::new();
                    if options.enable_hyperlinks {
                        append_text_with_links(&mut spans, line, style);
                    } else {
                        spans.push(Span::styled(line.to_string(), style));
                    }
                    Line::from(spans)
                }
            })
            .collect::<Vec<_>>()
    };
    normalize_blank_lines(&mut lines);
    if let Some(max_width) = options.max_width {
        manual_wrap(lines, max_width as u16)
    } else {
        lines
    }
}

fn stable_boundary_from_complete_lines(text: &str) -> usize {
    let mut lines = Vec::new();
    let mut offset = 0usize;
    for segment in text.split_inclusive('\n') {
        if !segment.ends_with('\n') {
            break;
        }
        let line_end = offset + segment.len();
        lines.push((
            offset,
            line_end,
            segment.trim_end_matches('\n').trim().to_string(),
        ));
        offset = line_end;
    }

    let mut in_code_fence = false;
    let mut last_safe = 0usize;
    let mut index = 0usize;

    while index < lines.len() {
        let line_end = lines[index].1;
        let trimmed = lines[index].2.as_str();

        if trimmed.starts_with("```") {
            if in_code_fence {
                in_code_fence = false;
                last_safe = line_end;
            } else {
                in_code_fence = true;
            }
            index += 1;
            continue;
        }

        if in_code_fence {
            index += 1;
            continue;
        }

        if trimmed.is_empty() {
            last_safe = line_end;
            index += 1;
            continue;
        }

        if looks_like_heading_candidate(trimmed) {
            let next = lines.get(index + 1).map(|line| line.2.as_str());
            if next.is_none() {
                break;
            }
            if next.is_some_and(looks_like_heading_followup) {
                last_safe = line_end;
                index += 1;
                continue;
            }
            break;
        }

        if is_streaming_table_line(trimmed) {
            let mut table_end = index;
            while table_end < lines.len() && is_streaming_table_line(lines[table_end].2.as_str()) {
                table_end += 1;
            }
            if table_end == lines.len() {
                break;
            }
            if lines[table_end].2.is_empty() {
                last_safe = lines[table_end].1;
                index = table_end + 1;
            } else {
                last_safe = lines[table_end - 1].1;
                index = table_end;
            }
            continue;
        }

        if is_streaming_list_item_line(trimmed) {
            let mut list_end = index;
            let mut ended_with_blank = false;
            while list_end < lines.len() {
                let current = lines[list_end].2.as_str();
                if current.is_empty() {
                    ended_with_blank = true;
                    list_end += 1;
                    continue;
                }
                if is_streaming_list_item_line(current) {
                    ended_with_blank = false;
                    list_end += 1;
                    continue;
                }
                break;
            }
            if list_end == lines.len() && !ended_with_blank {
                break;
            }
            let stable_index = if list_end > index && lines[list_end.saturating_sub(1)].2.is_empty()
            {
                list_end - 1
            } else {
                list_end.saturating_sub(1)
            };
            last_safe = lines[stable_index].1;
            index = list_end;
            continue;
        }

        last_safe = line_end;
        index += 1;
    }

    last_safe
}

fn is_streaming_table_line(trimmed: &str) -> bool {
    is_markdown_table_row(trimmed)
        || is_markdown_table_separator(trimmed)
        || normalize_unicode_table_row(trimmed).is_some()
        || normalize_unicode_table_separator(trimmed).is_some()
        || normalize_ascii_pipe_table_row(trimmed).is_some()
}

fn is_streaming_list_item_line(trimmed: &str) -> bool {
    trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("• ")
        || trimmed.starts_with("◦ ")
        || trimmed.starts_with("▪ ")
        || trimmed.split_once(". ").is_some_and(|(prefix, _)| {
            !prefix.is_empty() && prefix.chars().all(|ch| ch.is_ascii_digit())
        })
}

#[derive(Debug, Clone)]
enum MarkdownBlock {
    Heading {
        level: usize,
        content: Vec<InlineNode>,
    },
    Rule,
    Paragraph {
        content: Vec<InlineNode>,
    },
    Quote {
        blocks: Vec<MarkdownBlock>,
    },
    List {
        ordered_start: Option<u64>,
        items: Vec<ListItem>,
    },
    Table {
        rows: Vec<Vec<TableCell>>,
    },
    CodeFence {
        label: Option<String>,
        language: CodeLanguage,
        lines: Vec<String>,
    },
}

#[derive(Debug, Clone)]
struct ListItem {
    task_state: Option<bool>,
    blocks: Vec<MarkdownBlock>,
}

#[derive(Debug, Clone)]
struct TableCell {
    content: Vec<InlineNode>,
}

#[derive(Debug, Clone)]
enum InlineNode {
    Text(String),
    Strong(Vec<InlineNode>),
    Emphasis(Vec<InlineNode>),
    Code(String),
    Link { text: Vec<InlineNode>, url: String },
    SoftBreak,
    HardBreak,
}

fn parse_markdown_blocks(text: &str) -> Vec<MarkdownBlock> {
    let normalized = normalize_markdown_input(text);
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let events: Vec<Event<'_>> = Parser::new_ext(&normalized, options).collect();
    let mut index = 0;
    parse_blocks_until(&events, &mut index, None)
}

static COMPOUND_TABLE_ROW_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|\s+\|").unwrap());

fn normalize_markdown_input(text: &str) -> String {
    promote_heading_lines(collapse_list_blank_lines(normalize_structural_lines(text))).join("\n")
}

fn normalize_structural_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut in_code_fence = false;

    for raw_line in text.lines() {
        if raw_line.trim().is_empty() {
            lines.push(String::new());
            continue;
        }

        let line = strip_structural_indent(raw_line);
        let trimmed = line.trim().to_string();
        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
            lines.push(line);
            continue;
        }

        if in_code_fence {
            lines.push(line);
            continue;
        }

        if try_join_table_continuation(&mut lines, &trimmed) {
            continue;
        }

        if let Some(list_line) = normalize_unicode_bullet_line(&trimmed) {
            lines.push(list_line);
            continue;
        }

        if let Some(separator) = normalize_unicode_table_separator(&trimmed) {
            lines.push(separator);
            continue;
        }

        if let Some(row) = normalize_unicode_table_row(&trimmed) {
            lines.push(row);
            continue;
        }

        if let Some(row) = normalize_ascii_pipe_table_row(&trimmed) {
            lines.extend(expand_compound_table_rows(&row));
            continue;
        }

        lines.extend(expand_compound_table_rows(&line));
    }

    insert_missing_table_separator_lines(lines)
}

fn strip_structural_indent(raw_line: &str) -> String {
    let indent = raw_line.chars().take_while(|ch| *ch == ' ').count();
    let trimmed = raw_line.trim_start_matches(' ');
    if indent > 0 && looks_like_list_line(trimmed) {
        return raw_line.to_string();
    }
    if indent >= 2 && looks_like_structural_line(trimmed) {
        trimmed.to_string()
    } else {
        raw_line.to_string()
    }
}

fn looks_like_list_line(trimmed: &str) -> bool {
    trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("+ ")
        || trimmed.starts_with("• ")
        || trimmed.starts_with("◦ ")
        || trimmed.starts_with("▪ ")
        || trimmed
            .split_once(". ")
            .is_some_and(|(number, _)| number.chars().all(|ch| ch.is_ascii_digit()))
}

fn normalize_unicode_bullet_line(trimmed: &str) -> Option<String> {
    for marker in ["• ", "◦ ", "▪ "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return Some(format!("- {}", rest.trim_start()));
        }
    }
    None
}

fn try_join_table_continuation(lines: &mut Vec<String>, trimmed: &str) -> bool {
    if trimmed.is_empty()
        || trimmed.starts_with('|')
        || trimmed.contains('│')
        || !trimmed.ends_with('|')
    {
        return false;
    }

    let Some(last_nonempty) = lines.iter().rposition(|line| !line.trim().is_empty()) else {
        return false;
    };
    if !lines[last_nonempty].trim_start().starts_with('|') {
        return false;
    }

    lines.truncate(last_nonempty + 1);
    let continuation = trimmed.trim_end_matches('|').trim();
    if continuation.is_empty() {
        return false;
    }

    let previous = lines.last_mut().unwrap();
    let cleaned_previous = previous
        .trim_end_matches(|ch: char| ch.is_whitespace())
        .trim_end_matches('|')
        .trim_end()
        .to_string();
    *previous = format!("{} {} |", cleaned_previous, continuation);
    true
}

fn looks_like_structural_line(trimmed: &str) -> bool {
    normalize_unicode_table_separator(trimmed).is_some()
        || normalize_unicode_table_row(trimmed).is_some()
        || normalize_ascii_pipe_table_row(trimmed).is_some()
        || trimmed.starts_with('|')
        || trimmed.starts_with("```")
        || trimmed.starts_with("• ")
        || trimmed.starts_with("◦ ")
        || trimmed.starts_with("▪ ")
        || trimmed.starts_with("- [")
        || trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("> ")
        || trimmed.find(". ").is_some_and(|dot| {
            dot > 0 && dot <= 3 && trimmed[..dot].chars().all(|c| c.is_ascii_digit())
        })
        || trimmed.chars().all(|ch| {
            ch == '─' || ch == '┼' || ch == '-' || ch == '*' || ch == '_' || ch.is_whitespace()
        })
        || looks_like_heading_candidate(trimmed)
}

fn normalize_unicode_table_row(trimmed: &str) -> Option<String> {
    if !trimmed.contains('│') || trimmed.contains("```") {
        return None;
    }
    let cells = trimmed
        .split('│')
        .map(str::trim)
        .filter(|cell| !cell.is_empty())
        .collect::<Vec<_>>();
    if cells.len() < 2 {
        return None;
    }
    Some(format!("| {} |", cells.join(" | ")))
}

fn normalize_unicode_table_separator(trimmed: &str) -> Option<String> {
    if !trimmed.contains('┼')
        || !trimmed
            .chars()
            .all(|ch| ch == '─' || ch == '┼' || ch.is_whitespace())
    {
        return None;
    }
    let cols = trimmed.split('┼').count().max(2);
    Some(format!(
        "| {} |",
        std::iter::repeat_n("---", cols)
            .collect::<Vec<_>>()
            .join(" | ")
    ))
}

fn normalize_ascii_pipe_table_row(trimmed: &str) -> Option<String> {
    if trimmed.starts_with('|')
        || trimmed.contains("```")
        || trimmed.contains("http://")
        || trimmed.contains("https://")
        || trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("> ")
        || trimmed.starts_with("```")
    {
        return None;
    }

    let pipe_count = trimmed.matches('|').count();
    if pipe_count < 2 {
        return None;
    }

    let cells = trimmed
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .filter(|cell| !cell.is_empty())
        .collect::<Vec<_>>();
    if cells.len() < 2 {
        return None;
    }

    Some(format!("| {} |", cells.join(" | ")))
}

fn insert_missing_table_separator_lines(lines: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::with_capacity(lines.len());
    let mut index = 0usize;

    while index < lines.len() {
        if !is_markdown_table_row(&lines[index]) {
            normalized.push(lines[index].clone());
            index += 1;
            continue;
        }

        let start = index;
        while index < lines.len() && is_markdown_table_row(&lines[index]) {
            index += 1;
        }
        let block = &lines[start..index];

        if block.len() >= 2 && !is_markdown_table_separator(&block[1]) {
            normalized.push(block[0].clone());
            normalized.push(markdown_table_separator_for_row(&block[0]));
            normalized.extend(block.iter().skip(1).cloned());
        } else {
            normalized.extend(block.iter().cloned());
        }
    }

    normalized
}

fn collapse_list_blank_lines(lines: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        if !line.trim().is_empty() {
            normalized.push(line.clone());
            continue;
        }

        let previous = normalized.last().map(|line| line.trim()).unwrap_or("");
        let next = lines
            .iter()
            .skip(index + 1)
            .find(|candidate| !candidate.trim().is_empty())
            .map(|candidate| candidate.trim())
            .unwrap_or("");

        if is_streaming_list_item_line(previous) && is_streaming_list_item_line(next) {
            continue;
        }

        normalized.push(line.clone());
    }
    normalized
}

fn is_markdown_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.matches('|').count() >= 2
}

fn is_markdown_table_separator(line: &str) -> bool {
    let trimmed = line.trim().trim_matches('|');
    !trimmed.is_empty()
        && trimmed.split('|').map(str::trim).all(|cell| {
            !cell.is_empty() && cell.chars().all(|ch| ch == '-' || ch == ':' || ch == ' ')
        })
}

fn markdown_table_separator_for_row(line: &str) -> String {
    let cols = line
        .trim()
        .trim_matches('|')
        .split('|')
        .filter(|cell| !cell.trim().is_empty())
        .count()
        .max(2);
    format!(
        "| {} |",
        std::iter::repeat_n("---", cols)
            .collect::<Vec<_>>()
            .join(" | ")
    )
}

fn expand_compound_table_rows(raw_line: &str) -> Vec<String> {
    let trimmed = raw_line.trim_start();
    if !trimmed.starts_with('|') || trimmed.matches('|').count() < 8 {
        return vec![raw_line.to_string()];
    }

    COMPOUND_TABLE_ROW_RE
        .replace_all(trimmed, "|\n|")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn promote_heading_lines(lines: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::with_capacity(lines.len());
    let mut in_code_fence = false;

    for index in 0..lines.len() {
        let line = &lines[index];
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
            normalized.push(line.clone());
            continue;
        }
        if in_code_fence {
            normalized.push(line.clone());
            continue;
        }
        if !looks_like_heading_candidate(trimmed) {
            normalized.push(line.clone());
            continue;
        }

        let next_nonempty = lines
            .iter()
            .skip(index + 1)
            .find(|candidate| !candidate.trim().is_empty())
            .map(|candidate| candidate.trim().to_string());

        let Some(level) = classify_promoted_heading_level(trimmed, next_nonempty.as_deref()) else {
            normalized.push(line.clone());
            continue;
        };
        let marker = "#".repeat(level);
        normalized.push(format!("{} {}", marker, trimmed));
    }

    normalized
}

fn classify_promoted_heading_level(trimmed: &str, next: Option<&str>) -> Option<usize> {
    if !next.is_some_and(looks_like_heading_followup) {
        return None;
    }
    if is_chinese_section_heading(trimmed) {
        return Some(1);
    }
    if is_priority_heading(trimmed) {
        return Some(2);
    }
    if looks_like_numbered_section_heading(trimmed, next) {
        return Some(3);
    }
    if looks_like_heading_candidate(trimmed) {
        return Some(2);
    }
    None
}

fn looks_like_heading_candidate(trimmed: &str) -> bool {
    let has_cjk = trimmed
        .chars()
        .any(|ch| ('\u{4E00}'..='\u{9FFF}').contains(&ch));
    let has_upper_ascii = trimmed.chars().any(|ch| ch.is_ascii_uppercase());
    !trimmed.is_empty()
        && trimmed.chars().count() <= 40
        && (has_cjk || has_upper_ascii || trimmed.starts_with('P'))
        && !trimmed.starts_with('#')
        && !trimmed.starts_with("```")
        && !trimmed.starts_with('>')
        && !trimmed.starts_with("- ")
        && !trimmed.starts_with("* ")
        && !trimmed.starts_with("| ")
        && !trimmed.starts_with('|')
        && !trimmed.contains('│')
        && !trimmed.contains("http://")
        && !trimmed.contains("https://")
        && !trimmed.ends_with('。')
        && !trimmed.ends_with('！')
        && !trimmed.ends_with('？')
        && !trimmed.ends_with('.')
        && !trimmed.ends_with(':')
        && !trimmed.ends_with('：')
}

fn looks_like_heading_followup(next: &str) -> bool {
    next.starts_with('|')
        || next.starts_with('P')
        || next.starts_with("- ")
        || next.starts_with("* ")
        || next.starts_with("• ")
        || next.starts_with("◦ ")
        || next.starts_with("▪ ")
        || next.starts_with("P0")
        || next.starts_with("P1")
        || next.starts_with("P2")
        || next.starts_with("1. ")
        || looks_like_heading_candidate(next)
}

fn is_chinese_section_heading(trimmed: &str) -> bool {
    let Some((prefix, rest)) = trimmed.split_once('、') else {
        return false;
    };
    !rest.trim().is_empty()
        && prefix.chars().all(|ch| {
            matches!(
                ch,
                '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十'
            )
        })
}

fn is_priority_heading(trimmed: &str) -> bool {
    trimmed.starts_with('P')
        && trimmed.chars().nth(1).is_some_and(|ch| ch.is_ascii_digit())
        && (trimmed.contains("核心")
            || trimmed.contains("缺失")
            || trimmed.contains("增强")
            || trimmed.contains("优先"))
}

fn looks_like_numbered_section_heading(trimmed: &str, next: Option<&str>) -> bool {
    let Some((prefix, rest)) = trimmed.split_once(". ") else {
        return false;
    };
    if prefix.is_empty()
        || prefix.len() > 2
        || !prefix.chars().all(|ch| ch.is_ascii_digit())
        || rest.is_empty()
        || rest.chars().count() > 64
    {
        return false;
    }

    let has_cjk = rest
        .chars()
        .any(|ch| ('\u{4E00}'..='\u{9FFF}').contains(&ch));
    let has_upper_ascii = rest.chars().any(|ch| ch.is_ascii_uppercase());
    if !has_cjk && !has_upper_ascii {
        return false;
    }

    let Some(next) = next else {
        return false;
    };
    looks_like_numbered_section_followup(next)
}

fn looks_like_numbered_section_followup(next: &str) -> bool {
    let next = next.trim();
    if next.is_empty() {
        return false;
    }
    if next.starts_with("- ")
        || next.starts_with("* ")
        || next.starts_with("• ")
        || next.starts_with("◦ ")
        || next.starts_with("▪ ")
        || next.starts_with('|')
        || next.contains('│')
    {
        return true;
    }

    if let Some((prefix, _)) = next.split_once(". ") {
        if prefix.chars().all(|ch| ch.is_ascii_digit()) {
            return false;
        }
    }

    true
}

#[derive(Clone, Copy)]
enum ContainerEnd {
    BlockQuote,
}

#[derive(Clone, Copy)]
enum InlineEnd {
    Paragraph,
    Heading,
    TableCell,
    Strong,
    Emphasis,
    Link,
    Image,
}

fn parse_blocks_until(
    events: &[Event<'_>],
    index: &mut usize,
    end: Option<ContainerEnd>,
) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();

    while *index < events.len() {
        match &events[*index] {
            Event::End(tag_end)
                if end.is_some_and(|container| is_container_end(tag_end, container)) =>
            {
                *index += 1;
                break;
            }
            Event::Rule => {
                blocks.push(MarkdownBlock::Rule);
                *index += 1;
            }
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    *index += 1;
                    blocks.push(MarkdownBlock::Paragraph {
                        content: parse_inline_nodes_until(events, index, InlineEnd::Paragraph),
                    });
                }
                Tag::Heading { level, .. } => {
                    let level = heading_level_to_usize(*level);
                    *index += 1;
                    let content = parse_inline_nodes_until(events, index, InlineEnd::Heading);
                    blocks.push(MarkdownBlock::Heading { level, content });
                }
                Tag::BlockQuote(_) => {
                    *index += 1;
                    blocks.push(MarkdownBlock::Quote {
                        blocks: parse_blocks_until(events, index, Some(ContainerEnd::BlockQuote)),
                    });
                }
                Tag::List(ordered_start) => {
                    *index += 1;
                    blocks.push(parse_list_block(events, index, *ordered_start));
                }
                Tag::CodeBlock(kind) => {
                    let block = parse_code_fence_block(events, index, kind);
                    blocks.push(block);
                }
                Tag::Table(_) => {
                    *index += 1;
                    blocks.push(parse_table_block(events, index));
                }
                Tag::HtmlBlock | Tag::FootnoteDefinition(_) | Tag::DefinitionList => {
                    *index += 1;
                }
                _ => {
                    *index += 1;
                }
            },
            _ => {
                *index += 1;
            }
        }
    }

    blocks
}

fn parse_list_block(
    events: &[Event<'_>],
    index: &mut usize,
    ordered_start: Option<u64>,
) -> MarkdownBlock {
    let mut items = Vec::new();

    while *index < events.len() {
        match &events[*index] {
            Event::Start(Tag::Item) => {
                *index += 1;
                let task_state = match events.get(*index) {
                    Some(Event::TaskListMarker(checked)) => {
                        *index += 1;
                        Some(*checked)
                    }
                    _ => None,
                };
                items.push(ListItem {
                    task_state,
                    blocks: parse_item_blocks(events, index),
                });
            }
            Event::End(tag_end) if is_list_end(tag_end) => {
                *index += 1;
                break;
            }
            _ => {
                *index += 1;
            }
        }
    }

    MarkdownBlock::List {
        ordered_start,
        items,
    }
}

fn parse_item_blocks(events: &[Event<'_>], index: &mut usize) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();

    while *index < events.len() {
        match &events[*index] {
            Event::End(TagEnd::Item) => {
                *index += 1;
                break;
            }
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    *index += 1;
                    blocks.push(MarkdownBlock::Paragraph {
                        content: parse_inline_nodes_until(events, index, InlineEnd::Paragraph),
                    });
                }
                Tag::Heading { level, .. } => {
                    let level = heading_level_to_usize(*level);
                    *index += 1;
                    let content = parse_inline_nodes_until(events, index, InlineEnd::Heading);
                    blocks.push(MarkdownBlock::Heading { level, content });
                }
                Tag::BlockQuote(_) => {
                    *index += 1;
                    blocks.push(MarkdownBlock::Quote {
                        blocks: parse_blocks_until(events, index, Some(ContainerEnd::BlockQuote)),
                    });
                }
                Tag::List(ordered_start) => {
                    *index += 1;
                    blocks.push(parse_list_block(events, index, *ordered_start));
                }
                Tag::CodeBlock(kind) => {
                    blocks.push(parse_code_fence_block(events, index, kind));
                }
                Tag::Table(_) => {
                    *index += 1;
                    blocks.push(parse_table_block(events, index));
                }
                _ if is_inline_start_tag(tag) => {
                    blocks.push(MarkdownBlock::Paragraph {
                        content: parse_inline_nodes_until_item_boundary(events, index),
                    });
                }
                _ => {
                    *index += 1;
                }
            },
            event if is_inline_event(event) => {
                blocks.push(MarkdownBlock::Paragraph {
                    content: parse_inline_nodes_until_item_boundary(events, index),
                });
            }
            _ => {
                *index += 1;
            }
        }
    }

    blocks
}

fn parse_code_fence_block(
    events: &[Event<'_>],
    index: &mut usize,
    kind: &CodeBlockKind<'_>,
) -> MarkdownBlock {
    let raw_label = match kind {
        CodeBlockKind::Fenced(info) => info.trim().to_string(),
        CodeBlockKind::Indented => String::new(),
    };
    let language = parse_code_language(&raw_label);
    let label = (!raw_label.is_empty()).then_some(raw_label);

    *index += 1;
    let mut content = String::new();
    while *index < events.len() {
        match &events[*index] {
            Event::End(TagEnd::CodeBlock) => {
                *index += 1;
                break;
            }
            Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => {
                content.push_str(text);
                *index += 1;
            }
            Event::SoftBreak | Event::HardBreak => {
                content.push('\n');
                *index += 1;
            }
            _ => {
                *index += 1;
            }
        }
    }

    MarkdownBlock::CodeFence {
        label,
        language,
        lines: if content.is_empty() {
            Vec::new()
        } else {
            content.split('\n').map(str::to_string).collect()
        },
    }
}

fn parse_table_block(events: &[Event<'_>], index: &mut usize) -> MarkdownBlock {
    let mut rows = Vec::new();

    while *index < events.len() {
        match &events[*index] {
            Event::Start(Tag::TableHead) => {
                *index += 1;
                rows.push(parse_table_row(events, index, true));
            }
            Event::Start(Tag::TableRow) => {
                *index += 1;
                rows.push(parse_table_row(events, index, false));
            }
            Event::End(TagEnd::Table) => {
                *index += 1;
                break;
            }
            _ => {
                *index += 1;
            }
        }
    }

    MarkdownBlock::Table { rows }
}

fn parse_table_row(events: &[Event<'_>], index: &mut usize, is_header: bool) -> Vec<TableCell> {
    let mut row = Vec::new();

    while *index < events.len() {
        match &events[*index] {
            Event::Start(Tag::TableCell) => {
                *index += 1;
                row.push(TableCell {
                    content: parse_inline_nodes_until(events, index, InlineEnd::TableCell),
                });
            }
            Event::End(TagEnd::TableHead) if is_header => {
                *index += 1;
                break;
            }
            Event::End(TagEnd::TableRow) if !is_header => {
                *index += 1;
                break;
            }
            _ => {
                *index += 1;
            }
        }
    }

    row
}

fn parse_inline_nodes_until(
    events: &[Event<'_>],
    index: &mut usize,
    end: InlineEnd,
) -> Vec<InlineNode> {
    let mut nodes = Vec::new();

    while *index < events.len() {
        match &events[*index] {
            Event::End(tag_end) if is_inline_end(tag_end, end) => {
                *index += 1;
                break;
            }
            Event::Start(Tag::Strong) => {
                *index += 1;
                nodes.push(InlineNode::Strong(parse_inline_nodes_until(
                    events,
                    index,
                    InlineEnd::Strong,
                )));
            }
            Event::Start(Tag::Emphasis) => {
                *index += 1;
                nodes.push(InlineNode::Emphasis(parse_inline_nodes_until(
                    events,
                    index,
                    InlineEnd::Emphasis,
                )));
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                let url = dest_url.to_string();
                *index += 1;
                nodes.push(InlineNode::Link {
                    text: parse_inline_nodes_until(events, index, InlineEnd::Link),
                    url,
                });
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                let url = dest_url.to_string();
                *index += 1;
                let alt = parse_inline_nodes_until(events, index, InlineEnd::Image);
                let text = inline_nodes_to_plain_text(&alt);
                nodes.push(InlineNode::Text(if text.is_empty() { url } else { text }));
            }
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                nodes.push(InlineNode::Text(text.to_string()));
                *index += 1;
            }
            Event::Code(text) => {
                nodes.push(InlineNode::Code(text.to_string()));
                *index += 1;
            }
            Event::SoftBreak => {
                nodes.push(InlineNode::SoftBreak);
                *index += 1;
            }
            Event::HardBreak => {
                nodes.push(InlineNode::HardBreak);
                *index += 1;
            }
            Event::FootnoteReference(text) => {
                nodes.push(InlineNode::Text(text.to_string()));
                *index += 1;
            }
            _ => {
                *index += 1;
            }
        }
    }

    nodes
}

fn parse_inline_nodes_until_item_boundary(
    events: &[Event<'_>],
    index: &mut usize,
) -> Vec<InlineNode> {
    let mut nodes = Vec::new();

    while *index < events.len() {
        match &events[*index] {
            Event::End(TagEnd::Item)
            | Event::Start(Tag::List(_))
            | Event::Start(Tag::BlockQuote(_))
            | Event::Start(Tag::CodeBlock(_))
            | Event::Start(Tag::Table(_))
            | Event::Start(Tag::Heading { .. })
            | Event::Start(Tag::Paragraph) => {
                break;
            }
            Event::Start(Tag::Strong) => {
                *index += 1;
                nodes.push(InlineNode::Strong(parse_inline_nodes_until(
                    events,
                    index,
                    InlineEnd::Strong,
                )));
            }
            Event::Start(Tag::Emphasis) => {
                *index += 1;
                nodes.push(InlineNode::Emphasis(parse_inline_nodes_until(
                    events,
                    index,
                    InlineEnd::Emphasis,
                )));
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                let url = dest_url.to_string();
                *index += 1;
                nodes.push(InlineNode::Link {
                    text: parse_inline_nodes_until(events, index, InlineEnd::Link),
                    url,
                });
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                let url = dest_url.to_string();
                *index += 1;
                let alt = parse_inline_nodes_until(events, index, InlineEnd::Image);
                let text = inline_nodes_to_plain_text(&alt);
                nodes.push(InlineNode::Text(if text.is_empty() { url } else { text }));
            }
            Event::Text(text) | Event::Html(text) | Event::InlineHtml(text) => {
                nodes.push(InlineNode::Text(text.to_string()));
                *index += 1;
            }
            Event::Code(text) => {
                nodes.push(InlineNode::Code(text.to_string()));
                *index += 1;
            }
            Event::SoftBreak => {
                nodes.push(InlineNode::SoftBreak);
                *index += 1;
            }
            Event::HardBreak => {
                nodes.push(InlineNode::HardBreak);
                *index += 1;
            }
            Event::FootnoteReference(text) => {
                nodes.push(InlineNode::Text(text.to_string()));
                *index += 1;
            }
            _ => {
                *index += 1;
            }
        }
    }

    nodes
}

fn render_block_sequence(
    lines: &mut Vec<Line<'static>>,
    blocks: &[MarkdownBlock],
    default_fg: Option<Color>,
    list_depth: usize,
    with_gap: bool,
    options: &MarkdownRenderOptions,
) {
    for (index, block) in blocks.iter().enumerate() {
        let spacing = block_spacing(block);
        if with_gap && index > 0 && spacing.before {
            ensure_blank_line(lines);
        }
        render_markdown_block(lines, block, default_fg, list_depth, options);
        if with_gap && spacing.after {
            ensure_blank_line(lines);
        }
    }
    normalize_blank_lines(lines);
}

fn render_markdown_block(
    lines: &mut Vec<Line<'static>>,
    block: &MarkdownBlock,
    default_fg: Option<Color>,
    list_depth: usize,
    options: &MarkdownRenderOptions,
) {
    match block {
        MarkdownBlock::Heading { level, content } => {
            let style = match level {
                1 => Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD | Modifier::ITALIC | Modifier::UNDERLINED),
                2 => Style::default()
                    .fg(Color::Indexed(51))
                    .add_modifier(Modifier::BOLD),
                _ => Style::default()
                    .fg(Color::LightBlue)
                    .add_modifier(Modifier::BOLD),
            };
            let rendered = render_inline_nodes_as_lines_with_style(content, style, options);
            for line in rendered {
                lines.push(prepend_prefix(line, String::new(), style));
            }
        }
        MarkdownBlock::Rule => {
            lines.push(Line::from(Span::styled(
                "────────────────────────────────────────",
                Style::default().fg(DIM),
            )));
        }
        MarkdownBlock::Paragraph { content } => {
            lines.extend(render_inline_nodes_as_lines(content, default_fg, options));
        }
        MarkdownBlock::Quote { blocks } => {
            let mut inner = Vec::new();
            render_block_sequence(&mut inner, blocks, default_fg, list_depth, true, options);
            for line in inner {
                if is_blank_line(&line) {
                    lines.push(line);
                } else {
                    lines.push(prepend_prefix(
                        add_modifier_to_line(line, Modifier::ITALIC),
                        "▎ ".to_string(),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
        }
        MarkdownBlock::List {
            ordered_start,
            items,
        } => {
            render_list_block(
                lines,
                *ordered_start,
                items,
                default_fg,
                list_depth,
                options,
            );
        }
        MarkdownBlock::Table { rows } => {
            render_table(lines, rows, options);
        }
        MarkdownBlock::CodeFence {
            label,
            language,
            lines: code_lines,
        } => {
            lines.push(Line::from(render_code_block_header(
                label.as_deref(),
                *language,
            )));
            render_code_block(lines, code_lines, *language);
        }
    }
}

fn ensure_blank_line(lines: &mut Vec<Line<'static>>) {
    if !lines.is_empty() && !is_blank_line(lines.last().unwrap()) {
        lines.push(Line::from(""));
    }
}

fn normalize_blank_lines(lines: &mut Vec<Line<'static>>) {
    let mut normalized = Vec::with_capacity(lines.len());
    let mut previous_blank = true;

    for line in lines.drain(..) {
        let blank = is_blank_line(&line);
        if blank {
            if !previous_blank {
                normalized.push(Line::from(""));
            }
        } else {
            normalized.push(line);
        }
        previous_blank = blank;
    }

    while normalized.last().is_some_and(is_blank_line) {
        normalized.pop();
    }
    *lines = normalized;
}

fn is_blank_line(line: &Line<'static>) -> bool {
    line.spans.is_empty() || line.spans.iter().all(|span| span.content.trim().is_empty())
}

#[derive(Clone, Copy)]
struct BlockSpacing {
    before: bool,
    after: bool,
}

fn block_spacing(block: &MarkdownBlock) -> BlockSpacing {
    match block {
        MarkdownBlock::Heading { .. } => BlockSpacing {
            before: true,
            after: true,
        },
        MarkdownBlock::Rule => BlockSpacing {
            before: true,
            after: true,
        },
        MarkdownBlock::CodeFence { .. } => BlockSpacing {
            before: true,
            after: true,
        },
        MarkdownBlock::Paragraph { .. }
        | MarkdownBlock::Quote { .. }
        | MarkdownBlock::List { .. }
        | MarkdownBlock::Table { .. } => BlockSpacing {
            before: true,
            after: false,
        },
    }
}

fn render_list_block(
    lines: &mut Vec<Line<'static>>,
    ordered_start: Option<u64>,
    items: &[ListItem],
    default_fg: Option<Color>,
    list_depth: usize,
    options: &MarkdownRenderOptions,
) {
    for (index, item) in items.iter().enumerate() {
        let mut rendered_item = Vec::new();
        render_block_sequence(
            &mut rendered_item,
            &item.blocks,
            default_fg,
            list_depth + 1,
            false,
            options,
        );

        let number = ordered_start.map(|start| start + index as u64);
        let (prefix, prefix_style) = list_item_prefix(list_depth, number, item.task_state);
        let rest_prefix = " ".repeat(UnicodeWidthStr::width(prefix.as_str()));

        let mut rendered_any = false;
        for line in rendered_item {
            if is_blank_line(&line) {
                lines.push(line);
                continue;
            }
            let prefixed = if rendered_any {
                prepend_prefix(line, rest_prefix.clone(), Style::default())
            } else {
                prepend_prefix(line, prefix.clone(), prefix_style)
            };
            lines.push(prefixed);
            rendered_any = true;
        }

        if !rendered_any {
            lines.push(Line::from(Span::styled(prefix, prefix_style)));
        }
    }
}

fn list_item_prefix(
    list_depth: usize,
    ordered_number: Option<u64>,
    task_state: Option<bool>,
) -> (String, Style) {
    let indent = "  ".repeat(list_depth);
    if let Some(checked) = task_state {
        return (
            format!("{}{} ", indent, if checked { "☑" } else { "☐" }),
            Style::default().fg(if checked { Color::LightGreen } else { DIM }),
        );
    }

    if let Some(number) = ordered_number {
        return (
            format!("{}{}. ", indent, format_list_number(list_depth, number)),
            Style::default().fg(DIM),
        );
    }

    let bullet = match list_depth {
        0 => "•",
        1 => "◦",
        _ => "▪",
    };
    (format!("{}{} ", indent, bullet), Style::default().fg(DIM))
}

fn format_list_number(list_depth: usize, number: u64) -> String {
    match list_depth {
        0 | 1 => number.to_string(),
        2 => number_to_letter(number),
        3 => number_to_roman(number),
        _ => number.to_string(),
    }
}

fn render_inline_nodes_as_lines(
    nodes: &[InlineNode],
    default_fg: Option<Color>,
    options: &MarkdownRenderOptions,
) -> Vec<Line<'static>> {
    let base_style = default_fg
        .map(|fg| Style::default().fg(fg))
        .unwrap_or_default();
    render_inline_nodes_as_lines_with_style(nodes, base_style, options)
}

fn render_inline_nodes_as_lines_with_style(
    nodes: &[InlineNode],
    base_style: Style,
    options: &MarkdownRenderOptions,
) -> Vec<Line<'static>> {
    let mut rendered: Vec<Vec<Span<'static>>> = vec![Vec::new()];
    append_inline_nodes(&mut rendered, nodes, base_style, options);

    let lines = rendered
        .into_iter()
        .map(|spans| {
            if spans.is_empty() {
                Line::from("")
            } else {
                Line::from(spans)
            }
        })
        .collect::<Vec<_>>();
    if let Some(max_width) = options.max_width {
        manual_wrap(lines, max_width as u16)
    } else {
        lines
    }
}

fn append_inline_nodes(
    lines: &mut Vec<Vec<Span<'static>>>,
    nodes: &[InlineNode],
    style: Style,
    options: &MarkdownRenderOptions,
) {
    for node in nodes {
        match node {
            InlineNode::Text(text) => {
                if !text.is_empty() {
                    if options.enable_hyperlinks {
                        append_text_with_links(lines.last_mut().unwrap(), text, style);
                    } else {
                        lines
                            .last_mut()
                            .unwrap()
                            .push(Span::styled(text.clone(), style));
                    }
                }
            }
            InlineNode::Strong(children) => {
                append_inline_nodes(lines, children, style.add_modifier(Modifier::BOLD), options)
            }
            InlineNode::Emphasis(children) => append_inline_nodes(
                lines,
                children,
                style.add_modifier(Modifier::ITALIC),
                options,
            ),
            InlineNode::Code(text) => {
                lines
                    .last_mut()
                    .unwrap()
                    .extend(render_inline_code_spans(text));
            }
            InlineNode::Link { text, url } => {
                if let Some(email) = url.strip_prefix("mailto:") {
                    lines
                        .last_mut()
                        .unwrap()
                        .push(Span::styled(email.to_string(), style));
                    continue;
                }
                let link_style = style.fg(INFO_COLOR).add_modifier(Modifier::UNDERLINED);
                if options.enable_hyperlinks {
                    lines
                        .last_mut()
                        .unwrap()
                        .push(Span::raw(osc8_start_sequence(url)));
                }
                if text.is_empty() {
                    lines
                        .last_mut()
                        .unwrap()
                        .push(Span::styled(url.clone(), link_style));
                } else {
                    append_inline_nodes(lines, text, link_style, options);
                }
                if options.enable_hyperlinks {
                    lines
                        .last_mut()
                        .unwrap()
                        .push(Span::raw(osc8_close_sequence()));
                }
            }
            InlineNode::SoftBreak => {
                lines.push(Vec::new());
            }
            InlineNode::HardBreak => {
                lines.push(Vec::new());
            }
        }
    }
}

fn line_display_width(line: &Line<'static>) -> usize {
    line.spans
        .iter()
        .map(|span| visible_text_width(&span.content))
        .sum()
}

fn pad_line_to_width(
    spans: &mut Vec<Span<'static>>,
    current_width: usize,
    target_width: usize,
    style: Style,
) {
    if target_width > current_width {
        spans.push(Span::styled(
            " ".repeat(target_width - current_width),
            style,
        ));
    }
}

fn prepend_prefix(line: Line<'static>, prefix: String, style: Style) -> Line<'static> {
    let mut spans = vec![Span::styled(prefix, style)];
    spans.extend(line.spans);
    Line::from(spans)
}

fn add_modifier_to_line(line: Line<'static>, modifier: Modifier) -> Line<'static> {
    let spans = line
        .spans
        .into_iter()
        .map(|span| Span::styled(span.content, span.style.add_modifier(modifier)))
        .collect::<Vec<_>>();
    Line::from(spans)
}

fn append_text_with_links(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
    let mut last = 0usize;
    while last < text.len() {
        let issue_match = ISSUE_REF_RE.captures(&text[last..]).and_then(|captures| {
            let whole = captures.get(0)?;
            Some((captures, last + whole.start(), last + whole.end()))
        });
        let url_match = URL_RE
            .find(&text[last..])
            .map(|m| (last + m.start(), last + m.end()));

        let next_kind = match (issue_match, url_match) {
            (Some((captures, start, end)), Some((url_start, url_end))) => {
                if start <= url_start {
                    Some(LinkMatch::Issue(captures, start, end))
                } else {
                    Some(LinkMatch::Url(url_start, url_end))
                }
            }
            (Some((captures, start, end)), None) => Some(LinkMatch::Issue(captures, start, end)),
            (None, Some((start, end))) => Some(LinkMatch::Url(start, end)),
            (None, None) => None,
        };

        let Some(link_match) = next_kind else {
            spans.push(Span::styled(text[last..].to_string(), style));
            break;
        };

        match link_match {
            LinkMatch::Issue(captures, start, end) => {
                let prefix = captures.get(1).map(|value| value.as_str()).unwrap_or("");
                let repo = captures.get(2).map(|value| value.as_str()).unwrap_or("");
                let number = captures.get(3).map(|value| value.as_str()).unwrap_or("");
                let prefix_start = start;
                let link_start = prefix_start + prefix.len();

                if last < prefix_start {
                    spans.push(Span::styled(text[last..prefix_start].to_string(), style));
                }
                if !prefix.is_empty() {
                    spans.push(Span::styled(prefix.to_string(), style));
                }
                let link_text = format!("{}#{}", repo, number);
                let url = format!("https://github.com/{}/issues/{}", repo, number);
                push_hyperlink_spans(spans, &link_text, &url, style);
                last = end;
                debug_assert!(last >= link_start);
            }
            LinkMatch::Url(start, mut end) => {
                if last < start {
                    spans.push(Span::styled(text[last..start].to_string(), style));
                }

                let mut url = &text[start..end];
                let mut trailing = String::new();
                while let Some(ch) = url.chars().last() {
                    if matches!(ch, '.' | ',' | ';' | ':' | '!' | '?') {
                        let trim_at = url.len() - ch.len_utf8();
                        trailing.insert(0, ch);
                        url = &url[..trim_at];
                        end -= ch.len_utf8();
                    } else {
                        break;
                    }
                }
                if !url.is_empty() {
                    push_hyperlink_spans(spans, url, url, style);
                }
                let trailing_len = trailing.len();
                if trailing_len > 0 {
                    spans.push(Span::styled(trailing, style));
                    last = end + trailing_len;
                } else {
                    last = end;
                }
            }
        }
    }
}

enum LinkMatch<'a> {
    Issue(regex::Captures<'a>, usize, usize),
    Url(usize, usize),
}

fn push_hyperlink_spans(spans: &mut Vec<Span<'static>>, text: &str, url: &str, style: Style) {
    let link_style = style.fg(INFO_COLOR).add_modifier(Modifier::UNDERLINED);
    spans.push(Span::raw(osc8_start_sequence(url)));
    spans.push(Span::styled(text.to_string(), link_style));
    spans.push(Span::raw(osc8_close_sequence()));
}

fn osc8_start_sequence(url: &str) -> String {
    format!("\x1b]8;;{}\x07", url)
}

fn line_to_ansi_string(line: &Line<'static>) -> String {
    let mut output = String::new();
    for span in &line.spans {
        if span.style == Style::default() {
            output.push_str(&span.content);
            continue;
        }

        let sgr = style_to_ansi_prefix(span.style);
        if !sgr.is_empty() {
            output.push_str(&sgr);
        }
        output.push_str(&span.content);
        if !sgr.is_empty() {
            output.push_str("\x1b[0m");
        }
    }
    output
}

fn style_to_ansi_prefix(style: Style) -> String {
    let mut parts = Vec::new();
    if style.add_modifier.contains(Modifier::BOLD) {
        parts.push("1".to_string());
    }
    if style.add_modifier.contains(Modifier::ITALIC) {
        parts.push("3".to_string());
    }
    if style.add_modifier.contains(Modifier::UNDERLINED) {
        parts.push("4".to_string());
    }
    if let Some(fg) = style.fg {
        parts.extend(color_to_ansi_codes(fg, false));
    }
    if let Some(bg) = style.bg {
        parts.extend(color_to_ansi_codes(bg, true));
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!("\x1b[{}m", parts.join(";"))
    }
}

fn color_to_ansi_codes(color: Color, background: bool) -> Vec<String> {
    match color {
        Color::Rgb(r, g, b) => vec![if background {
            format!("48;2;{};{};{}", r, g, b)
        } else {
            format!("38;2;{};{};{}", r, g, b)
        }],
        Color::Indexed(index) => vec![if background {
            format!("48;5;{}", index)
        } else {
            format!("38;5;{}", index)
        }],
        Color::Black => vec![if background { "40" } else { "30" }.to_string()],
        Color::Red => vec![if background { "41" } else { "31" }.to_string()],
        Color::Green => vec![if background { "42" } else { "32" }.to_string()],
        Color::Yellow => vec![if background { "43" } else { "33" }.to_string()],
        Color::Blue => vec![if background { "44" } else { "34" }.to_string()],
        Color::Magenta => vec![if background { "45" } else { "35" }.to_string()],
        Color::Cyan => vec![if background { "46" } else { "36" }.to_string()],
        Color::Gray => vec![if background { "47" } else { "37" }.to_string()],
        Color::DarkGray => vec![if background { "100" } else { "90" }.to_string()],
        Color::LightRed => vec![if background { "101" } else { "91" }.to_string()],
        Color::LightGreen => vec![if background { "102" } else { "92" }.to_string()],
        Color::LightYellow => vec![if background { "103" } else { "93" }.to_string()],
        Color::LightBlue => vec![if background { "104" } else { "94" }.to_string()],
        Color::LightMagenta => vec![if background { "105" } else { "95" }.to_string()],
        Color::LightCyan => vec![if background { "106" } else { "96" }.to_string()],
        Color::White => vec![if background { "107" } else { "97" }.to_string()],
        Color::Reset => Vec::new(),
    }
}

fn inline_nodes_to_plain_text(nodes: &[InlineNode]) -> String {
    let mut output = String::new();
    for node in nodes {
        match node {
            InlineNode::Text(text) | InlineNode::Code(text) => output.push_str(text),
            InlineNode::Strong(children) | InlineNode::Emphasis(children) => {
                output.push_str(&inline_nodes_to_plain_text(children));
            }
            InlineNode::Link { text, url } => {
                if text.is_empty() {
                    output.push_str(url);
                } else {
                    output.push_str(&inline_nodes_to_plain_text(text));
                }
            }
            InlineNode::SoftBreak => output.push(' '),
            InlineNode::HardBreak => output.push('\n'),
        }
    }
    output
}

fn is_container_end(tag_end: &TagEnd, end: ContainerEnd) -> bool {
    matches!(
        (tag_end, end),
        (TagEnd::BlockQuote(_), ContainerEnd::BlockQuote)
    )
}

fn is_list_end(tag_end: &TagEnd) -> bool {
    matches!(tag_end, TagEnd::List(_))
}

fn is_inline_end(tag_end: &TagEnd, end: InlineEnd) -> bool {
    matches!(
        (tag_end, end),
        (TagEnd::Paragraph, InlineEnd::Paragraph)
            | (TagEnd::Heading(_), InlineEnd::Heading)
            | (TagEnd::TableCell, InlineEnd::TableCell)
            | (TagEnd::Strong, InlineEnd::Strong)
            | (TagEnd::Emphasis, InlineEnd::Emphasis)
            | (TagEnd::Link, InlineEnd::Link)
            | (TagEnd::Image, InlineEnd::Image)
    )
}

fn is_inline_start_tag(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Strong | Tag::Emphasis | Tag::Link { .. } | Tag::Image { .. }
    )
}

fn is_inline_event(event: &Event<'_>) -> bool {
    matches!(
        event,
        Event::Text(_)
            | Event::Code(_)
            | Event::Html(_)
            | Event::InlineHtml(_)
            | Event::SoftBreak
            | Event::HardBreak
            | Event::FootnoteReference(_)
    )
}

fn heading_level_to_usize(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn number_to_letter(mut number: u64) -> String {
    let mut result = String::new();
    while number > 0 {
        number -= 1;
        result.insert(0, char::from(b'a' + (number % 26) as u8));
        number /= 26;
    }
    result
}

fn number_to_roman(mut number: u64) -> String {
    const ROMAN_VALUES: &[(u64, &str)] = &[
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];

    let mut result = String::new();
    for (value, numeral) in ROMAN_VALUES {
        while number >= *value {
            result.push_str(numeral);
            number -= *value;
        }
    }
    result
}

#[derive(Clone, Copy)]
struct CodeBlockTheme {
    badge_bg: Color,
    badge_fg: Color,
    border: Color,
    background: Color,
}

fn code_block_theme(language: CodeLanguage) -> CodeBlockTheme {
    match language {
        CodeLanguage::Diff => CodeBlockTheme {
            badge_bg: Color::Indexed(110),
            badge_fg: Color::Indexed(232),
            border: Color::Indexed(67),
            background: Color::Indexed(235),
        },
        CodeLanguage::Shell => CodeBlockTheme {
            badge_bg: Color::Indexed(72),
            badge_fg: Color::Indexed(232),
            border: Color::Indexed(66),
            background: Color::Indexed(235),
        },
        CodeLanguage::Json => CodeBlockTheme {
            badge_bg: Color::Indexed(179),
            badge_fg: Color::Indexed(232),
            border: Color::Indexed(143),
            background: Color::Indexed(235),
        },
        CodeLanguage::Rust => CodeBlockTheme {
            badge_bg: Color::Indexed(173),
            badge_fg: Color::Indexed(232),
            border: Color::Indexed(137),
            background: CODE_BG,
        },
        CodeLanguage::Python => CodeBlockTheme {
            badge_bg: Color::Indexed(110),
            badge_fg: Color::Indexed(232),
            border: Color::Indexed(74),
            background: Color::Indexed(235),
        },
        CodeLanguage::Plain => CodeBlockTheme {
            badge_bg: PANEL_ACCENT,
            badge_fg: Color::Indexed(232),
            border: BORDER_MUTED,
            background: CODE_BG,
        },
    }
}

fn render_code_block_header(label: Option<&str>, language: CodeLanguage) -> Vec<Span<'static>> {
    let theme = code_block_theme(language);
    let label = label.unwrap_or("code");
    vec![
        Span::styled("╭─", Style::default().fg(theme.border)),
        Span::styled(
            format!(" {} ", label),
            Style::default()
                .fg(theme.badge_fg)
                .bg(theme.badge_bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "──────────────────────────────",
            Style::default().fg(theme.border),
        ),
    ]
}

fn render_code_block_footer(language: CodeLanguage) -> Vec<Span<'static>> {
    let theme = code_block_theme(language);
    vec![Span::styled(
        "╰────────────────────────────────────",
        Style::default().fg(theme.border),
    )]
}

fn render_code_block(
    lines: &mut Vec<Line<'static>>,
    code_block_lines: &[String],
    language: CodeLanguage,
) {
    if language == CodeLanguage::Diff {
        lines.extend(render_structured_diff_block(code_block_lines));
        lines.push(Line::from(render_code_block_footer(language)));
        return;
    }

    let theme = code_block_theme(language);
    lines.extend(render_highlighted_code_block(
        code_block_lines,
        language,
        theme.border,
        theme.background,
    ));
    lines.push(Line::from(render_code_block_footer(language)));
}

fn code_token_color(kind: CodeTokenKind, language: CodeLanguage) -> Color {
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
        CodeTokenKind::ShellError => Color::Indexed(210),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        line_display_width, render_markdown_ansi_with_options, render_markdown_impl,
        render_markdown_with_options, streaming_markdown_advance_stable_boundary,
        MarkdownRenderOptions,
    };
    use crate::ui::chat::WHITE;
    use ratatui::style::{Color, Modifier};

    #[test]
    fn fenced_code_blocks_render_header_and_highlighted_tokens() {
        let lines = render_markdown_impl("```rust\nfn main() {}\n```", None);
        let header_line = lines
            .iter()
            .find(|line| line.to_string().contains("rust"))
            .unwrap();
        assert!(header_line.to_string().contains("rust"));
        let code_line = lines
            .iter()
            .find(|line| line.to_string().contains("fn main"))
            .unwrap();
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.content == " 1 " && span.style.fg == Some(Color::Indexed(244))));
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Indexed(111))));
    }

    #[test]
    fn diff_code_blocks_render_added_lines_with_gutter_background_and_syntax() {
        let lines = render_markdown_impl(
            "```diff\ndiff --git a/src/main.rs b/src/main.rs\n@@ -0,0 +1,1 @@\n+fn main() {}\n```",
            None,
        );
        let code_line = lines
            .iter()
            .find(|line| line.to_string().contains("fn main"))
            .unwrap();
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.content == "+ 1 " && span.style.fg == Some(Color::Indexed(114))));
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.content == "fn" && span.style.fg == Some(Color::Indexed(111))));
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.style.bg == Some(Color::Indexed(22))));
    }

    #[test]
    fn json_code_blocks_highlight_property_keys() {
        let lines = render_markdown_impl("```json\n{\"name\": \"yode\"}\n```", None);
        let code_line = lines
            .iter()
            .find(|line| line.to_string().contains("\"name\""))
            .unwrap();
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Indexed(153))));
    }

    #[test]
    fn diff_code_blocks_highlight_file_headers_and_hunk_ranges() {
        let lines = render_markdown_impl(
            "```diff\ndiff --git a/src/main.rs b/src/main.rs\n@@ -10,2 +10,4 @@ fn render()\n```",
            None,
        );
        let file_line = lines
            .iter()
            .find(|line| line.to_string().contains("a/src/main.rs"))
            .unwrap();
        assert!(file_line
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Indexed(223))));

        let hunk_line = lines
            .iter()
            .find(|line| line.to_string().contains("@@ -10,2 +10,4 @@"))
            .unwrap();
        assert!(hunk_line
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Indexed(153))));
    }

    #[test]
    fn inline_code_renders_token_spans() {
        let lines = render_markdown_impl("Use `fn main()` here.", None);
        let line = lines
            .iter()
            .find(|line| line.to_string().contains("fn main"))
            .unwrap();
        assert!(line
            .spans
            .iter()
            .any(|span| span.content == "fn" && span.style.fg == Some(Color::Indexed(111))));
    }

    #[test]
    fn headings_preserve_inline_rich_rendering() {
        let lines = render_markdown_impl("# Build **fast** with `cargo test`", None);
        let heading = lines
            .iter()
            .find(|line| line.to_string().contains("Build"))
            .unwrap();
        assert!(!heading.to_string().contains('#'));
        assert!(
            heading
                .spans
                .iter()
                .any(|span| span.content == "fast"
                    && span.style.add_modifier.contains(Modifier::BOLD))
        );
        assert!(heading.spans.iter().any(|span| {
            span.content == "cargo" && span.style.bg == Some(crate::ui::chat::INLINE_CODE_BG)
        }));
        assert!(heading.spans.iter().any(|span| {
            span.content.contains("Build")
                && span.style.add_modifier.contains(Modifier::ITALIC)
                && span.style.add_modifier.contains(Modifier::UNDERLINED)
        }));
    }

    #[test]
    fn table_cells_preserve_inline_rich_rendering() {
        let lines = render_markdown_impl("| Col |\n| --- |\n| **bold** and `code` |", None);
        let row = lines
            .iter()
            .find(|line| line.to_string().contains("bold") && line.to_string().contains("code"))
            .unwrap();
        assert!(
            row.spans
                .iter()
                .any(|span| span.content == "bold"
                    && span.style.add_modifier.contains(Modifier::BOLD))
        );
        assert!(row.spans.iter().any(|span| {
            span.content == "code" && span.style.bg == Some(crate::ui::chat::INLINE_CODE_BG)
        }));
    }

    #[test]
    fn tables_render_full_box_borders_like_claude() {
        let lines = render_markdown_impl("| A | B |\n| --- | --- |\n| 1 | 2 |", None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(lines.iter().any(|line| line.starts_with('┌')));
        assert!(lines.iter().any(|line| line.starts_with('├')));
        assert!(lines.iter().any(|line| line.starts_with('└')));
        assert!(lines
            .iter()
            .any(|line| line.starts_with('│') && line.contains('1') && line.contains('2')));
    }

    #[test]
    fn links_render_as_osc8_hyperlinks_when_enabled() {
        let lines = render_markdown_with_options(
            "[Rust](https://www.rust-lang.org)",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: true,
            },
        );
        let line = lines.first().unwrap();
        assert!(line.spans.iter().any(|span| span
            .content
            .contains("\x1b]8;;https://www.rust-lang.org\x07")));
        assert!(line
            .spans
            .iter()
            .any(|span| span.content == crate::ui::chat_layout::osc8_close_sequence()));
    }

    #[test]
    fn mailto_links_render_as_plain_text() {
        let lines = render_markdown_with_options(
            "[support](mailto:support@example.com)",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: true,
            },
        );
        let rendered = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("support@example.com"));
        assert!(!rendered.contains("mailto:"));
        assert!(!rendered.contains("\x1b]8;;"));
    }

    #[test]
    fn github_issue_references_are_hyperlinked_in_plain_text() {
        let lines = render_markdown_with_options(
            "See anthropics/claude-code#24180 for context.",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: true,
            },
        );
        let rendered = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("anthropics/claude-code#24180"));
        assert!(rendered.contains("\x1b]8;;https://github.com/anthropics/claude-code/issues/24180"));
    }

    #[test]
    fn bare_urls_are_hyperlinked_in_plain_text() {
        let lines = render_markdown_with_options(
            "Open https://example.com/docs for details.",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: true,
            },
        );
        let rendered = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("https://example.com/docs"));
        assert!(rendered.contains("\x1b]8;;https://example.com/docs"));
    }

    #[test]
    fn bare_urls_do_not_absorb_trailing_punctuation() {
        let lines = render_markdown_with_options(
            "Visit https://example.com/docs, then continue.",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: true,
            },
        );
        let rendered = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("\x1b]8;;https://example.com/docs"));
        assert!(!rendered.contains("\x1b]8;;https://example.com/docs,"));
    }

    #[test]
    fn github_issue_links_exclude_trailing_punctuation() {
        let rendered = render_markdown_ansi_with_options(
            "See anthropics/claude-code#24180, then continue.",
            Some(WHITE),
            MarkdownRenderOptions {
                max_width: None,
                enable_hyperlinks: true,
            },
        )
        .join("\n");
        assert!(rendered.contains("anthropics/claude-code#24180"));
        assert!(
            rendered.contains("\u{1b}]8;;https://github.com/anthropics/claude-code/issues/24180")
        );
        assert!(
            !rendered.contains("\u{1b}]8;;https://github.com/anthropics/claude-code/issues/24180,")
        );
    }

    #[test]
    fn markdown_lists_keep_single_space_after_bullets() {
        let lines = render_markdown_impl("- one\n- two", None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(lines.iter().any(|line| line == "• one"));
        assert!(lines.iter().any(|line| line == "• two"));
    }

    #[test]
    fn long_headings_wrap_without_losing_heading_text() {
        let rendered = render_markdown_with_options(
            "# This is a very long heading that should wrap cleanly",
            Some(WHITE),
            MarkdownRenderOptions {
                max_width: Some(24),
                enable_hyperlinks: false,
            },
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
        assert!(rendered.len() >= 2);
        assert!(rendered
            .iter()
            .any(|line| line.contains("This is a very long")));
    }

    #[test]
    fn heading_wrap_continuations_keep_heading_style() {
        let rendered = render_markdown_with_options(
            "# Heading with enough words to wrap across multiple visual rows",
            Some(WHITE),
            MarkdownRenderOptions {
                max_width: Some(22),
                enable_hyperlinks: false,
            },
        );
        assert!(rendered.len() >= 2);
        assert!(rendered.iter().all(|line| {
            line.spans.iter().any(|span| {
                span.style.add_modifier.contains(Modifier::BOLD)
                    && span.style.add_modifier.contains(Modifier::UNDERLINED)
            })
        }));
    }

    #[test]
    fn inline_code_wraps_across_narrow_widths() {
        let rendered = render_markdown_with_options(
            "Use `very_long_inline_code_value_here` now.",
            Some(WHITE),
            MarkdownRenderOptions {
                max_width: Some(18),
                enable_hyperlinks: false,
            },
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
        assert!(rendered.len() >= 2);
        assert!(rendered.iter().any(|line| line.contains("very_long")));
    }

    #[test]
    fn nested_bullets_keep_distinct_indentation_and_markers() {
        let rendered = render_markdown_impl("- parent\n    - child\n        - grandchild", None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(rendered.iter().any(|line| line.starts_with("• parent")));
        let child = rendered
            .iter()
            .find(|line| line.contains("◦ child"))
            .expect("child bullet");
        let grandchild = rendered
            .iter()
            .find(|line| line.contains("▪ grandchild"))
            .expect("grandchild bullet");
        assert!(child.find('◦').unwrap() > 0);
        assert!(grandchild.find('▪').unwrap() > child.find('◦').unwrap());
    }

    #[test]
    fn mixed_bold_italic_wrap_preserves_style_continuity() {
        let rendered = render_markdown_with_options(
            "***important wrapped emphasis keeps its combined style across lines***",
            Some(WHITE),
            MarkdownRenderOptions {
                max_width: Some(18),
                enable_hyperlinks: false,
            },
        );
        assert!(rendered.len() >= 2);
        assert!(rendered.iter().all(|line| {
            line.spans.iter().any(|span| {
                span.style.add_modifier.contains(Modifier::BOLD)
                    && span.style.add_modifier.contains(Modifier::ITALIC)
            })
        }));
    }

    #[test]
    fn cjk_tables_render_without_losing_cells() {
        let lines = render_markdown_impl(
            "| 列 | 值 |\n| --- | --- |\n| 工具 | 远程 |\n| 状态 | 正常 |",
            None,
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
        assert!(lines.iter().any(|line| line.contains("工具")));
        assert!(lines.iter().any(|line| line.contains("状态")));
        assert!(lines
            .iter()
            .any(|line| line.starts_with('┌') || line.contains("Column")));
    }

    #[test]
    fn tables_wrap_to_fit_requested_width() {
        let lines = render_markdown_with_options(
            "| Column |\n| --- |\n| this is a very long cell with `inline code` inside |",
            None,
            MarkdownRenderOptions {
                max_width: Some(24),
                enable_hyperlinks: false,
            },
        );
        assert!(lines.iter().any(|line| {
            let text = line.to_string();
            text.contains("Column:") || text.contains("this is a")
        }));
        assert!(lines.iter().all(|line| line_display_width(line) <= 24));
    }

    #[test]
    fn narrow_tables_fall_back_to_vertical_key_value_layout() {
        let lines = render_markdown_with_options(
            "| Metric | Value |\n| --- | --- |\n| Runtime | This is a very long wrapped explanation |\n| Status | Healthy |",
            None,
            MarkdownRenderOptions {
                max_width: Some(18),
                enable_hyperlinks: false,
            },
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
        assert!(lines.iter().any(|line| line.contains("Metric:")));
        assert!(lines.iter().any(|line| line.contains("Value:")));
        assert!(!lines.iter().any(|line| line.contains("┼")));
    }

    #[test]
    fn wrapped_tables_fall_back_to_vertical_layout_to_avoid_empty_padding() {
        let lines = render_markdown_with_options(
            "| 优化项 | 说明 | 工作量 |\n| --- | --- | --- |\n| hooks | 缺少 notification hooks, permission hooks, lifecycle hooks，需要补齐事件通知、权限拦截、生命周期回调 | 中 |",
            None,
            MarkdownRenderOptions {
                max_width: Some(64),
                enable_hyperlinks: false,
            },
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.contains("优化项:")));
        assert!(lines.iter().any(|line| line.contains("说明:")));
        assert!(!lines.iter().any(|line| line.contains("┼")));
    }

    #[test]
    fn code_fence_caption_stays_dense_and_labeled() {
        let rendered = render_markdown_impl("```rust\nfn main() {}\n```", None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(rendered[0].contains("rust"));
        assert!(rendered[0].starts_with("╭─"));
        assert!(!rendered[0].contains("Code block"));
    }

    #[test]
    fn hyperlink_text_keeps_underline_intensity() {
        let lines = render_markdown_with_options(
            "[Docs](https://example.com/docs)",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: true,
            },
        );
        assert!(lines[0].spans.iter().any(|span| {
            span.content == "Docs" && span.style.add_modifier.contains(Modifier::UNDERLINED)
        }));
    }

    #[test]
    fn streaming_boundary_advances_monotonically_from_existing_prefix() {
        let first = "first paragraph\n\nsecond";
        let first_boundary = streaming_markdown_advance_stable_boundary(first, 0);
        assert!(first_boundary > 0);

        let second = "first paragraph\n\nsecond line grows";
        let second_boundary = streaming_markdown_advance_stable_boundary(second, first_boundary);
        assert_eq!(second_boundary, first_boundary);

        let third = "first paragraph\n\nsecond line grows\n\nthird block";
        let third_boundary = streaming_markdown_advance_stable_boundary(third, first_boundary);
        assert!(third_boundary > first_boundary);
    }

    #[test]
    fn streaming_boundary_keeps_unclosed_code_fence_unstable() {
        let text = "intro\n\n```rust\nfn main()";
        let boundary = streaming_markdown_advance_stable_boundary(text, 0);
        assert_eq!(boundary, "intro\n\n".len());
    }

    #[test]
    fn streaming_boundary_keeps_unicode_table_block_unstable_until_next_section() {
        let text = "根据已有的深度分析记忆，我直接给你综合结论，不需要重新扫描。\nYode vs Claude Code 综合对比\n基本面\n维度 │ Yode │ Claude Code\n──────┼──────┼─────────────\n";
        let boundary = streaming_markdown_advance_stable_boundary(text, 0);
        assert_eq!(
            boundary,
            "根据已有的深度分析记忆，我直接给你综合结论，不需要重新扫描。\nYode vs Claude Code 综合对比\n".len()
        );
    }

    #[test]
    fn streaming_boundary_holds_trailing_heading_until_followup_arrives() {
        let text = "根据已有的深度分析记忆，我直接给你综合结论，不需要重新扫描。\nYode vs Claude Code 综合对比\n";
        let boundary = streaming_markdown_advance_stable_boundary(text, 0);
        assert_eq!(
            boundary,
            "根据已有的深度分析记忆，我直接给你综合结论，不需要重新扫描。\n".len()
        );
    }

    #[test]
    fn streaming_boundary_keeps_pipe_table_header_unstable_until_table_ends() {
        let first = "一、基本盘\n| 维度 | Yode | Claude Code |\n";
        let first_boundary = streaming_markdown_advance_stable_boundary(first, 0);
        assert_eq!(first_boundary, "一、基本盘\n".len());

        let second = "一、基本盘\n| 维度 | Yode | Claude Code |\n| --- | --- | --- |\n";
        let second_boundary = streaming_markdown_advance_stable_boundary(second, 0);
        assert_eq!(second_boundary, "一、基本盘\n".len());

        let third = "一、基本盘\n| 维度 | Yode | Claude Code |\n| --- | --- | --- |\n| 代码量 | 15万 | 52万 |\n二、命令系统\n";
        let third_boundary = streaming_markdown_advance_stable_boundary(third, 0);
        assert!(third_boundary > second_boundary);
    }

    #[test]
    fn unicode_table_and_compound_rows_are_normalized() {
        let lines = render_markdown_impl(
            "基本面\n 维度 │ Yode │ Claude Code\n──────┼──────┼─────────────\n| 语言 | Rust | TypeScript | | 工具数 | 45 | 50+ |",
            None,
        );
        assert!(lines.iter().any(|line| line.to_string().contains("基本面")));
        assert!(lines.iter().any(|line| line.to_string().contains("维度")));
        assert!(lines.iter().any(|line| line.to_string().contains("语言")));
        assert!(lines.iter().any(|line| line.to_string().contains("工具数")));
    }

    #[test]
    fn short_section_lines_are_promoted_to_headings() {
        let lines = render_markdown_impl(
            "按优先级的优化空间\nP0 — 核心缺失（严重影响日常使用）\n1. First item",
            None,
        );
        let heading = lines
            .iter()
            .find(|line| line.to_string().contains("按优先级的优化空间"))
            .unwrap();
        assert!(!heading.to_string().contains('#'));
        let p0 = lines
            .iter()
            .find(|line| line.to_string().contains("P0 — 核心缺失"))
            .unwrap();
        assert!(!p0.to_string().contains('#'));
    }

    #[test]
    fn real_world_summary_keeps_blank_lines_around_promoted_headings() {
        let lines = render_markdown_impl(
            "Yode vs Claude Code 综合对比\n基本面\n维度 │ Yode │ Claude Code\n──────┼──────┼─────────────\n语言 │ Rust (~15万行) │ TypeScript (~52万行)\n核心差距 (按影响程度排序)\nP0 - 严重缺失 (阻塞日常使用)\n1. 命令系统缺陷",
            None,
        );
        let basic_index = lines
            .iter()
            .position(|line| line.to_string().contains("基本面"))
            .unwrap();
        assert!(basic_index > 0);
        assert!(lines[basic_index - 1].to_string().is_empty());

        let gap_index = lines
            .iter()
            .position(|line| line.to_string().contains("核心差距"))
            .unwrap();
        assert!(gap_index > 0);
        assert!(lines[gap_index - 1].to_string().is_empty());
    }

    #[test]
    fn numbered_section_headings_get_consistent_blank_lines() {
        let lines = render_markdown_impl(
            "1. 命令系统 — 差 2.7 倍\n• 差距：Yode 只有同步 trait\n2. 上下文压缩 — 差 7 层\n• 差距：Yode 只有 eviction\n3. MCP 客户端 — 严重缺失\n• 差距：仅 stdio",
            None,
        );

        let second = lines
            .iter()
            .position(|line| line.to_string().contains("2. 上下文压缩"))
            .unwrap();
        assert!(second > 0);
        assert!(lines[second - 1].to_string().is_empty());

        let third = lines
            .iter()
            .position(|line| line.to_string().contains("3. MCP 客户端"))
            .unwrap();
        assert!(third > 0);
        assert!(lines[third - 1].to_string().is_empty());
    }

    #[test]
    fn chinese_sections_and_priority_heads_use_distinct_levels() {
        let lines = render_markdown_impl(
            "三、Yode 严重缺失的功能（按优先级）\nP0 - 核心缺失\n- /init\nP1 - 重要缺失\n- Skills\n四、Yode 的相对优势\n1. 开源",
            None,
        );

        let section = lines
            .iter()
            .find(|line| line.to_string().contains("三、Yode 严重缺失的功能"))
            .unwrap();
        let p0 = lines
            .iter()
            .find(|line| line.to_string().contains("P0 - 核心缺失"))
            .unwrap();
        let section_fg = section.spans.iter().find_map(|span| span.style.fg).unwrap();
        let p0_fg = p0.spans.iter().find_map(|span| span.style.fg).unwrap();
        assert_ne!(section_fg, p0_fg);
    }

    #[test]
    fn pasted_analysis_sample_normalizes_sections_and_table_rows() {
        let sample = "Yode vs Claude Code 综合对比\n基本面\n 维度 │ Yode           │ Claude Code          \n──────┼────────────────┼──────────────────────\n 语言 │ Rust (~15万行) │ TypeScript (~52万行) \n| 工具数 | ~45 | ~50+ |\n| 命令数 | ~30 | ~80+ |\n| MCP | rmcp (基础) | 完整SDK (SSE/Stdio/HTTP) |\n核心差距 (优先级排序)\nP0 - 严重缺失\n1. 命令系统缺陷";
        let lines = render_markdown_impl(sample, None);

        assert!(lines.iter().all(|line| !line.to_string().contains("###")));
        assert!(lines
            .iter()
            .all(|line| !line.to_string().contains("| 工具数 | ~45 | ~50+ |")));

        let basic = lines
            .iter()
            .position(|line| line.to_string().contains("基本面"))
            .unwrap();
        assert!(basic > 0);
        assert!(lines[basic - 1].to_string().is_empty());

        let p0 = lines
            .iter()
            .position(|line| line.to_string().contains("P0 - 严重缺失"))
            .unwrap();
        assert!(p0 > 0);
        assert!(lines[p0 - 1].to_string().is_empty());
    }

    #[test]
    fn loose_ascii_pipe_rows_become_real_table_blocks() {
        let sample = "架构对比要点\n维度 | Yode | Claude Code |\n命令注册 | 全量静态编译时 | 懒加载 + 运行时动态 |\nUI 渲染 | 纯文本 | React JSX (交互) |";
        let lines = render_markdown_impl(sample, None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.contains("架构对比要点")));

        assert!(lines.iter().any(|line| line.contains("维度")));
        assert!(lines.iter().any(|line| line.contains("命令注册")));
        assert!(lines.iter().any(|line| line.contains("UI 渲染")));
        assert!(lines
            .iter()
            .all(|line| !line.contains("维度 | Yode | Claude Code |")));
        assert!(lines
            .iter()
            .all(|line| !line.contains("命令注册 | 全量静态编译时 | 懒加载 + 运行时动态 |")));
    }

    #[test]
    fn unicode_bullet_lines_become_markdown_lists() {
        let lines = render_markdown_impl("优势\n  • 性能\n  • 安全", None);
        assert!(lines.iter().any(|line| line.to_string().contains("• 性能")));
        assert!(lines.iter().any(|line| line.to_string().contains("• 安全")));
    }

    #[test]
    fn structural_lines_strip_two_space_indent() {
        let lines = render_markdown_impl("  基本面\n  | A | B |\n  | 1 | 2 |", None);
        let heading = lines
            .iter()
            .find(|line| line.to_string().contains("基本面"))
            .unwrap();
        assert!(!heading.to_string().starts_with("  基本面"));
    }

    #[test]
    fn shell_code_blocks_render_as_highlighted_code() {
        let lines = render_markdown_impl(
            "```bash\nuser@yode ~/repo $ cargo test -- --nocapture\necho $HOME\n```",
            None,
        );
        let command_line = lines
            .iter()
            .find(|line| line.to_string().contains("cargo test"))
            .unwrap();
        assert!(command_line
            .spans
            .iter()
            .any(|span| span.content == " 1 " && span.style.fg == Some(Color::Indexed(244))));
        assert!(command_line
            .spans
            .iter()
            .any(|span| span.content == "cargo" && span.style.fg == Some(Color::Indexed(222))));
        assert!(command_line.spans.iter().any(
            |span| span.content == "--nocapture" && span.style.fg == Some(Color::Indexed(111))
        ));

        let second_line = lines
            .iter()
            .find(|line| line.to_string().contains("echo $HOME"))
            .unwrap();
        assert!(second_line
            .spans
            .iter()
            .any(|span| span.content == " 2 " && span.style.fg == Some(Color::Indexed(244))));
        assert!(second_line
            .spans
            .iter()
            .any(|span| span.content == "echo" && span.style.fg == Some(Color::Indexed(222))));
        assert!(second_line
            .spans
            .iter()
            .any(|span| span.content == "$HOME" && span.style.fg == Some(Color::Indexed(215))));
    }

    #[test]
    fn shell_code_blocks_render_as_generic_highlighted_code_without_transcript_gutter() {
        let lines = render_markdown_impl(
            "```bash\nPS C:\\repo> cargo test `\n>> -- --nocapture\n./scripts/run.sh --config ./cfg.json\n```",
            None,
        );
        let continuation_line = lines
            .iter()
            .find(|line| line.to_string().contains("-- --nocapture"))
            .unwrap();
        assert!(continuation_line
            .spans
            .iter()
            .any(|span| span.content == " 2 " && span.style.fg == Some(Color::Indexed(244))));

        let path_line = lines
            .iter()
            .find(|line| line.to_string().contains("./scripts/run.sh"))
            .unwrap();
        assert!(path_line
            .spans
            .iter()
            .any(|span| span.content.contains("./scripts/run.sh")
                && span.style.fg == Some(Color::Indexed(222))));
        assert!(path_line
            .spans
            .iter()
            .any(|span| span.content.contains("./cfg.json")
                && span.style.fg == Some(Color::Indexed(153))));
    }

    #[test]
    fn shell_code_blocks_highlight_output_paths_and_numbers() {
        let lines = render_markdown_impl(
            "```bash\ncat /Users/pyu/code/yode/package.json\nprintf 14\n```",
            None,
        );
        let compiling_line = lines
            .iter()
            .find(|line| {
                line.to_string()
                    .contains("/Users/pyu/code/yode/package.json")
            })
            .unwrap();
        assert!(compiling_line.spans.iter().any(|span| span
            .content
            .contains("/Users/pyu/code/yode/package.json")
            && span.style.fg == Some(Color::Indexed(153))));

        let count_line = lines
            .iter()
            .find(|line| line.to_string().contains("printf 14"))
            .unwrap();
        assert!(count_line
            .spans
            .iter()
            .any(|span| span.content == "14" && span.style.fg == Some(Color::Indexed(151))));
    }

    #[test]
    fn headings_and_code_blocks_insert_vertical_spacing() {
        let lines = render_markdown_impl("# Title\nparagraph\n```rust\nfn main() {}\n```", None);
        let title_index = lines
            .iter()
            .position(|line| line.to_string().contains("Title"))
            .unwrap();
        assert!(!lines[title_index].to_string().contains('#'));
        let code_index = lines
            .iter()
            .position(|line| line.to_string().contains("rust"))
            .unwrap();
        assert_eq!(title_index, 0);
        assert!(lines[title_index + 1].to_string().is_empty());
        assert!(lines[code_index - 1].to_string().is_empty());
    }

    #[test]
    fn blank_lines_are_collapsed_and_trimmed() {
        let lines = render_markdown_impl("\n\n> quote\n\n\ntext\n\n", None);
        assert_eq!(lines.first().unwrap().to_string(), "▎ quote");
        let text_index = lines
            .iter()
            .position(|line| line.to_string() == "text")
            .unwrap();
        assert!(lines[text_index - 1].to_string().is_empty());
        assert!(!lines.last().unwrap().to_string().is_empty());
    }

    #[test]
    fn whitespace_only_lines_do_not_create_large_vertical_gaps() {
        let lines = render_markdown_impl(
            "10. MCP 多配置源\n11. MCP OAuth 认证\n   \n      \n\n\n21. Auto Dream - 后台思考能力",
            None,
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.contains("10. MCP 多配置源")));
        assert!(lines.iter().any(|line| line.contains("21. Auto Dream")));
        assert!(!lines
            .windows(2)
            .any(|window| window[0].is_empty() && window[1].is_empty()));
    }

    #[test]
    fn plain_text_fast_path_treats_whitespace_only_lines_as_blank() {
        let lines = render_markdown_with_options(
            "alpha\n   \n\nbeta",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: false,
            },
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();

        assert_eq!(lines, vec!["alpha", "", "beta"]);
    }

    #[test]
    fn blockquote_content_is_rendered_italic_like_claude() {
        let lines = render_markdown_impl("> quoted text", None);
        let quote = lines.first().unwrap();
        assert!(quote
            .spans
            .iter()
            .any(|span| span.content.contains("quoted text")
                && span.style.add_modifier.contains(Modifier::ITALIC)));
    }

    #[test]
    fn paragraph_lines_collapse_into_single_markdown_block() {
        let lines = render_markdown_impl("first line\nsecond line\n\n- item", None);
        assert_eq!(lines[0].to_string(), "first line");
        assert_eq!(lines[1].to_string(), "second line");
        assert!(lines[2].to_string().is_empty());
        assert!(lines.iter().any(|line| line.to_string().contains("• item")));
    }

    #[test]
    fn plain_text_fast_path_preserves_wrapping_without_markdown_parse() {
        let lines = render_markdown_with_options(
            "plain text line one\nplain text line two with width",
            None,
            MarkdownRenderOptions {
                max_width: Some(20),
                enable_hyperlinks: false,
            },
        );
        assert!(lines.iter().all(|line| line_display_width(line) <= 20));
        assert!(lines
            .iter()
            .any(|line| line.to_string().contains("plain text line one")));
        assert!(lines
            .iter()
            .any(|line| line.to_string().contains("plain text line two")));
    }

    #[test]
    fn blank_lines_between_list_items_are_collapsed() {
        let lines = render_markdown_impl("1. one\n\n2. two\n\n3. three", None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(lines.iter().any(|line| line.contains("1. one")));
        assert!(lines.iter().any(|line| line.contains("2. two")));
        assert!(lines.iter().any(|line| line.contains("3. three")));
        assert!(!lines
            .windows(2)
            .any(|window| window[0].contains("1. one") && window[1].is_empty()));
        assert!(!lines
            .windows(2)
            .any(|window| window[0].contains("2. two") && window[1].is_empty()));
    }

    #[test]
    fn soft_breaks_preserve_table_like_lines_separately() {
        let lines = render_markdown_impl("intro line.\n| a | b |\n| c | d |", None);
        assert!(lines
            .iter()
            .any(|line| line.to_string().contains("intro line.")));
        assert!(lines.iter().any(|line| line.to_string().contains("a")));
        assert!(lines.iter().any(|line| line.to_string().contains("c")));
    }
}

fn render_table(
    lines: &mut Vec<Line<'static>>,
    rows: &[Vec<TableCell>],
    options: &MarkdownRenderOptions,
) {
    if rows.is_empty() {
        return;
    }

    let col_count = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; col_count];
    let mut min_widths = vec![3usize; col_count];
    for row in rows {
        for (j, cell) in row.iter().enumerate() {
            if j < col_count {
                let rendered = render_inline_nodes_as_lines_with_style(
                    &cell.content,
                    Style::default().fg(WHITE),
                    options,
                );
                let max_width = rendered.iter().map(line_display_width).max().unwrap_or(0);
                widths[j] = widths[j].max(max_width);
                min_widths[j] = min_widths[j].max(min_cell_width(&cell.content));
            }
        }
    }

    let available_width = options.max_width.unwrap_or(80).max(12);
    let border_overhead = 2 + col_count * 2 + col_count.saturating_sub(1);
    let cell_budget = available_width.saturating_sub(border_overhead);
    let total_ideal: usize = widths.iter().sum();
    let total_min: usize = min_widths.iter().sum();

    if total_ideal > cell_budget {
        if total_min <= cell_budget {
            let extra = cell_budget - total_min;
            let overflow_total = total_ideal - total_min;
            for index in 0..widths.len() {
                let overflow = widths[index].saturating_sub(min_widths[index]);
                let share = overflow
                    .saturating_mul(extra)
                    .checked_div(overflow_total)
                    .unwrap_or(0);
                widths[index] = min_widths[index] + share;
            }
        } else if total_min > 0 {
            let scaled = cell_budget as f32 / total_min as f32;
            for index in 0..widths.len() {
                widths[index] = ((min_widths[index] as f32 * scaled).floor() as usize).max(3);
            }
        }
    }

    for width in &mut widths {
        *width = (*width).max(3);
    }

    if rows.len() > 1 && should_render_table_vertically(rows, &widths, options) {
        render_vertical_table(lines, rows, options.max_width.unwrap_or(80).max(12));
        return;
    }

    let mut rendered_table = Vec::new();

    rendered_table.push(render_table_border_line(&widths, '┌', '┬', '┐'));

    if let Some(header) = rows.first() {
        render_table_row(&mut rendered_table, header, &widths, true, options);
        rendered_table.push(render_table_border_line(&widths, '├', '┼', '┤'));
    }

    for (row_index, row) in rows.iter().skip(1).enumerate() {
        render_table_row(&mut rendered_table, row, &widths, false, options);
        if row_index + 2 < rows.len() {
            rendered_table.push(render_table_border_line(&widths, '├', '┼', '┤'));
        }
    }

    rendered_table.push(render_table_border_line(&widths, '└', '┴', '┘'));

    if rendered_table
        .iter()
        .any(|line| line_display_width(line) > available_width.saturating_sub(TABLE_SAFETY_MARGIN))
    {
        render_vertical_table(lines, rows, available_width);
        return;
    }

    lines.extend(rendered_table);
}

fn render_table_row(
    lines: &mut Vec<Line<'static>>,
    row: &[TableCell],
    widths: &[usize],
    is_header: bool,
    options: &MarkdownRenderOptions,
) {
    let base_style = if is_header {
        Style::default().fg(WHITE).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(WHITE)
    };
    let rendered_cells: Vec<Vec<Line<'static>>> = row
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let cell_lines =
                render_inline_nodes_as_lines_with_style(&cell.content, base_style, options);
            wrap_cell_lines(cell_lines, widths.get(index).copied().unwrap_or(10))
        })
        .collect();
    let row_height = rendered_cells
        .iter()
        .map(|cell| cell.len())
        .max()
        .unwrap_or(1);

    for line_index in 0..row_height {
        let mut spans = vec![Span::styled("│", Style::default().fg(DIM))];
        for (col_index, cell_lines) in rendered_cells.iter().enumerate() {
            let width = widths.get(col_index).copied().unwrap_or(10);
            spans.push(Span::styled(" ", base_style));

            if let Some(cell_line) = cell_lines.get(line_index) {
                let cell_width = line_display_width(cell_line);
                spans.extend(cell_line.spans.clone());
                pad_line_to_width(&mut spans, cell_width, width, base_style);
            } else {
                spans.push(Span::styled(" ".repeat(width), base_style));
            }

            spans.push(Span::styled(" ", base_style));
            spans.push(Span::styled("│", Style::default().fg(DIM)));
        }
        lines.push(Line::from(spans));
    }
}

fn render_table_border_line(
    widths: &[usize],
    left: char,
    middle: char,
    right: char,
) -> Line<'static> {
    let mut content = String::new();
    content.push(left);
    for (index, width) in widths.iter().enumerate() {
        content.push_str(&"─".repeat(*width + 2));
        content.push(if index + 1 < widths.len() {
            middle
        } else {
            right
        });
    }
    Line::from(Span::styled(content, Style::default().fg(DIM)))
}

fn wrap_cell_lines(cell_lines: Vec<Line<'static>>, width: usize) -> Vec<Line<'static>> {
    manual_wrap(cell_lines, width as u16)
}

fn should_render_table_vertically(
    rows: &[Vec<TableCell>],
    widths: &[usize],
    options: &MarkdownRenderOptions,
) -> bool {
    rows.iter()
        .flat_map(|row| row.iter().enumerate())
        .map(|(index, cell)| {
            let base_style = Style::default().fg(WHITE);
            let rendered =
                render_inline_nodes_as_lines_with_style(&cell.content, base_style, options);
            wrap_cell_lines(rendered, widths.get(index).copied().unwrap_or(10)).len()
        })
        .max()
        .unwrap_or(1)
        > TABLE_MAX_ROW_LINES
}

fn render_vertical_table(
    lines: &mut Vec<Line<'static>>,
    rows: &[Vec<TableCell>],
    max_width: usize,
) {
    if rows.is_empty() {
        return;
    }

    let headers = rows
        .first()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, cell)| {
            let label = inline_nodes_to_plain_text(&cell.content).trim().to_string();
            if label.is_empty() {
                format!("Column {}", index + 1)
            } else {
                label
            }
        })
        .collect::<Vec<_>>();
    let separator_width = max_width.saturating_sub(1).clamp(3, 40);
    let separator = Line::from(Span::styled(
        "─".repeat(separator_width),
        Style::default().fg(DIM),
    ));
    let continuation_prefix = "  ";
    let continuation_width = max_width.saturating_sub(continuation_prefix.len()).max(10);

    for (row_index, row) in rows.iter().enumerate().skip(1) {
        if row_index > 1 {
            lines.push(separator.clone());
        }

        for (col_index, cell) in row.iter().enumerate() {
            let label = headers
                .get(col_index)
                .cloned()
                .unwrap_or_else(|| format!("Column {}", col_index + 1));
            let label_width = UnicodeWidthStr::width(label.as_str());
            let first_line_width = max_width.saturating_sub(label_width + 2).max(10);
            let value_lines = wrap_cell_lines(
                render_inline_nodes_as_lines_with_style(
                    &cell.content,
                    Style::default().fg(WHITE),
                    &MarkdownRenderOptions {
                        max_width: Some(first_line_width),
                        enable_hyperlinks: false,
                    },
                ),
                first_line_width,
            );

            let mut first_spans = vec![Span::styled(
                format!("{}:", label),
                Style::default().fg(WHITE).add_modifier(Modifier::BOLD),
            )];
            if let Some(first_value) = value_lines.first() {
                first_spans.push(Span::raw(" "));
                first_spans.extend(first_value.spans.clone());
            }
            lines.push(Line::from(first_spans));

            for continuation in value_lines.iter().skip(1) {
                let wrapped = wrap_cell_lines(vec![continuation.clone()], continuation_width);
                for line in wrapped {
                    lines.push(prepend_prefix(
                        line,
                        continuation_prefix.to_string(),
                        Style::default(),
                    ));
                }
            }
        }
    }
}

fn min_cell_width(nodes: &[InlineNode]) -> usize {
    let text = inline_nodes_to_plain_text(nodes);
    text.split_whitespace()
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(3)
        .max(3)
}

fn render_inline_code_spans(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for token in tokenize_code_line_with_language(text, CodeLanguage::Plain) {
        spans.push(Span::styled(
            token.text,
            Style::default()
                .fg(inline_code_token_color(token.kind))
                .bg(INLINE_CODE_BG),
        ));
    }
    if spans.is_empty() {
        spans.push(Span::styled(
            String::new(),
            Style::default().fg(YELLOW).bg(INLINE_CODE_BG),
        ));
    }
    spans
}

fn inline_code_token_color(kind: CodeTokenKind) -> Color {
    match kind {
        CodeTokenKind::Plain => YELLOW,
        _ => code_token_color(kind, CodeLanguage::Plain),
    }
}
