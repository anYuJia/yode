use pulldown_cmark::{Event, HeadingLevel, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use regex::Regex;
use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{LazyLock, Mutex};
use unicode_width::UnicodeWidthStr;

use super::types::{
    ContainerEnd, InlineEnd, InlineNode, ListItem, MarkdownBlock, MarkdownRenderOptions, TableCell,
};
use crate::app::rendering::{
    parse_code_language, tokenize_code_line_with_language, CodeLanguage, CodeTokenKind,
};
use crate::ui::chat::{CODE_BG, DIM, INLINE_CODE_BG, WHITE, YELLOW};
use crate::ui::chat_layout::{manual_wrap, osc8_close_sequence, visible_text_width};
use crate::ui::highlighted_code::render_highlighted_code_block;
use crate::ui::palette::{BORDER_MUTED, INFO_COLOR, LIGHT, MUTED, PANEL_ACCENT};
use crate::ui::structured_diff::render_structured_diff_block;

pub const MARKDOWN_BLOCK_CACHE_MAX: usize = 500;
pub const TABLE_MAX_ROW_LINES: usize = 4;

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

pub(crate) static ISSUE_REF_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|[^\w./-])([A-Za-z0-9][\w-]*/[A-Za-z0-9][\w.-]*)#(\d+)\b").unwrap()
});

pub(crate) static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s<>"']+"#).unwrap());

pub fn has_markdown_syntax(text: &str) -> bool {
    MD_SYNTAX_RE.is_match(text)
}

