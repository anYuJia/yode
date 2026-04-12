use crate::app::rendering::{is_code_block_line, markdown_to_plain};
use crate::app::ChatEntry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlainChatLine {
    pub prefix: &'static str,
    pub content: String,
    pub highlight_code: bool,
}

pub(crate) fn user_plain_lines(entry: &ChatEntry) -> Vec<PlainChatLine> {
    let mut result = entry
        .content
        .lines()
        .enumerate()
        .map(|(index, line)| PlainChatLine {
            prefix: if index == 0 { "> " } else { "  " },
            content: line.to_string(),
            highlight_code: is_code_block_line(line),
        })
        .collect::<Vec<_>>();
    if result.is_empty() {
        result.push(PlainChatLine {
            prefix: "> ",
            content: String::new(),
            highlight_code: false,
        });
    }
    result
}

pub(crate) fn assistant_plain_lines(entry: &ChatEntry) -> Vec<PlainChatLine> {
    let processed = markdown_to_plain(&entry.content);
    if processed.trim().is_empty() {
        return Vec::new();
    }

    let mut first_content = true;
    let mut result = Vec::new();
    for line in processed.lines() {
        if line.trim().is_empty() {
            result.push(PlainChatLine {
                prefix: "",
                content: String::new(),
                highlight_code: false,
            });
            continue;
        }

        let prefix = if first_content { "⏺ " } else { "  " };
        result.push(PlainChatLine {
            prefix,
            content: line.to_string(),
            highlight_code: !first_content && is_code_block_line(line),
        });
        first_content = false;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{assistant_plain_lines, user_plain_lines, PlainChatLine};
    use crate::app::{ChatEntry, ChatRole};

    #[test]
    fn user_plain_lines_mark_indented_code() {
        let entry = ChatEntry::new(ChatRole::User, "hello\n    let x = 1;".to_string());

        assert_eq!(
            user_plain_lines(&entry),
            vec![
                PlainChatLine {
                    prefix: "> ",
                    content: "hello".to_string(),
                    highlight_code: false,
                },
                PlainChatLine {
                    prefix: "  ",
                    content: "    let x = 1;".to_string(),
                    highlight_code: true,
                },
            ]
        );
    }

    #[test]
    fn assistant_plain_lines_mark_code_after_first_line() {
        let entry = ChatEntry::new(
            ChatRole::Assistant,
            "Here is the fix:\n```rust\nfn main() {}\n```".to_string(),
        );

        assert_eq!(
            assistant_plain_lines(&entry),
            vec![
                PlainChatLine {
                    prefix: "⏺ ",
                    content: "Here is the fix:".to_string(),
                    highlight_code: false,
                },
                PlainChatLine {
                    prefix: "  ",
                    content: "─── rust ───".to_string(),
                    highlight_code: true,
                },
                PlainChatLine {
                    prefix: "  ",
                    content: "    fn main() {}".to_string(),
                    highlight_code: true,
                },
            ]
        );
    }
}
