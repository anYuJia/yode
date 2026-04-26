use crate::app::rendering::highlight_code_line;
use crate::app::ChatEntry;
use crate::ui::chat::{
    render_markdown_ansi_dim_with_options, render_markdown_ansi_white_with_options, WHITE,
};
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
    show_reasoning_detail: bool,
) {
    if let Some(reasoning) = &entry.reasoning {
        if !reasoning.trim().is_empty() {
            result.push((
                "  ∴ Thinking… (ctrl+o to inspect)".to_string(),
                dim.add_modifier(ratatui::style::Modifier::ITALIC),
            ));
            if show_reasoning_detail {
                let render_width = scrollback_render_width(4, 76);
                let lines = render_markdown_ansi_dim_with_options(
                    reasoning.trim(),
                    Some(render_width),
                    true,
                );
                for line in lines {
                    if line.trim().is_empty() {
                        result.push((String::new(), dim));
                    } else {
                        result.push((
                            format!("  {}", line),
                            dim.add_modifier(ratatui::style::Modifier::ITALIC),
                        ));
                    }
                }
                result.push((String::new(), dim));
            }
        }
    }

    result.push((String::new(), dim));
    let render_width = scrollback_render_width(2, 78);
    let lines = render_markdown_ansi_white_with_options(&entry.content, Some(render_width), true);
    let mut first_content = true;
    for line in lines {
        if line.trim().is_empty() {
            result.push((String::new(), dim));
            continue;
        }
        let prefix = if first_content { "⏺ " } else { "  " };
        result.push((
            if first_content {
                format!("{}{} (ctrl+o to inspect)", prefix, line)
            } else {
                format!("{}{}", prefix, line)
            },
            ratatui::style::Style::default().fg(WHITE),
        ));
        first_content = false;
    }
}

#[cfg(test)]
mod tests {
    use ratatui::style::Style;

    use crate::app::rendering::strip_ansi;
    use crate::app::{ChatEntry, ChatRole};

    use super::render_assistant;

    #[test]
    fn scrollback_assistant_keeps_blank_lines_for_promoted_sections() {
        let entry = ChatEntry::new(
            ChatRole::Assistant,
            "Yode vs Claude Code 综合对比\n基本面\n维度 │ Yode │ Claude Code\n──────┼──────┼─────────────\n语言 │ Rust │ TypeScript\n核心差距 (按影响程度排序)\nP0 - 严重缺失 (阻塞日常使用)\n1. 命令系统缺陷"
                .to_string(),
        );
        let mut result = Vec::new();
        render_assistant(&entry, &mut result, Style::default(), Style::default(), true);

        let basic_index = result
            .iter()
            .position(|(line, _)| line.contains("基本面"))
            .unwrap();
        assert!(basic_index > 0);
        assert!(result[basic_index - 1].0.is_empty());

        let p0_index = result
            .iter()
            .position(|(line, _)| line.contains("P0 - 严重缺失"))
            .unwrap();
        assert!(p0_index > 0);
        assert!(result[p0_index - 1].0.is_empty());
    }

    #[test]
    fn scrollback_assistant_normalizes_pasted_analysis_sample() {
        let entry = ChatEntry::new(
            ChatRole::Assistant,
            "Yode vs Claude Code 综合对比\n基本面\n 维度 │ Yode           │ Claude Code          \n──────┼────────────────┼──────────────────────\n 语言 │ Rust (~15万行) │ TypeScript (~52万行) \n| 工具数 | ~45 | ~50+ |\n| 命令数 | ~30 | ~80+ |\n| MCP | rmcp (基础) | 完整SDK (SSE/Stdio/HTTP) |\n核心差距 (优先级排序)\nP0 - 严重缺失\n1. 命令系统缺陷"
                .to_string(),
        );
        let mut result = Vec::new();
        render_assistant(&entry, &mut result, Style::default(), Style::default(), true);

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
        render_assistant(&entry, &mut result, Style::default(), Style::default(), true);

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
    fn scrollback_assistant_includes_reasoning_markdown() {
        let entry = ChatEntry::new_with_reasoning(
            ChatRole::Assistant,
            "final answer".to_string(),
            Some("## Plan\n- inspect\n- patch".to_string()),
        );
        let mut result = Vec::new();
        render_assistant(&entry, &mut result, Style::default(), Style::default(), true);

        let rendered = result
            .iter()
            .map(|(line, _)| strip_ansi(line))
            .collect::<Vec<_>>();
        assert!(rendered
            .iter()
            .any(|line| line.contains("∴ Thinking… (ctrl+o to inspect)")));
        assert!(rendered
            .iter()
            .any(|line| line.contains("final answer (ctrl+o to inspect)")));
        assert!(rendered.iter().any(|line| line.contains("Plan")));
        assert!(rendered.iter().any(|line| line.contains("• inspect")));
    }

    #[test]
    fn scrollback_assistant_content_advertises_detail_inspection() {
        let entry = ChatEntry::new(ChatRole::Assistant, "Final answer".to_string());
        let mut result = Vec::new();
        render_assistant(&entry, &mut result, Style::default(), Style::default(), false);

        let rendered = result
            .iter()
            .map(|(line, _)| strip_ansi(line))
            .collect::<Vec<_>>();
        assert!(rendered
            .iter()
            .any(|line| line.contains("Final answer (ctrl+o to inspect)")));
    }

    #[test]
    fn scrollback_older_assistant_reasoning_can_collapse_to_teaser_only() {
        let entry = ChatEntry::new_with_reasoning(
            ChatRole::Assistant,
            "final answer".to_string(),
            Some("## Plan\n- inspect\n- patch".to_string()),
        );
        let mut result = Vec::new();
        render_assistant(&entry, &mut result, Style::default(), Style::default(), false);

        let rendered = result
            .iter()
            .map(|(line, _)| strip_ansi(line))
            .collect::<Vec<_>>();
        assert!(rendered
            .iter()
            .any(|line| line.contains("Thinking… (ctrl+o to inspect)")));
        assert!(rendered.iter().all(|line| !line.contains("• inspect")));
    }
}
