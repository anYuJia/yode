#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssistantDisplayContent {
    pub text: String,
    pub was_compacted: bool,
}

const MAX_LINES_BEFORE_COMPACT: usize = 16;
const MAX_PRESERVED_BODY_LINES: usize = 14;
const PREFACE_MAX_CHARS: usize = 120;

pub(crate) fn compact_assistant_display_markdown(content: &str) -> AssistantDisplayContent {
    let normalized = normalize_assistant_display_markdown(content);
    let was_compacted = normalized != content;
    AssistantDisplayContent {
        text: normalized,
        was_compacted,
    }
}

pub(crate) fn compact_assistant_streaming_preview_markdown(
    content: &str,
) -> AssistantDisplayContent {
    if is_project_comparison_report(content) {
        return compact_project_comparison_streaming_preview(content);
    }

    if !should_compact_assistant_display(content) {
        return AssistantDisplayContent {
            text: normalize_assistant_display_markdown(content),
            was_compacted: false,
        };
    }

    compact_generic_streaming_preview(content)
}

fn normalize_assistant_display_markdown(content: &str) -> String {
    let mut normalized = Vec::new();
    let mut previous_blank = true;
    let mut in_code_fence = false;

    for raw_line in content.lines() {
        let line = raw_line.trim_end();
        let trimmed = line.trim();

        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
            normalized.push(line.to_string());
            previous_blank = false;
            continue;
        }

        if !in_code_fence && trimmed.is_empty() {
            if !previous_blank {
                normalized.push(String::new());
            }
            previous_blank = true;
            continue;
        }

        normalized.push(line.to_string());
        previous_blank = false;
    }

    while normalized.last().is_some_and(|line| line.trim().is_empty()) {
        normalized.pop();
    }

    normalized.join("\n")
}

fn compact_generic_streaming_preview(content: &str) -> AssistantDisplayContent {
    let mut kept = Vec::new();
    let mut hidden_lines = 0usize;
    let mut nonempty_seen = 0usize;
    let mut preserved_body = 0usize;
    let mut in_code_fence = false;
    let mut inserted_table_note = false;

    for raw_line in content.lines() {
        let trimmed = raw_line.trim();

        if trimmed.starts_with("```") {
            in_code_fence = !in_code_fence;
            hidden_lines += 1;
            continue;
        }
        if in_code_fence {
            hidden_lines += 1;
            continue;
        }

        if trimmed.is_empty() {
            if kept.last().is_some_and(|line: &String| !line.is_empty()) {
                kept.push(String::new());
            }
            continue;
        }

        nonempty_seen += 1;
        if nonempty_seen <= 2 {
            kept.push(trimmed.to_string());
            continue;
        }

        if is_table_like(trimmed) {
            hidden_lines += 1;
            if !inserted_table_note {
                if kept.last().is_some_and(|line: &String| !line.is_empty()) {
                    kept.push(String::new());
                }
                kept.push("- 表格预览已折叠，完成后显示全文。".to_string());
                inserted_table_note = true;
                preserved_body += 1;
            }
            continue;
        }

        let structural =
            is_heading_like(trimmed) || is_bullet_like(trimmed) || is_short_label_line(trimmed);

        if preserved_body >= MAX_PRESERVED_BODY_LINES && !is_heading_like(trimmed) {
            hidden_lines += 1;
            continue;
        }

        if structural {
            kept.push(trimmed.to_string());
            preserved_body += 1;
        } else {
            hidden_lines += 1;
        }
    }

    while kept.first().is_some_and(|line| line.is_empty()) {
        kept.remove(0);
    }
    while kept.last().is_some_and(|line| line.is_empty()) {
        kept.pop();
    }
    collapse_repeated_blank_lines(&mut kept);

    if hidden_lines > 0 {
        if kept.last().is_some_and(|line| !line.is_empty()) {
            kept.push(String::new());
        }
        kept.push("… 流式预览已折叠，完成后显示全文。".to_string());
    }

    AssistantDisplayContent {
        text: kept.join("\n"),
        was_compacted: hidden_lines > 0,
    }
}

