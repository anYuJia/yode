use crate::app::rendering::CodeLanguage;
use ratatui::style::Color;

#[derive(Debug, Clone, Copy, Default)]
pub struct MarkdownRenderOptions {
    pub max_width: Option<usize>,
    pub enable_hyperlinks: bool,
}

#[derive(Debug, Clone)]
pub enum MarkdownBlock {
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
pub struct ListItem {
    pub task_state: Option<bool>,
    pub blocks: Vec<MarkdownBlock>,
}

#[derive(Debug, Clone)]
pub struct TableCell {
    pub content: Vec<InlineNode>,
}

#[derive(Debug, Clone)]
pub enum InlineNode {
    Text(String),
    Strong(Vec<InlineNode>),
    Emphasis(Vec<InlineNode>),
    Code(String),
    Link { text: Vec<InlineNode>, url: String },
    SoftBreak,
    HardBreak,
}

#[derive(Clone, Copy)]
pub enum ContainerEnd {
    BlockQuote,
}

#[derive(Clone, Copy)]
pub enum InlineEnd {
    Paragraph,
    Heading,
    TableCell,
    Strong,
    Emphasis,
    Link,
    Image,
}
