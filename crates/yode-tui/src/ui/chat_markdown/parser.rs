use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::sync::LazyLock;

use super::types::{ContainerEnd, InlineEnd, InlineNode, ListItem, MarkdownBlock, TableCell};
use super::utils::{
    heading_level_to_usize, inline_nodes_to_plain_text, is_container_end, is_inline_end,
    is_inline_event, is_inline_start_tag, is_list_end,
};
use crate::app::rendering::parse_code_language;

static COMPOUND_TABLE_ROW_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\|\s+\|").unwrap());

pub fn parse_markdown_blocks(text: &str) -> Vec<MarkdownBlock> {
    let normalized = normalize_markdown_input(text);
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let events: Vec<Event<'_>> = Parser::new_ext(&normalized, options).collect();
    let mut index = 0;
    parse_blocks_until(&events, &mut index, None)
}

pub fn normalize_markdown_input(text: &str) -> String {
    promote_heading_lines(collapse_list_blank_lines(normalize_structural_lines(text))).join("\n")
}

pub fn normalize_structural_lines(text: &str) -> Vec<String> {
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

pub fn strip_structural_indent(raw_line: &str) -> String {
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

pub fn looks_like_list_line(trimmed: &str) -> bool {
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

pub fn normalize_unicode_bullet_line(trimmed: &str) -> Option<String> {
    for marker in ["• ", "◦ ", "▪ "] {
        if let Some(rest) = trimmed.strip_prefix(marker) {
            return Some(format!("- {}", rest.trim_start()));
        }
    }
    None
}

pub fn try_join_table_continuation(lines: &mut Vec<String>, trimmed: &str) -> bool {
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

pub fn looks_like_structural_line(trimmed: &str) -> bool {
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

pub fn normalize_unicode_table_row(trimmed: &str) -> Option<String> {
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

pub fn normalize_unicode_table_separator(trimmed: &str) -> Option<String> {
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

pub fn normalize_ascii_pipe_table_row(trimmed: &str) -> Option<String> {
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

pub fn insert_missing_table_separator_lines(lines: Vec<String>) -> Vec<String> {
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

pub fn collapse_list_blank_lines(lines: Vec<String>) -> Vec<String> {
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

        if super::utils::is_streaming_list_item_line(previous)
            && super::utils::is_streaming_list_item_line(next)
        {
            continue;
        }

        normalized.push(line.clone());
    }
    normalized
}

pub fn is_markdown_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.matches('|').count() >= 2
}

pub fn is_markdown_table_separator(line: &str) -> bool {
    let trimmed = line.trim().trim_matches('|');
    !trimmed.is_empty()
        && trimmed.split('|').map(str::trim).all(|cell| {
            !cell.is_empty() && cell.chars().all(|ch| ch == '-' || ch == ':' || ch == ' ')
        })
}

pub fn markdown_table_separator_for_row(line: &str) -> String {
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

pub fn expand_compound_table_rows(raw_line: &str) -> Vec<String> {
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

pub fn promote_heading_lines(lines: Vec<String>) -> Vec<String> {
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

pub fn classify_promoted_heading_level(trimmed: &str, next: Option<&str>) -> Option<usize> {
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

pub fn looks_like_heading_candidate(trimmed: &str) -> bool {
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

pub fn looks_like_heading_followup(next: &str) -> bool {
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

pub fn is_chinese_section_heading(trimmed: &str) -> bool {
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

pub fn is_priority_heading(trimmed: &str) -> bool {
    trimmed.starts_with('P')
        && trimmed.chars().nth(1).is_some_and(|ch| ch.is_ascii_digit())
        && (trimmed.contains("核心")
            || trimmed.contains("缺失")
            || trimmed.contains("增强")
            || trimmed.contains("优先"))
}

pub fn looks_like_numbered_section_heading(trimmed: &str, next: Option<&str>) -> bool {
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

pub fn looks_like_numbered_section_followup(next: &str) -> bool {
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

pub fn parse_blocks_until(
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

pub fn parse_list_block(
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

pub fn parse_item_blocks(events: &[Event<'_>], index: &mut usize) -> Vec<MarkdownBlock> {
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

pub fn parse_code_fence_block(
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

pub fn parse_table_block(events: &[Event<'_>], index: &mut usize) -> MarkdownBlock {
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

pub fn parse_table_row(events: &[Event<'_>], index: &mut usize, is_header: bool) -> Vec<TableCell> {
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

pub fn parse_inline_nodes_until(
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

pub fn parse_inline_nodes_until_item_boundary(
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
