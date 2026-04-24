use crate::app::rendering::is_code_block_line;
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

#[cfg(test)]
mod tests {
    use super::{user_plain_lines, PlainChatLine};
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
}