fn compact_project_comparison_streaming_preview(content: &str) -> AssistantDisplayContent {
    let lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    let title = lines
        .iter()
        .find(|line| {
            (line.contains("Yode") && line.contains("Claude"))
                || line.contains("深度对比")
                || line.contains("优化建议")
        })
        .or_else(|| lines.first())
        .copied()
        .unwrap_or("项目对比分析");

    let mut kept = vec![
        title.to_string(),
        "结论：项目对比报告已压缩，宽表、长清单和展开说明已折叠。".to_string(),
        "- 基本规模 / 已做好的部分：已折叠，按 ctrl+o 查看全文。".to_string(),
    ];

    let mut in_gap_section = false;
    let mut gap_lines = 0usize;
    for line in lines {
        if is_table_like(line) || line.chars().all(|ch| "─━═-— ".contains(ch)) {
            continue;
        }

        if line.contains("关键差距") || line.contains("核心差距") {
            in_gap_section = true;
            if !kept.iter().any(|kept_line| kept_line == line) {
                kept.push(line.to_string());
            }
            continue;
        }

        let is_priority = line.starts_with("P0")
            || line.starts_with("P1")
            || line.starts_with("P2")
            || line.starts_with("P3")
            || line.contains(" P0")
            || line.contains(" P1")
            || line.contains(" P2")
            || line.contains(" P3");
        let is_gap_item = in_gap_section && is_numbered_list_item(line);
        if (is_priority || is_gap_item) && gap_lines < 6 {
            kept.push(line.to_string());
            gap_lines += 1;
        }
    }

    if gap_lines == 0 {
        kept.push("- 关键差距：已折叠，按 ctrl+o 查看全文。".to_string());
    }
    kept.push("… 全文已折叠，按 ctrl+o 查看完整报告。".to_string());

    AssistantDisplayContent {
        text: kept.join("\n"),
        was_compacted: true,
    }
}

fn collapse_repeated_blank_lines(lines: &mut Vec<String>) {
    let mut normalized = Vec::with_capacity(lines.len());
    let mut previous_blank = true;
    for line in lines.drain(..) {
        let blank = line.trim().is_empty();
        if blank {
            if !previous_blank {
                normalized.push(String::new());
            }
        } else {
            normalized.push(line);
        }
        previous_blank = blank;
    }
    while normalized.last().is_some_and(|line| line.trim().is_empty()) {
        normalized.pop();
    }
    *lines = normalized;
}

pub(crate) fn should_hide_assistant_preface_for_tools(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed.contains('\n') {
        return false;
    }
    if trimmed.chars().count() > PREFACE_MAX_CHARS {
        return false;
    }

    let lower = trimmed.to_ascii_lowercase();
    let english_prefixes = [
        "let me ",
        "i'll ",
        "i will ",
        "now i'll ",
        "now i will ",
        "based on ",
    ];
    let chinese_prefixes = [
        "我来",
        "我先",
        "继续",
        "接下来",
        "让我",
        "下面",
        "先看",
        "继续看",
        "我继续",
    ];
    let action_markers = [
        "analy",
        "explore",
        "inspect",
        "scan",
        "check",
        "look",
        "read",
        "dig",
        "deep dive",
        "summar",
        "分析",
        "查看",
        "检查",
        "读取",
        "扫描",
        "总结",
        "对比",
    ];

    (english_prefixes
        .iter()
        .any(|prefix| lower.starts_with(prefix))
        || chinese_prefixes
            .iter()
            .any(|prefix| trimmed.starts_with(prefix)))
        && action_markers
            .iter()
            .any(|marker| lower.contains(marker) || trimmed.contains(marker))
}

fn should_compact_assistant_display(content: &str) -> bool {
    let lines = content.lines().collect::<Vec<_>>();
    if content.contains("```") {
        return false;
    }

    let table_lines = lines
        .iter()
        .filter(|line| is_table_like(line.trim()))
        .count();
    let heading_lines = lines
        .iter()
        .filter(|line| is_heading_like(line.trim()))
        .count();
    let bullet_lines = lines
        .iter()
        .filter(|line| is_bullet_like(line.trim()))
        .count();

    if table_lines >= 2 && heading_lines >= 3 && lines.len() >= 12 {
        return true;
    }

    if lines.len() <= MAX_LINES_BEFORE_COMPACT {
        return false;
    }

    table_lines >= 2 || heading_lines >= 4 || bullet_lines >= 8
}