pub fn cached_markdown_blocks(text: &str) -> Vec<MarkdownBlock> {
    let key = hash_markdown_text(text);
    if let Ok(mut cache) = MARKDOWN_BLOCK_CACHE.lock() {
        if let Some(blocks) = cache.entries.get(&key).cloned() {
            cache.order.retain(|existing| *existing != key);
            cache.order.push_back(key);
            return blocks;
        }
    }

    let blocks = super::parser::parse_markdown_blocks(text);

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

pub fn render_plain_text_lines(
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

pub fn stable_boundary_from_complete_lines(text: &str) -> usize {
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

        if super::parser::looks_like_heading_candidate(trimmed) {
            let next = lines.get(index + 1).map(|line| line.2.as_str());
            if next.is_none() {
                break;
            }
            if next.is_some_and(super::parser::looks_like_heading_followup) {
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

pub fn is_streaming_table_line(trimmed: &str) -> bool {
    super::parser::is_markdown_table_row(trimmed)
        || super::parser::is_markdown_table_separator(trimmed)
        || super::parser::normalize_unicode_table_row(trimmed).is_some()
        || super::parser::normalize_unicode_table_separator(trimmed).is_some()
        || super::parser::normalize_ascii_pipe_table_row(trimmed).is_some()
}

pub fn is_streaming_list_item_line(trimmed: &str) -> bool {
    trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("• ")
        || trimmed.starts_with("◦ ")
        || trimmed.starts_with("▪ ")
        || trimmed.split_once(". ").is_some_and(|(prefix, _)| {
            !prefix.is_empty() && prefix.chars().all(|ch| ch.is_ascii_digit())
        })
}

pub fn line_to_ansi_string(line: &Line<'static>) -> String {
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

pub fn inline_nodes_to_plain_text(nodes: &[InlineNode]) -> String {
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

pub fn is_container_end(tag_end: &TagEnd, end: ContainerEnd) -> bool {
    matches!(
        (tag_end, end),
        (TagEnd::BlockQuote(_), ContainerEnd::BlockQuote)
    )
}

pub fn is_list_end(tag_end: &TagEnd) -> bool {
    matches!(tag_end, TagEnd::List(_))
}

pub fn is_inline_end(tag_end: &TagEnd, end: InlineEnd) -> bool {
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

pub fn is_inline_start_tag(tag: &Tag<'_>) -> bool {
    matches!(
        tag,
        Tag::Strong | Tag::Emphasis | Tag::Link { .. } | Tag::Image { .. }
    )
}

pub fn is_inline_event(event: &Event<'_>) -> bool {
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

pub fn heading_level_to_usize(level: HeadingLevel) -> usize {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

pub fn number_to_letter(mut number: u64) -> String {
    let mut result = String::new();
    while number > 0 {
        number -= 1;
        result.insert(0, char::from(b'a' + (number % 26) as u8));
        number /= 26;
    }
    result
}

pub fn number_to_roman(mut number: u64) -> String {
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

pub fn compact_table_cell_text(nodes: &[InlineNode]) -> String {
    inline_nodes_to_plain_text(nodes)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn min_cell_width(nodes: &[InlineNode]) -> usize {
    let text = inline_nodes_to_plain_text(nodes);
    text.split_whitespace()
        .map(UnicodeWidthStr::width)
        .max()
        .unwrap_or(3)
        .max(3)
}

pub fn render_inline_code_spans(text: &str) -> Vec<Span<'static>> {
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

pub fn code_token_color(kind: CodeTokenKind, language: CodeLanguage) -> Color {
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

#[derive(Clone, Copy)]
pub struct CodeBlockTheme {
    pub badge_bg: Color,
    pub badge_fg: Color,
    pub border: Color,
    pub background: Color,
}

pub fn code_block_theme(language: CodeLanguage) -> CodeBlockTheme {
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

pub fn render_code_block_header(label: Option<&str>, language: CodeLanguage) -> Vec<Span<'static>> {
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

pub fn render_code_block_footer(language: CodeLanguage) -> Vec<Span<'static>> {
    let theme = code_block_theme(language);
    vec![Span::styled(
        "╰────────────────────────────────────",
        Style::default().fg(theme.border),
    )]
}

pub fn render_code_block(
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

pub fn line_display_width(line: &Line<'static>) -> usize {
    line.spans
        .iter()
        .map(|span| visible_text_width(&span.content))
        .sum()
}

pub fn pad_line_to_width(
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

pub fn prepend_prefix(line: Line<'static>, prefix: String, style: Style) -> Line<'static> {
    let mut spans = vec![Span::styled(prefix, style)];
    spans.extend(line.spans);
    Line::from(spans)
}

pub fn add_modifier_to_line(line: Line<'static>, modifier: Modifier) -> Line<'static> {
    let spans = line
        .spans
        .into_iter()
        .map(|span| Span::styled(span.content, span.style.add_modifier(modifier)))
        .collect::<Vec<_>>();
    Line::from(spans)
}

pub fn append_text_with_links(spans: &mut Vec<Span<'static>>, text: &str, style: Style) {
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

pub enum LinkMatch<'a> {
    Issue(regex::Captures<'a>, usize, usize),
    Url(usize, usize),
}

pub fn push_hyperlink_spans(spans: &mut Vec<Span<'static>>, text: &str, url: &str, style: Style) {
    let link_style = style.fg(INFO_COLOR).add_modifier(Modifier::UNDERLINED);
    spans.push(Span::raw(osc8_start_sequence(url)));
    spans.push(Span::styled(text.to_string(), link_style));
    spans.push(Span::raw(osc8_close_sequence()));
}

pub fn osc8_start_sequence(url: &str) -> String {
    format!("\x1b]8;;{}\x07", url)
}

pub fn ensure_blank_line(lines: &mut Vec<Line<'static>>) {
    if !lines.is_empty() && !is_blank_line(lines.last().unwrap()) {
        lines.push(Line::from(""));
    }
}

pub fn normalize_blank_lines(lines: &mut Vec<Line<'static>>) {
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

pub fn is_blank_line(line: &Line<'static>) -> bool {
    line.spans.is_empty() || line.spans.iter().all(|span| span.content.trim().is_empty())
}

#[derive(Clone, Copy)]
pub struct BlockSpacing {
    pub before: bool,
    pub after: bool,
}

pub fn block_spacing(block: &MarkdownBlock) -> BlockSpacing {
    match block {
        MarkdownBlock::Heading { level, .. } => BlockSpacing {
            before: true,
            after: *level <= 1,
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

pub fn keeps_following_block_tight(previous: &MarkdownBlock, current: &MarkdownBlock) -> bool {
    matches!(previous, MarkdownBlock::Heading { level, .. } if *level > 1)
        && !matches!(current, MarkdownBlock::Heading { .. })
}
