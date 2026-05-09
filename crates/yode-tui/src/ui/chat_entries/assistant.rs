use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::app::rendering::strip_ansi;
use crate::app::ChatEntry;
use crate::ui::chat::{ACCENT, WHITE};
use crate::ui::chat_markdown::{render_markdown_with_options, MarkdownRenderOptions};

use super::assistant_compact::compact_assistant_display_markdown;

// Claude Code style: ⏺ prefix on first line, indented continuation
pub(crate) fn render_assistant(
    lines: &mut Vec<Line<'static>>,
    entry: &ChatEntry,
    max_width: usize,
    enable_hyperlinks: bool,
    _show_reasoning_detail: bool,
) {
    let compacted = compact_assistant_display_markdown(&entry.content);
    let markdown = render_markdown_with_options(
        &compacted.text,
        Some(WHITE),
        MarkdownRenderOptions {
            max_width: Some(max_width),
            enable_hyperlinks,
        },
    );
    let mut previous_blank = false;
    for (index, line) in markdown.into_iter().enumerate() {
        if is_visually_blank_line(&line) {
            if compacted.was_compacted && previous_blank {
                continue;
            }
            lines.push(Line::from(""));
            previous_blank = true;
            continue;
        }

        let mut spans = Vec::new();
        if index == 0 {
            spans.push(Span::styled("⏺ ", Style::default().fg(ACCENT)));
        } else {
            spans.push(Span::raw("  "));
        }
        spans.extend(line.spans);
        lines.push(Line::from(spans));
        previous_blank = false;
    }
}

fn is_visually_blank_line(line: &Line<'static>) -> bool {
    if line.spans.is_empty() {
        return true;
    }
    line.spans
        .iter()
        .all(|span| strip_ansi(span.content.as_ref()).trim().is_empty())
}

#[cfg(test)]
mod tests {
    use ratatui::text::Line;

    use crate::app::{ChatEntry, ChatRole};

    use super::render_assistant;

    fn render_lines(content: &str) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let entry = ChatEntry::new(ChatRole::Assistant, content.to_string());
        render_assistant(&mut lines, &entry, 120, false, true);
        lines
    }

    #[test]
    fn assistant_render_preserves_project_comparison_reports() {
        let sample = "根据已有的深度分析记忆，我直接给你综合结论，不需要重新扫描。\nYode vs Claude Code 综合对比\n基本面\n维度 │ Yode │ Claude Code\n──────┼──────┼─────────────\n语言 │ Rust (~15万行) │ TypeScript (~52万行)\n工具数 │ ~45 │ ~50+\n命令数 │ ~30 │ ~80+\nMCP Transport │ 仅 Stdio │ 7种 (sse/http/ws/sdk等)\n5 大核心差距（按影响排序）\n1. MCP 客户端 — 严重不足\n• 缺 SSE/HTTP/WS transport，无法连远程 MCP 服务器\n2. 上下文压缩 — 单层 vs 七层\n• Yode：单层 eviction + 本地模板 summary（1.2K chars）\n3. 命令系统 — 缺少 prompt 类型命令\n• Yode 只有同步 Command trait，CC 有 prompt/local/local-jsx 三种\n优化建议（按 ROI 排序）\n🔴 P0 — 不做会严重影响可用性\n1. LLM 生成 summary 替代本地模板\nYode 的优势（Rust 带来的）\n• 性能：启动快、内存小、无 GC 停顿\n建议优先做 P0 的 1-3（LLM summary + SSE transport + prompt 命令），这三个投入产出比最高。";
        let lines = render_lines(sample)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(lines
            .iter()
            .any(|line| line.contains("Yode vs Claude Code 综合对比")));
        assert!(lines.iter().any(|line| line.contains("1. MCP 客户端")));
        assert!(lines.iter().any(|line| line.contains("2. 上下文压缩")));
        assert!(lines.iter().any(|line| line.contains("3. 命令系统")));
        assert!(lines.iter().any(|line| line.contains("LLM 生成 summary")));
        assert!(lines.iter().any(|line| line.contains("性能：启动快")));
        assert!(lines.iter().any(|line| line.contains("工具数")));
        assert!(lines
            .iter()
            .all(|line| !line.contains("其余展开说明已折叠")));
    }

    #[test]
    fn assistant_reasoning_is_hidden_in_main_chat() {
        let mut lines = Vec::new();
        let entry = ChatEntry::new_with_reasoning(
            ChatRole::Assistant,
            "final answer".to_string(),
            Some("## Plan\n- inspect\n- patch".to_string()),
        );
        render_assistant(&mut lines, &entry, 120, false, true);

        assert!(lines
            .iter()
            .all(|line| !line.to_string().contains("Thinking")
                && !line.to_string().contains("Plan")
                && !line.to_string().contains("• inspect")));
        assert!(lines
            .iter()
            .any(|line| line.to_string().contains("final answer")));
    }

    #[test]
    fn assistant_content_does_not_append_inspection_hint_to_body() {
        let lines = render_lines("Final answer");
        assert_eq!(lines[0].to_string(), "⏺ Final answer");
    }

    #[test]
    fn older_assistant_reasoning_is_hidden_in_main_chat() {
        let mut lines = Vec::new();
        let entry = ChatEntry::new_with_reasoning(
            ChatRole::Assistant,
            "final answer".to_string(),
            Some("## Plan\n- inspect\n- patch".to_string()),
        );
        render_assistant(&mut lines, &entry, 120, false, false);

        let rendered = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(rendered.iter().all(|line| !line.contains("Thinking")));
        assert!(rendered.iter().all(|line| !line.contains("• inspect")));
        assert!(rendered.iter().any(|line| line.contains("final answer")));
    }
}