fn is_project_comparison_report(content: &str) -> bool {
    let mentions_yode = content.contains("Yode") || content.contains("yode");
    let mentions_claude = content.contains("Claude Code")
        || content.contains("claude-code-rev")
        || content.contains("Claude");
    let has_report_shape = content.contains("深度对比")
        || content.contains("关键差距")
        || content.contains("核心差距")
        || content.contains("基本规模")
        || content.contains("已做好的部分")
        || content.contains("优化建议");

    mentions_yode && mentions_claude && has_report_shape
}

fn is_numbered_list_item(trimmed: &str) -> bool {
    let Some((prefix, rest)) = trimmed.split_once('.') else {
        return false;
    };
    !rest.trim().is_empty()
        && (1..=2).contains(&prefix.len())
        && prefix.chars().all(|ch| ch.is_ascii_digit())
}

fn is_table_like(trimmed: &str) -> bool {
    trimmed.contains('│')
        || trimmed.contains('┼')
        || trimmed.contains('├')
        || trimmed.contains('└')
        || trimmed.contains('┌')
        || trimmed.contains('┐')
        || trimmed.contains('┘')
        || trimmed.starts_with('|')
        || (trimmed.contains('|') && trimmed.split('|').count() >= 3)
}

fn is_heading_like(trimmed: &str) -> bool {
    trimmed.starts_with('#')
        || trimmed.starts_with("P0")
        || trimmed.starts_with("P1")
        || trimmed.starts_with("P2")
        || trimmed.starts_with("P3")
        || trimmed.starts_with("🔴 P0")
        || trimmed.starts_with("🟠 P1")
        || trimmed.starts_with("🟡 P2")
        || trimmed.starts_with("🟢 P3")
        || trimmed.starts_with("一、")
        || trimmed.starts_with("二、")
        || trimmed.starts_with("三、")
        || trimmed.starts_with("四、")
        || trimmed.starts_with("五、")
        || trimmed.starts_with("六、")
        || trimmed.starts_with("七、")
        || trimmed.starts_with("八、")
        || trimmed.starts_with("九、")
        || trimmed.starts_with("十、")
        || trimmed.starts_with("基本面")
        || trimmed.contains("关键差距")
        || trimmed.contains("核心差距")
        || trimmed.contains("优化建议")
        || trimmed.contains("Yode 的优势")
}

fn is_bullet_like(trimmed: &str) -> bool {
    trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("• ")
        || trimmed.starts_with("◦ ")
        || trimmed.starts_with("▪ ")
        || trimmed.starts_with("1. ")
        || trimmed.starts_with("2. ")
        || trimmed.starts_with("3. ")
        || trimmed.starts_with("4. ")
        || trimmed.starts_with("5. ")
}

fn is_short_label_line(trimmed: &str) -> bool {
    trimmed.chars().count() <= 28
        && (trimmed.ends_with('：') || trimmed.ends_with(':') || trimmed.ends_with(')'))
}

#[cfg(test)]
mod tests {
    use super::{
        compact_assistant_display_markdown, compact_assistant_streaming_preview_markdown,
        should_hide_assistant_preface_for_tools,
    };

