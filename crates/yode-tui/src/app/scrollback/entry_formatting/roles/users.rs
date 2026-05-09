use crate::app::rendering::highlight_code_line;
use crate::app::rendering::strip_ansi;
use crate::app::ChatEntry;
use crate::ui::chat::{render_markdown_ansi_white_with_options, WHITE};
use crate::ui::chat_entries::compact_assistant_display_markdown;
use crate::ui::chat_entries::user_plain_lines;

use super::style::scrollback_render_width;

pub(super) fn render_user(
    entry: &ChatEntry,
    result: &mut Vec<(String, ratatui::style::Style)>,
    cyan: ratatui::style::Style,
) {
    for (index, line) in user_plain_lines(entry).into_iter().enumerate() {
        let style = if index == 0 {
            cyan.add_modifier(ratatui::style::Modifier::BOLD)
        } else {
            cyan
        };
        let content = if line.highlight_code {
            highlight_code_line(&line.content)
        } else {
            line.content
        };
        result.push((format!("{}{}", line.prefix, content), style));
    }
}

pub(super) fn render_assistant(
    entry: &ChatEntry,
    result: &mut Vec<(String, ratatui::style::Style)>,
    dim: ratatui::style::Style,
    _white: ratatui::style::Style,
    _show_reasoning_detail: bool,
) {
    result.push((String::new(), dim));
    let render_width = scrollback_render_width(2, 78);
    let compacted = compact_assistant_display_markdown(&entry.content);
    let lines = render_markdown_ansi_white_with_options(&compacted.text, Some(render_width), true);
    let mut first_content = true;
    let mut previous_blank = false;
    for line in lines.iter() {
        if strip_ansi(line).trim().is_empty() {
            if compacted.was_compacted && previous_blank {
                continue;
            }
            result.push((String::new(), dim));
            previous_blank = true;
            continue;
        }
        let prefix = if first_content { "⏺ " } else { "  " };
        result.push((
            format!("{}{}", prefix, line),
            ratatui::style::Style::default().fg(WHITE),
        ));
        first_content = false;
        previous_blank = false;
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Style;

    use crate::app::rendering::strip_ansi;
    use crate::app::{ChatEntry, ChatRole};

    use super::render_assistant;

    #[test]
    fn scrollback_assistant_preserves_project_reports() {
        let entry = ChatEntry::new(
            ChatRole::Assistant,
            "Yode vs Claude Code 综合对比\n基本面\n维度 │ Yode │ Claude Code\n──────┼──────┼─────────────\n语言 │ Rust │ TypeScript\n核心差距 (按影响程度排序)\nP0 - 严重缺失 (阻塞日常使用)\n1. 命令系统缺陷"
                .to_string(),
        );
        let mut result = Vec::new();
        render_assistant(
            &entry,
            &mut result,
            Style::default(),
            Style::default(),
            true,
        );

        assert!(result
            .iter()
            .any(|(line, _)| line.contains("P0 - 严重缺失")));
        assert!(result.iter().any(|(line, _)| line.contains("维度")));
        assert!(result.iter().all(|(line, _)| !line.contains("折叠")));
    }

    #[test]
    fn scrollback_assistant_normalizes_pasted_analysis_sample() {
        let entry = ChatEntry::new(
            ChatRole::Assistant,
            "Yode vs Claude Code 综合对比\n基本面\n 维度 │ Yode           │ Claude Code          \n──────┼────────────────┼──────────────────────\n 语言 │ Rust (~15万行) │ TypeScript (~52万行) \n| 工具数 | ~45 | ~50+ |\n| 命令数 | ~30 | ~80+ |\n| MCP | rmcp (基础) | 完整SDK (SSE/Stdio/HTTP) |\n核心差距 (优先级排序)\nP0 - 严重缺失\n1. 命令系统缺陷"
                .to_string(),
        );
        let mut result = Vec::new();
        render_assistant(
            &entry,
            &mut result,
            Style::default(),
            Style::default(),
            true,
        );

        let rendered = result
            .iter()
            .map(|(line, _)| strip_ansi(line))
            .collect::<Vec<_>>();
        assert!(rendered.iter().all(|line| !line.contains("###")));
        assert!(rendered
            .iter()
            .all(|line| !line.contains("| 工具数 | ~45 | ~50+ |")));
    }

    #[test]
    fn scrollback_assistant_normalizes_loose_ascii_pipe_tables() {
        let entry = ChatEntry::new(
            ChatRole::Assistant,
            "架构对比要点\n维度 | Yode | Claude Code |\n命令注册 | 全量静态编译时 | 懒加载 + 运行时动态 |\nUI 渲染 | 纯文本 | React JSX (交互) |"
                .to_string(),
        );
        let mut result = Vec::new();
        render_assistant(
            &entry,
            &mut result,
            Style::default(),
            Style::default(),
            true,
        );

        let rendered = result
            .iter()
            .map(|(line, _)| strip_ansi(line))
            .collect::<Vec<_>>();
        assert!(rendered
            .iter()
            .all(|line| !line.contains("维度 | Yode | Claude Code |")));
        assert!(rendered
            .iter()
            .all(|line| !line.contains("命令注册 | 全量静态编译时 | 懒加载 + 运行时动态 |")));
    }

    #[test]
    fn scrollback_assistant_hides_reasoning_in_main_transcript() {
        let entry = ChatEntry::new_with_reasoning(
            ChatRole::Assistant,
            "final answer".to_string(),
            Some("## Plan\n- inspect\n- patch".to_string()),
        );
        let mut result = Vec::new();
        render_assistant(
            &entry,
            &mut result,
            Style::default(),
            Style::default(),
            true,
        );

        let rendered = result
            .iter()
            .map(|(line, _)| strip_ansi(line))
            .collect::<Vec<_>>();
        assert!(rendered.iter().any(|line| line.contains("final answer")));
        assert!(rendered.iter().all(|line| !line.contains("Thinking")));
        assert!(rendered.iter().all(|line| !line.contains("Plan")));
        assert!(rendered.iter().all(|line| !line.contains("• inspect")));
    }

    #[test]
    fn scrollback_assistant_content_does_not_append_inspection_hint() {
        let entry = ChatEntry::new(ChatRole::Assistant, "Final answer".to_string());
        let mut result = Vec::new();
        render_assistant(
            &entry,
            &mut result,
            Style::default(),
            Style::default(),
            false,
        );

        let rendered = result
            .iter()
            .map(|(line, _)| strip_ansi(line))
            .collect::<Vec<_>>();
        assert!(rendered.iter().any(|line| line == "⏺ Final answer"));
    }

    #[test]
    fn scrollback_long_assistant_content_is_not_hidden_by_generic_line_cap() {
        let content = (1..=24)
            .map(|index| format!("第{}行分析内容", index))
            .collect::<Vec<_>>()
            .join("\n");
        let entry = ChatEntry::new(ChatRole::Assistant, content);
        let mut result = Vec::new();
        render_assistant(
            &entry,
            &mut result,
            Style::default(),
            Style::default(),
            true,
        );

        let rendered = result
            .iter()
            .map(|(line, _)| strip_ansi(line))
            .collect::<Vec<_>>();
        assert!(rendered.iter().any(|line| line.contains("第1行分析内容")));
        assert!(rendered.iter().any(|line| line.contains("第24行分析内容")));
        assert!(rendered
            .iter()
            .all(|line| !line.contains("more lines (ctrl+o to inspect)")));
    }

    #[test]
    fn scrollback_compacted_assistant_does_not_emit_repeated_blank_lines() {
        let entry = ChatEntry::new(
            ChatRole::Assistant,
            "Yode vs Claude Code 深度对比分析\n\n一、项目规模与技术栈\n\n| 维度 | Yode | Claude Code |\n| --- | --- | --- |\n| MCP | stdio | SSE/HTTP/WS |\n\n\n\n二、关键差距\n\n1. 上下文压缩\n2. MCP transport\n\n\n\n优化建议\n\n1. 先修输出格式\n2. 再补 transport\n"
                .to_string(),
        );
        let mut result = Vec::new();
        render_assistant(
            &entry,
            &mut result,
            Style::default(),
            Style::default(),
            true,
        );

        let rendered = result
            .iter()
            .map(|(line, _)| strip_ansi(line))
            .collect::<Vec<_>>();
        assert!(rendered
            .windows(2)
            .all(|pair| !(pair[0].trim().is_empty() && pair[1].trim().is_empty())));
    }

    #[test]
    fn scrollback_older_assistant_reasoning_is_hidden() {
        let entry = ChatEntry::new_with_reasoning(
            ChatRole::Assistant,
            "final answer".to_string(),
            Some("## Plan\n- inspect\n- patch".to_string()),
        );
        let mut result = Vec::new();
        render_assistant(
            &entry,
            &mut result,
            Style::default(),
            Style::default(),
            false,
        );

        let rendered = result
            .iter()
            .map(|(line, _)| strip_ansi(line))
            .collect::<Vec<_>>();
        assert!(rendered.iter().all(|line| !line.contains("Thinking")));
        assert!(rendered.iter().all(|line| !line.contains("• inspect")));
        assert!(rendered.iter().any(|line| line.contains("final answer")));
    }
}
