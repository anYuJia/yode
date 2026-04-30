use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::ChatEntry;
use crate::ui::chat::{ACCENT, DIM, WHITE};
use crate::ui::chat_markdown::{render_markdown_with_options, MarkdownRenderOptions};
use crate::ui::palette::{INFO_COLOR, PANEL_ACCENT};

// Claude Code style: ⏺ prefix on first line, indented continuation
pub(crate) fn render_assistant(
    lines: &mut Vec<Line<'static>>,
    entry: &ChatEntry,
    max_width: usize,
    enable_hyperlinks: bool,
    show_reasoning_detail: bool,
) {
    if let Some(reasoning) = &entry.reasoning {
        if !reasoning.trim().is_empty() {
            lines.push(Line::from(vec![Span::styled(
                if show_reasoning_detail {
                    "  ∴ Thinking… (ctrl+o to inspect)"
                } else {
                    "  ∴ Thinking hidden (ctrl+o to inspect)"
                },
                Style::default()
                    .fg(PANEL_ACCENT)
                    .add_modifier(Modifier::ITALIC | Modifier::BOLD),
            )]));

            if show_reasoning_detail {
                let reasoning_lines = render_markdown_with_options(
                    reasoning.trim(),
                    Some(DIM),
                    MarkdownRenderOptions {
                        max_width: Some(max_width.saturating_sub(2)),
                        enable_hyperlinks,
                    },
                );
                for line in reasoning_lines {
                    if line.spans.is_empty()
                        || (line.spans.len() == 1
                            && line
                                .spans
                                .first()
                                .is_some_and(|span| span.content.is_empty()))
                    {
                        lines.push(Line::from(""));
                        continue;
                    }
                    let mut spans = vec![Span::styled(
                        "  ",
                        Style::default().fg(INFO_COLOR).add_modifier(Modifier::DIM),
                    )];
                    spans.extend(line.spans.into_iter().map(|span| {
                        Span::styled(
                            span.content,
                            span.style.fg(DIM).add_modifier(Modifier::ITALIC),
                        )
                    }));
                    lines.push(Line::from(spans));
                }
                lines.push(Line::from(""));
            }
        }
    }

    let markdown = render_markdown_with_options(
        &entry.content,
        Some(WHITE),
        MarkdownRenderOptions {
            max_width: Some(max_width),
            enable_hyperlinks,
        },
    );
    for (index, line) in markdown.into_iter().enumerate() {
        if line.spans.is_empty()
            || (line.spans.len() == 1
                && line
                    .spans
                    .first()
                    .is_some_and(|span| span.content.is_empty()))
        {
            lines.push(Line::from(""));
            continue;
        }

        let mut spans = Vec::new();
        if index == 0 {
            spans.push(Span::styled("⏺ ", Style::default().fg(ACCENT)));
        } else {
            spans.push(Span::raw("  "));
        }
        spans.extend(line.spans);
        if index == 0 {
            spans.push(Span::styled(
                " (ctrl+o to inspect)",
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            ));
        }
        lines.push(Line::from(spans));
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{style::Modifier, text::Line};

    use crate::app::{ChatEntry, ChatRole};

    use super::render_assistant;

    fn render_lines(content: &str) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        let entry = ChatEntry::new(ChatRole::Assistant, content.to_string());
        render_assistant(&mut lines, &entry, 120, false, true);
        lines
    }

    #[test]
    fn assistant_render_keeps_true_blank_lines_for_structured_summary() {
        let sample = "根据已有的深度分析记忆，我直接给你综合结论，不需要重新扫描。\nYode vs Claude Code 综合对比\n基本面\n维度 │ Yode │ Claude Code\n──────┼──────┼─────────────\n语言 │ Rust (~15万行) │ TypeScript (~52万行)\n工具数 │ ~45 │ ~50+\n命令数 │ ~30 │ ~80+\nMCP Transport │ 仅 Stdio │ 7种 (sse/http/ws/sdk等)\n5 大核心差距（按影响排序）\n1. MCP 客户端 — 严重不足\n• 缺 SSE/HTTP/WS transport，无法连远程 MCP 服务器\n2. 上下文压缩 — 单层 vs 七层\n• Yode：单层 eviction + 本地模板 summary（1.2K chars）\n3. 命令系统 — 缺少 prompt 类型命令\n• Yode 只有同步 Command trait，CC 有 prompt/local/local-jsx 三种\n优化建议（按 ROI 排序）\n🔴 P0 — 不做会严重影响可用性\n1. LLM 生成 summary 替代本地模板\nYode 的优势（Rust 带来的）\n• 性能：启动快、内存小、无 GC 停顿\n建议优先做 P0 的 1-3（LLM summary + SSE transport + prompt 命令），这三个投入产出比最高。";
        let lines = render_lines(sample)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        let title = lines
            .iter()
            .position(|line| line.contains("Yode vs Claude Code 综合对比"))
            .unwrap();
        assert!(title > 0);
        assert!(lines[title - 1].is_empty());

        let basic = lines
            .iter()
            .position(|line| line.contains("基本面"))
            .unwrap();
        assert!(basic > 0);
        assert!(lines[basic - 1].is_empty());

        let core_gap = lines
            .iter()
            .position(|line| line.contains("5 大核心差距"))
            .unwrap();
        assert!(core_gap > 0);
        assert!(lines[core_gap - 1].is_empty());

        let mcp_heading = lines
            .iter()
            .position(|line| line.contains("1. MCP 客户端"))
            .unwrap();
        assert!(mcp_heading > 0);
        assert!(lines[mcp_heading - 1].is_empty());

        let compact_heading = lines
            .iter()
            .position(|line| line.contains("2. 上下文压缩"))
            .unwrap();
        assert!(compact_heading > 0);
        assert!(lines[compact_heading - 1].is_empty());

        let command_heading = lines
            .iter()
            .position(|line| line.contains("3. 命令系统"))
            .unwrap();
        assert!(command_heading > 0);
        assert!(lines[command_heading - 1].is_empty());

        let roi = lines
            .iter()
            .position(|line| line.contains("优化建议（按 ROI 排序）"))
            .unwrap();
        assert!(roi > 0);
        assert!(lines[roi - 1].is_empty());

        let p0 = lines
            .iter()
            .position(|line| line.contains("🔴 P0 — 不做会严重影响可用性"))
            .unwrap();
        assert!(p0 > 0);
        assert!(lines[p0 - 1].is_empty());

        let strengths = lines
            .iter()
            .position(|line| line.contains("Yode 的优势（Rust 带来的）"))
            .unwrap();
        assert!(strengths > 0);
        assert!(lines[strengths - 1].is_empty());
    }

    #[test]
    fn assistant_reasoning_renders_as_markdown_block() {
        let mut lines = Vec::new();
        let entry = ChatEntry::new_with_reasoning(
            ChatRole::Assistant,
            "final answer".to_string(),
            Some("## Plan\n- inspect\n- patch".to_string()),
        );
        render_assistant(&mut lines, &entry, 120, false, true);

        assert!(lines
            .iter()
            .any(|line| line.to_string().contains("∴ Thinking… (ctrl+o to inspect)")));
        assert!(lines.iter().any(|line| line.to_string().contains("Plan")));
        assert!(lines
            .iter()
            .any(|line| line.to_string().contains("• inspect")));
        assert!(lines.iter().any(|line| {
            line.spans.iter().any(|span| {
                span.content.contains("Plan")
                    && span.style.add_modifier.contains(Modifier::BOLD)
                    && span.style.add_modifier.contains(Modifier::ITALIC)
            })
        }));
    }

    #[test]
    fn assistant_content_advertises_detail_inspection() {
        let lines = render_lines("Final answer");
        assert!(lines[0].to_string().contains("ctrl+o to inspect"));
    }

    #[test]
    fn older_assistant_reasoning_can_collapse_to_teaser_only() {
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
        assert!(rendered
            .iter()
            .any(|line| line.contains("Thinking hidden (ctrl+o to inspect)")));
        assert!(rendered.iter().all(|line| !line.contains("• inspect")));
        assert!(!rendered[1].is_empty());
    }
}