    #[test]
    fn final_display_preserves_long_comparison_reports() {
        let sample = "根据已有的深度分析记忆，我直接给你综合结论，不需要重新扫描。\nYode vs Claude Code 综合对比\n基本面\n维度 │ Yode │ Claude Code\n──────┼──────┼─────────────\n语言 │ Rust (~15万行) │ TypeScript (~52万行)\n工具数 │ ~45 │ ~50+\n命令数 │ ~30 │ ~80+\nMCP Transport │ 仅 Stdio │ 7种 (sse/http/ws/sdk等)\n5 大核心差距（按影响排序）\n1. MCP 客户端 — 严重不足\n• 缺 SSE/HTTP/WS transport，无法连远程 MCP 服务器\n2. 上下文压缩 — 单层 vs 七层\n• Yode：单层 eviction + 本地模板 summary（1.2K chars）\n3. 命令系统 — 缺少 prompt 类型命令\n• Yode 只有同步 Command trait，CC 有 prompt/local/local-jsx 三种\n优化建议（按 ROI 排序）\n🔴 P0 — 不做会严重影响可用性\n1. LLM 生成 summary 替代本地模板\nYode 的优势（Rust 带来的）\n• 性能：启动快、内存小、无 GC 停顿\n建议优先做 P0 的 1-3（LLM summary + SSE transport + prompt 命令），这三个投入产出比最高。";

        let compacted = compact_assistant_display_markdown(sample);
        assert!(!compacted.was_compacted);
        assert!(compacted.text.contains("Yode vs Claude Code 综合对比"));
        assert!(compacted.text.contains("MCP 客户端"));
        assert!(compacted.text.contains("LLM 生成 summary"));
        assert!(compacted.text.contains("性能：启动快"));
        assert!(compacted.text.contains("工具数 │ ~45 │ ~50+"));
        assert!(!compacted.text.contains("折叠"));
    }

    #[test]
    fn leaves_short_answers_unchanged() {
        let sample = "结论：Yode 现在最该补 MCP transport 和输出压缩。";
        let compacted = compact_assistant_display_markdown(sample);
        assert!(!compacted.was_compacted);
        assert_eq!(compacted.text, sample);
    }

    #[test]
    fn detects_low_value_preface_before_tool_calls() {
        assert!(should_hide_assistant_preface_for_tools(
            "Let me continue the deep dive into the key architectural components."
        ));
        assert!(should_hide_assistant_preface_for_tools(
            "我来继续分析关键架构差异。"
        ));
        assert!(!should_hide_assistant_preface_for_tools(
            "结论：Yode 当前最大短板是 MCP transport。"
        ));
    }

    #[test]
    fn final_display_collapses_repeated_blank_lines_without_hiding_report() {
        let sample = "结合项目结构、代码和已有的深度分析笔记，以下是完整的对比分析：\n\n────────────────────────────────────────\nYode vs Claude Code 深度对比与优化建议\n一、基本规模\n┌────────────┬─────────────────────────────────────┬──────────────────────────┐\n│ 维度       │ Yode                                │ Claude Code              │\n└────────────┴─────────────────────────────────────┴──────────────────────────┘\nYode 以 Claude Code 1/3 的代码量实现了其 70% 的核心功能。\n二、Yode 已经做好的部分\n• Agent Loop 核心\n• TUI 界面\n• 权限系统\n• Subagent\n• 命令系统\n• Session 持久化\n• 自动更新\n• Microcompact\n• Hook 系统\n\n三、关键差距分析（按影响排序）\n\nP0 — 严重影响日常使用\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n\n1. 上下文压缩：单层 vs 7 层\n\nYode 的 ContextManager 是单一阈值压缩。\n";
        let compacted = compact_assistant_display_markdown(sample);
        assert!(compacted.was_compacted);
        assert!(compacted.text.contains("P0"));
        assert!(compacted
            .text
            .lines()
            .collect::<Vec<_>>()
            .windows(2)
            .all(|pair| !(pair[0].trim().is_empty() && pair[1].trim().is_empty())));
        assert!(compacted.text.contains("│ 维度"));
        assert!(compacted.text.contains("1. 上下文压缩：单层 vs 7 层"));
        assert!(!compacted.text.contains("其余展开说明已折叠"));
    }

    #[test]
    fn streaming_preview_uses_aggressive_project_report_summary() {
        let sample = "Yode vs Claude Code 深度对比与优化建议\n一、基本规模\n┌────────────┬────────────┐\n│ 维度 │ Yode │ Claude Code │\n└────────────┴────────────┘\n三、关键差距分析\nP0 — 核心缺失\n1. 上下文压缩\nP1 — 重要差距\n1. 命令系统\nYode 的优势\n1. Rust 性能";
        let compacted = compact_assistant_streaming_preview_markdown(sample);
        assert!(compacted.was_compacted);
        assert!(compacted.text.contains("项目对比报告已压缩"));
        assert!(!compacted.text.contains("│ 维度"));
    }
}
