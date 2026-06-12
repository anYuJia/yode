mod types;
mod utils;
mod parser;
mod renderer;

pub use types::MarkdownRenderOptions;
pub use utils::line_to_ansi_string;

use ratatui::style::Color;
use ratatui::text::Line;

pub fn render_markdown_ansi_with_options(
    text: &str,
    default_fg: Option<Color>,
    options: MarkdownRenderOptions,
) -> Vec<String> {
    render_markdown_with_options(text, default_fg, options)
        .into_iter()
        .map(|line| line_to_ansi_string(&line))
        .collect()
}

pub fn streaming_markdown_advance_stable_boundary(
    text: &str,
    current_stable_len: usize,
) -> usize {
    let stable_len = current_stable_len.min(text.len());
    stable_len + utils::stable_boundary_from_complete_lines(&text[stable_len..])
}

pub fn render_markdown_with_options(
    text: &str,
    default_fg: Option<Color>,
    options: MarkdownRenderOptions,
) -> Vec<Line<'static>> {
    if !utils::has_markdown_syntax(text) {
        return utils::render_plain_text_lines(text, default_fg, options);
    }

    let mut lines = Vec::new();
    let blocks = utils::cached_markdown_blocks(text);
    renderer::render_block_sequence(&mut lines, &blocks, default_fg, 0, true, &options);

    lines
}

pub fn render_markdown_impl(text: &str, default_fg: Option<Color>) -> Vec<Line<'static>> {
    render_markdown_with_options(text, default_fg, MarkdownRenderOptions::default())
}

#[cfg(test)]
mod tests {
    use super::{
        render_markdown_ansi_with_options, render_markdown_impl,
        render_markdown_with_options, streaming_markdown_advance_stable_boundary,
        MarkdownRenderOptions,
    };
    use super::utils::line_display_width;
    use crate::ui::chat::WHITE;
    use ratatui::style::{Color, Modifier};

    #[test]
    fn fenced_code_blocks_render_header_and_highlighted_tokens() {
        let lines = render_markdown_impl("```rust\nfn main() {}\n```", None);
        let header_line = lines
            .iter()
            .find(|line| line.to_string().contains("rust"))
            .unwrap();
        assert!(header_line.to_string().contains("rust"));
        let code_line = lines
            .iter()
            .find(|line| line.to_string().contains("fn main"))
            .unwrap();
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.content == " 1 " && span.style.fg == Some(Color::Indexed(244))));
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Indexed(111))));
    }

    #[test]
    fn diff_code_blocks_render_added_lines_with_gutter_background_and_syntax() {
        let lines = render_markdown_impl(
            "```diff\ndiff --git a/src/main.rs b/src/main.rs\n@@ -0,0 +1,1 @@\n+fn main() {}\n```",
            None,
        );
        let code_line = lines
            .iter()
            .find(|line| line.to_string().contains("fn main"))
            .unwrap();
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.content == "+ 1 " && span.style.fg == Some(Color::Indexed(114))));
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.content == "fn" && span.style.fg == Some(Color::Indexed(111))));
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.style.bg == Some(Color::Indexed(22))));
    }

    #[test]
    fn json_code_blocks_highlight_property_keys() {
        let lines = render_markdown_impl("```json\n{\"name\": \"yode\"}\n```", None);
        let code_line = lines
            .iter()
            .find(|line| line.to_string().contains("\"name\""))
            .unwrap();
        assert!(code_line
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Indexed(153))));
    }

    #[test]
    fn diff_code_blocks_highlight_file_headers_and_hunk_ranges() {
        let lines = render_markdown_impl(
            "```diff\ndiff --git a/src/main.rs b/src/main.rs\n@@ -10,2 +10,4 @@ fn render()\n```",
            None,
        );
        let file_line = lines
            .iter()
            .find(|line| line.to_string().contains("a/src/main.rs"))
            .unwrap();
        assert!(file_line
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Indexed(223))));

        let hunk_line = lines
            .iter()
            .find(|line| line.to_string().contains("@@ -10,2 +10,4 @@"))
            .unwrap();
        assert!(hunk_line
            .spans
            .iter()
            .any(|span| span.style.fg == Some(Color::Indexed(153))));
    }

    #[test]
    fn inline_code_renders_token_spans() {
        let lines = render_markdown_impl("Use `fn main()` here.", None);
        let line = lines
            .iter()
            .find(|line| line.to_string().contains("fn main"))
            .unwrap();
        assert!(line
            .spans
            .iter()
            .any(|span| span.content == "fn" && span.style.fg == Some(Color::Indexed(111))));
    }

    #[test]
    fn headings_preserve_inline_rich_rendering() {
        let lines = render_markdown_impl("# Build **fast** with `cargo test`", None);
        let heading = lines
            .iter()
            .find(|line| line.to_string().contains("Build"))
            .unwrap();
        assert!(!heading.to_string().contains('#'));
        assert!(
            heading
                .spans
                .iter()
                .any(|span| span.content == "fast"
                    && span.style.add_modifier.contains(Modifier::BOLD))
        );
        assert!(heading.spans.iter().any(|span| {
            span.content == "cargo" && span.style.bg == Some(crate::ui::chat::INLINE_CODE_BG)
        }));
        assert!(heading.spans.iter().any(|span| {
            span.content.contains("Build")
                && span.style.add_modifier.contains(Modifier::ITALIC)
                && span.style.add_modifier.contains(Modifier::UNDERLINED)
        }));
    }

    #[test]
    fn table_cells_preserve_inline_rich_rendering() {
        let lines = render_markdown_impl("| Col |\n| --- |\n| **bold** and `code` |", None);
        let row = lines
            .iter()
            .find(|line| line.to_string().contains("bold") && line.to_string().contains("code"))
            .unwrap();
        assert!(
            row.spans
                .iter()
                .any(|span| span.content == "bold"
                    && span.style.add_modifier.contains(Modifier::BOLD))
        );
        assert!(row.spans.iter().any(|span| {
            span.content == "code" && span.style.bg == Some(crate::ui::chat::INLINE_CODE_BG)
        }));
    }

    #[test]
    fn tables_render_full_box_borders_like_claude() {
        let lines = render_markdown_impl("| A | B |\n| --- | --- |\n| 1 | 2 |", None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(lines.iter().any(|line| line.starts_with('┌')));
        assert!(lines.iter().any(|line| line.starts_with('├')));
        assert!(lines.iter().any(|line| line.starts_with('└')));
        assert!(lines
            .iter()
            .any(|line| line.starts_with('│') && line.contains('1') && line.contains('2')));
    }

    #[test]
    fn links_render_as_osc8_hyperlinks_when_enabled() {
        let lines = render_markdown_with_options(
            "[Rust](https://www.rust-lang.org)",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: true,
            },
        );
        let line = lines.first().unwrap();
        assert!(line.spans.iter().any(|span| span
            .content
            .contains("\x1b]8;;https://www.rust-lang.org\x07")));
        assert!(line
            .spans
            .iter()
            .any(|span| span.content == crate::ui::chat_layout::osc8_close_sequence()));
    }

    #[test]
    fn mailto_links_render_as_plain_text() {
        let lines = render_markdown_with_options(
            "[support](mailto:support@example.com)",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: true,
            },
        );
        let rendered = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("support@example.com"));
        assert!(!rendered.contains("mailto:"));
        assert!(!rendered.contains("\x1b]8;;"));
    }

    #[test]
    fn github_issue_references_are_hyperlinked_in_plain_text() {
        let lines = render_markdown_with_options(
            "See anthropics/claude-code#24180 for context.",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: true,
            },
        );
        let rendered = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("anthropics/claude-code#24180"));
        assert!(rendered.contains("\x1b]8;;https://github.com/anthropics/claude-code/issues/24180"));
    }

    #[test]
    fn bare_urls_are_hyperlinked_in_plain_text() {
        let lines = render_markdown_with_options(
            "Open https://example.com/docs for details.",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: true,
            },
        );
        let rendered = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("https://example.com/docs"));
        assert!(rendered.contains("\x1b]8;;https://example.com/docs"));
    }

    #[test]
    fn bare_urls_do_not_absorb_trailing_punctuation() {
        let lines = render_markdown_with_options(
            "Visit https://example.com/docs, then continue.",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: true,
            },
        );
        let rendered = lines
            .iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("\x1b]8;;https://example.com/docs"));
        assert!(!rendered.contains("\x1b]8;;https://example.com/docs,"));
    }

    #[test]
    fn github_issue_links_exclude_trailing_punctuation() {
        let rendered = render_markdown_ansi_with_options(
            "See anthropics/claude-code#24180, then continue.",
            Some(WHITE),
            MarkdownRenderOptions {
                max_width: None,
                enable_hyperlinks: true,
            },
        )
        .join("\n");
        assert!(rendered.contains("anthropics/claude-code#24180"));
        assert!(
            rendered.contains("\u{1b}]8;;https://github.com/anthropics/claude-code/issues/24180")
        );
        assert!(
            !rendered.contains("\u{1b}]8;;https://github.com/anthropics/claude-code/issues/24180,")
        );
    }

    #[test]
    fn markdown_lists_keep_single_space_after_bullets() {
        let lines = render_markdown_impl("- one\n- two", None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(lines.iter().any(|line| line == "• one"));
        assert!(lines.iter().any(|line| line == "• two"));
    }

    #[test]
    fn long_headings_wrap_without_losing_heading_text() {
        let rendered = render_markdown_with_options(
            "# This is a very long heading that should wrap cleanly",
            Some(WHITE),
            MarkdownRenderOptions {
                max_width: Some(24),
                enable_hyperlinks: false,
            },
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
        assert!(rendered.len() >= 2);
        assert!(rendered
            .iter()
            .any(|line| line.contains("This is a very long")));
    }

    #[test]
    fn heading_wrap_continuations_keep_heading_style() {
        let rendered = render_markdown_with_options(
            "# Heading with enough words to wrap across multiple visual rows",
            Some(WHITE),
            MarkdownRenderOptions {
                max_width: Some(22),
                enable_hyperlinks: false,
            },
        );
        assert!(rendered.len() >= 2);
        assert!(rendered.iter().all(|line| {
            line.spans.iter().any(|span| {
                span.style.add_modifier.contains(Modifier::BOLD)
                    && span.style.add_modifier.contains(Modifier::UNDERLINED)
            })
        }));
    }

    #[test]
    fn inline_code_wraps_across_narrow_widths() {
        let rendered = render_markdown_with_options(
            "Use `very_long_inline_code_value_here` now.",
            Some(WHITE),
            MarkdownRenderOptions {
                max_width: Some(18),
                enable_hyperlinks: false,
            },
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
        assert!(rendered.len() >= 2);
        assert!(rendered.iter().any(|line| line.contains("very_long")));
    }

    #[test]
    fn nested_bullets_keep_distinct_indentation_and_markers() {
        let rendered = render_markdown_impl("- parent\n    - child\n        - grandchild", None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(rendered.iter().any(|line| line.starts_with("• parent")));
        let child = rendered
            .iter()
            .find(|line| line.contains("◦ child"))
            .expect("child bullet");
        let grandchild = rendered
            .iter()
            .find(|line| line.contains("▪ grandchild"))
            .expect("grandchild bullet");
        assert!(child.find('◦').unwrap() > 0);
        assert!(grandchild.find('▪').unwrap() > child.find('◦').unwrap());
    }

    #[test]
    fn mixed_bold_italic_wrap_preserves_style_continuity() {
        let rendered = render_markdown_with_options(
            "***important wrapped emphasis keeps its combined style across lines***",
            Some(WHITE),
            MarkdownRenderOptions {
                max_width: Some(18),
                enable_hyperlinks: false,
            },
        );
        assert!(rendered.len() >= 2);
        assert!(rendered.iter().all(|line| {
            line.spans.iter().any(|span| {
                span.style.add_modifier.contains(Modifier::BOLD)
                    && span.style.add_modifier.contains(Modifier::ITALIC)
            })
        }));
    }

    #[test]
    fn cjk_tables_render_without_losing_cells() {
        let lines = render_markdown_impl(
            "| 列 | 值 |\n| --- | --- |\n| 工具 | 远程 |\n| 状态 | 正常 |",
            None,
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
        assert!(lines.iter().any(|line| line.contains("工具")));
        assert!(lines.iter().any(|line| line.contains("状态")));
        assert!(lines
            .iter()
            .any(|line| line.starts_with('┌') || line.contains("Column")));
    }

    #[test]
    fn tables_wrap_to_fit_requested_width() {
        let lines = render_markdown_with_options(
            "| Column |\n| --- |\n| this is a very long cell with `inline code` inside |",
            None,
            MarkdownRenderOptions {
                max_width: Some(24),
                enable_hyperlinks: false,
            },
        );
        assert!(lines.iter().any(|line| {
            let text = line.to_string();
            text.contains("Column:") || text.contains("this is a")
        }));
        assert!(lines.iter().all(|line| line_display_width(line) <= 24));
    }

    #[test]
    fn narrow_tables_fall_back_to_vertical_key_value_layout() {
        let lines = render_markdown_with_options(
            "| Metric | Value |\n| --- | --- |\n| Runtime | This is a very long wrapped explanation |\n| Status | Healthy |",
            None,
            MarkdownRenderOptions {
                max_width: Some(18),
                enable_hyperlinks: false,
            },
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
        assert!(lines.iter().any(|line| line.contains("Metric:")));
        assert!(lines.iter().any(|line| line.contains("Value:")));
        assert!(!lines.iter().any(|line| line.contains("┼")));
    }

    #[test]
    fn wrapped_tables_under_row_threshold_stay_boxed() {
        let lines = render_markdown_with_options(
            "| 优化项 | 说明 | 工作量 |\n| --- | --- | --- |\n| hooks | 缺少 notification hooks, permission hooks, lifecycle hooks，需要补齐事件通知、权限拦截、生命周期回调 | 中 |",
            None,
            MarkdownRenderOptions {
                max_width: Some(64),
                enable_hyperlinks: false,
            },
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.starts_with('┌')));
        assert!(!lines.iter().any(|line| line.contains("优化项:")));
    }

    #[test]
    fn moderately_wrapped_tables_stay_boxed_like_claude() {
        let lines = render_markdown_with_options(
            "| Metric | Value |\n| --- | --- |\n| Runtime | concise wrapped explanation with useful detail |\n| Status | Healthy |",
            None,
            MarkdownRenderOptions {
                max_width: Some(44),
                enable_hyperlinks: false,
            },
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.starts_with('┌')));
        assert!(lines.iter().any(|line| line.starts_with('│')));
        assert!(!lines.iter().any(|line| line.contains("Metric:")));
    }

    #[test]
    fn vertical_table_cells_collapse_internal_whitespace() {
        let lines = render_markdown_with_options(
            "| Metric | Value |\n| --- | --- |\n| Runtime | alpha     beta      gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau |",
            None,
            MarkdownRenderOptions {
                max_width: Some(24),
                enable_hyperlinks: false,
            },
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.contains("Value: alpha beta")));
        assert!(lines.iter().all(|line| !line.contains("     ")));
    }

    #[test]
    fn code_fence_caption_stays_dense_and_labeled() {
        let rendered = render_markdown_impl("```rust\nfn main() {}\n```", None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(rendered[0].contains("rust"));
        assert!(rendered[0].starts_with("╭─"));
        assert!(!rendered[0].contains("Code block"));
    }

    #[test]
    fn hyperlink_text_keeps_underline_intensity() {
        let lines = render_markdown_with_options(
            "[Docs](https://example.com/docs)",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: true,
            },
        );
        assert!(lines[0].spans.iter().any(|span| {
            span.content == "Docs" && span.style.add_modifier.contains(Modifier::UNDERLINED)
        }));
    }

    #[test]
    fn streaming_boundary_advances_monotonically_from_existing_prefix() {
        let first = "first paragraph\n\nsecond";
        let first_boundary = streaming_markdown_advance_stable_boundary(first, 0);
        assert!(first_boundary > 0);

        let second = "first paragraph\n\nsecond line grows";
        let second_boundary = streaming_markdown_advance_stable_boundary(second, first_boundary);
        assert_eq!(second_boundary, first_boundary);

        let third = "first paragraph\n\nsecond line grows\n\nthird block";
        let third_boundary = streaming_markdown_advance_stable_boundary(third, first_boundary);
        assert!(third_boundary > first_boundary);
    }

    #[test]
    fn streaming_boundary_keeps_unclosed_code_fence_unstable() {
        let text = "intro\n\n```rust\nfn main()";
        let boundary = streaming_markdown_advance_stable_boundary(text, 0);
        assert_eq!(boundary, "intro\n\n".len());
    }

    #[test]
    fn streaming_boundary_keeps_unicode_table_block_unstable_until_next_section() {
        let text = "根据已有的深度分析记忆，我直接给你综合结论，不需要重新扫描。\nYode vs Claude Code 综合对比\n基本面\n维度 │ Yode │ Claude Code\n──────┼──────┼─────────────\n";
        let boundary = streaming_markdown_advance_stable_boundary(text, 0);
        assert_eq!(
            boundary,
            "根据已有的深度分析记忆，我直接给你综合结论，不需要重新扫描。\nYode vs Claude Code 综合对比\n".len()
        );
    }

    #[test]
    fn streaming_boundary_holds_trailing_heading_until_followup_arrives() {
        let text = "根据已有的深度分析记忆，我直接给你综合结论，不需要重新扫描。\nYode vs Claude Code 综合对比\n";
        let boundary = streaming_markdown_advance_stable_boundary(text, 0);
        assert_eq!(
            boundary,
            "根据已有的深度分析记忆，我直接给你综合结论，不需要重新扫描。\n".len()
        );
    }

    #[test]
    fn streaming_boundary_keeps_pipe_table_header_unstable_until_table_ends() {
        let first = "一、基本盘\n| 维度 | Yode | Claude Code |\n";
        let first_boundary = streaming_markdown_advance_stable_boundary(first, 0);
        assert_eq!(first_boundary, "一、基本盘\n".len());

        let second = "一、基本盘\n| 维度 | Yode | Claude Code |\n| --- | --- | --- |\n";
        let second_boundary = streaming_markdown_advance_stable_boundary(second, 0);
        assert_eq!(second_boundary, "一、基本盘\n".len());

        let third = "一、基本盘\n| 维度 | Yode | Claude Code |\n| --- | --- | --- |\n| 代码量 | 15万 | 52万 |\n二、命令系统\n";
        let third_boundary = streaming_markdown_advance_stable_boundary(third, 0);
        assert!(third_boundary > second_boundary);
    }

    #[test]
    fn unicode_table_and_compound_rows_are_normalized() {
        let lines = render_markdown_impl(
            "基本面\n 维度 │ Yode │ Claude Code\n──────┼──────┼─────────────\n| 语言 | Rust | TypeScript | | 工具数 | 45 | 50+ |",
            None,
        );
        assert!(lines.iter().any(|line| line.to_string().contains("基本面")));
        assert!(lines.iter().any(|line| line.to_string().contains("维度")));
        assert!(lines.iter().any(|line| line.to_string().contains("语言")));
        assert!(lines.iter().any(|line| line.to_string().contains("工具数")));
    }

    #[test]
    fn short_section_lines_are_promoted_to_headings() {
        let lines = render_markdown_impl(
            "按优先级的优化空间\nP0 — 核心缺失（严重影响日常使用）\n1. First item",
            None,
        );
        let heading = lines
            .iter()
            .find(|line| line.to_string().contains("按优先级的优化空间"))
            .unwrap();
        assert!(!heading.to_string().contains('#'));
        let p0 = lines
            .iter()
            .find(|line| line.to_string().contains("P0 — 核心缺失"))
            .unwrap();
        assert!(!p0.to_string().contains('#'));
    }

    #[test]
    fn real_world_summary_keeps_blank_lines_around_promoted_headings() {
        let lines = render_markdown_impl(
            "Yode vs Claude Code 综合对比\n基本面\n维度 │ Yode │ Claude Code\n──────┼──────┼─────────────\n语言 │ Rust (~15万行) │ TypeScript (~52万行)\n核心差距 (按影响程度排序)\nP0 - 严重缺失 (阻塞日常使用)\n1. 命令系统缺陷",
            None,
        );
        let basic_index = lines
            .iter()
            .position(|line| line.to_string().contains("基本面"))
            .unwrap();
        assert!(basic_index > 0);
        assert!(lines[basic_index - 1].to_string().is_empty());

        let gap_index = lines
            .iter()
            .position(|line| line.to_string().contains("核心差距"))
            .unwrap();
        assert!(gap_index > 0);
        assert!(lines[gap_index - 1].to_string().is_empty());
    }

    #[test]
    fn numbered_section_headings_get_consistent_blank_lines() {
        let lines = render_markdown_impl(
            "1. 命令系统 — 差 2.7 倍\n• 差距：Yode 只有同步 trait\n2. 上下文压缩 — 差 7 层\n• 差距：Yode 只有 eviction\n3. MCP 客户端 — 严重缺失\n• 差距：仅 stdio",
            None,
        );

        let second = lines
            .iter()
            .position(|line| line.to_string().contains("2. 上下文压缩"))
            .unwrap();
        assert!(second > 0);
        assert!(lines[second - 1].to_string().is_empty());

        let third = lines
            .iter()
            .position(|line| line.to_string().contains("3. MCP 客户端"))
            .unwrap();
        assert!(third > 0);
        assert!(lines[third - 1].to_string().is_empty());
    }

    #[test]
    fn chinese_sections_and_priority_heads_use_distinct_levels() {
        let lines = render_markdown_impl(
            "三、Yode 严重缺失的功能（按优先级）\nP0 - 核心缺失\n- /init\nP1 - 重要缺失\n- Skills\n四、Yode 的相对优势\n1. 开源",
            None,
        );

        let section = lines
            .iter()
            .find(|line| line.to_string().contains("三、Yode 严重缺失的功能"))
            .unwrap();
        let p0 = lines
            .iter()
            .find(|line| line.to_string().contains("P0 - 核心缺失"))
            .unwrap();
        let section_fg = section.spans.iter().find_map(|span| span.style.fg).unwrap();
        let p0_fg = p0.spans.iter().find_map(|span| span.style.fg).unwrap();
        assert_ne!(section_fg, p0_fg);
    }

    #[test]
    fn pasted_analysis_sample_normalizes_sections_and_table_rows() {
        let sample = "Yode vs Claude Code 综合对比\n基本面\n 维度 │ Yode           │ Claude Code          \n──────┼────────────────┼──────────────────────\n 语言 │ Rust (~15万行) │ TypeScript (~52万行) \n| 工具数 | ~45 | ~50+ |\n| 命令数 | ~30 | ~80+ |\n| MCP | rmcp (基础) | 完整SDK (SSE/Stdio/HTTP) |\n核心差距 (优先级排序)\nP0 - 严重缺失\n1. 命令系统缺陷";
        let lines = render_markdown_impl(sample, None);

        assert!(lines.iter().all(|line| !line.to_string().contains("###")));
        assert!(lines
            .iter()
            .all(|line| !line.to_string().contains("| 工具数 | ~45 | ~50+ |")));

        let basic = lines
            .iter()
            .position(|line| line.to_string().contains("基本面"))
            .unwrap();
        assert!(basic > 0);
        assert!(lines[basic - 1].to_string().is_empty());

        let p0 = lines
            .iter()
            .position(|line| line.to_string().contains("P0 - 严重缺失"))
            .unwrap();
        assert!(p0 > 0);
        assert!(lines[p0 - 1].to_string().is_empty());
    }

    #[test]
    fn loose_ascii_pipe_rows_become_real_table_blocks() {
        let sample = "架构对比要点\n维度 | Yode | Claude Code |\n命令注册 | 全量静态编译时 | 懒加载 + 运行时动态 |\nUI 渲染 | 纯文本 | React JSX (交互) |";
        let lines = render_markdown_impl(sample, None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.contains("架构对比要点")));

        assert!(lines.iter().any(|line| line.contains("维度")));
        assert!(lines.iter().any(|line| line.contains("命令注册")));
        assert!(lines.iter().any(|line| line.contains("UI 渲染")));
        assert!(lines
            .iter()
            .all(|line| !line.contains("维度 | Yode | Claude Code |")));
        assert!(lines
            .iter()
            .all(|line| !line.contains("命令注册 | 全量静态编译时 | 懒加载 + 运行时动态 |")));
    }

    #[test]
    fn unicode_bullet_lines_become_markdown_lists() {
        let lines = render_markdown_impl("优势\n  • 性能\n  • 安全", None);
        assert!(lines.iter().any(|line| line.to_string().contains("• 性能")));
        assert!(lines.iter().any(|line| line.to_string().contains("• 安全")));
    }

    #[test]
    fn structural_lines_strip_two_space_indent() {
        let lines = render_markdown_impl("  基本面\n  | A | B |\n  | 1 | 2 |", None);
        let heading = lines
            .iter()
            .find(|line| line.to_string().contains("基本面"))
            .unwrap();
        assert!(!heading.to_string().starts_with("  基本面"));
    }

    #[test]
    fn shell_code_blocks_render_as_highlighted_code() {
        let lines = render_markdown_impl(
            "```bash\nuser@yode ~/repo $ cargo test -- --nocapture\necho $HOME\n```",
            None,
        );
        let command_line = lines
            .iter()
            .find(|line| line.to_string().contains("cargo test"))
            .unwrap();
        assert!(command_line
            .spans
            .iter()
            .any(|span| span.content == " 1 " && span.style.fg == Some(Color::Indexed(244))));
        assert!(command_line
            .spans
            .iter()
            .any(|span| span.content == "cargo" && span.style.fg == Some(Color::Indexed(222))));
        assert!(command_line.spans.iter().any(
            |span| span.content == "--nocapture" && span.style.fg == Some(Color::Indexed(111))
        ));

        let second_line = lines
            .iter()
            .find(|line| line.to_string().contains("echo $HOME"))
            .unwrap();
        assert!(second_line
            .spans
            .iter()
            .any(|span| span.content == " 2 " && span.style.fg == Some(Color::Indexed(244))));
        assert!(second_line
            .spans
            .iter()
            .any(|span| span.content == "echo" && span.style.fg == Some(Color::Indexed(222))));
        assert!(second_line
            .spans
            .iter()
            .any(|span| span.content == "$HOME" && span.style.fg == Some(Color::Indexed(215))));
    }

    #[test]
    fn shell_code_blocks_render_as_generic_highlighted_code_without_transcript_gutter() {
        let lines = render_markdown_impl(
            "```bash\nPS C:\\repo> cargo test `\n>> -- --nocapture\n./scripts/run.sh --config ./cfg.json\n```",
            None,
        );
        let continuation_line = lines
            .iter()
            .find(|line| line.to_string().contains("-- --nocapture"))
            .unwrap();
        assert!(continuation_line
            .spans
            .iter()
            .any(|span| span.content == " 2 " && span.style.fg == Some(Color::Indexed(244))));

        let path_line = lines
            .iter()
            .find(|line| line.to_string().contains("./scripts/run.sh"))
            .unwrap();
        assert!(path_line
            .spans
            .iter()
            .any(|span| span.content.contains("./scripts/run.sh")
                && span.style.fg == Some(Color::Indexed(222))));
        assert!(path_line
            .spans
            .iter()
            .any(|span| span.content.contains("./cfg.json")
                && span.style.fg == Some(Color::Indexed(153))));
    }

    #[test]
    fn shell_code_blocks_highlight_output_paths_and_numbers() {
        let lines = render_markdown_impl(
            "```bash\ncat /Users/pyu/code/yode/package.json\nprintf 14\n```",
            None,
        );
        let compiling_line = lines
            .iter()
            .find(|line| {
                line.to_string()
                    .contains("/Users/pyu/code/yode/package.json")
            })
            .unwrap();
        assert!(compiling_line.spans.iter().any(|span| span
            .content
            .contains("/Users/pyu/code/yode/package.json")
            && span.style.fg == Some(Color::Indexed(153))));

        let count_line = lines
            .iter()
            .find(|line| line.to_string().contains("printf 14"))
            .unwrap();
        assert!(count_line
            .spans
            .iter()
            .any(|span| span.content == "14" && span.style.fg == Some(Color::Indexed(151))));
    }

    #[test]
    fn headings_and_code_blocks_insert_vertical_spacing() {
        let lines = render_markdown_impl("# Title\nparagraph\n```rust\nfn main() {}\n```", None);
        let title_index = lines
            .iter()
            .position(|line| line.to_string().contains("Title"))
            .unwrap();
        assert!(!lines[title_index].to_string().contains('#'));
        let code_index = lines
            .iter()
            .position(|line| line.to_string().contains("rust"))
            .unwrap();
        assert_eq!(title_index, 0);
        assert!(lines[title_index + 1].to_string().is_empty());
        assert!(lines[code_index - 1].to_string().is_empty());
    }

    #[test]
    fn secondary_headings_stay_tight_with_following_content() {
        let lines = render_markdown_impl("## Section\nparagraph\n- item", None);
        let section_index = lines
            .iter()
            .position(|line| line.to_string().contains("Section"))
            .unwrap();
        assert_eq!(section_index, 0);
        assert_eq!(lines[section_index + 1].to_string(), "paragraph");
        assert!(lines.iter().any(|line| line.to_string().contains("• item")));
    }

    #[test]
    fn blank_lines_are_collapsed_and_trimmed() {
        let lines = render_markdown_impl("\n\n> quote\n\n\ntext\n\n", None);
        assert_eq!(lines.first().unwrap().to_string(), "▎ quote");
        let text_index = lines
            .iter()
            .position(|line| line.to_string() == "text")
            .unwrap();
        assert!(lines[text_index - 1].to_string().is_empty());
        assert!(!lines.last().unwrap().to_string().is_empty());
    }

    #[test]
    fn whitespace_only_lines_do_not_create_large_vertical_gaps() {
        let lines = render_markdown_impl(
            "10. MCP 多配置源\n11. MCP OAuth 认证\n   \n      \n\n\n21. Auto Dream - 后台思考能力",
            None,
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();

        assert!(lines.iter().any(|line| line.contains("10. MCP 多配置源")));
        assert!(lines.iter().any(|line| line.contains("21. Auto Dream")));
        assert!(!lines
            .windows(2)
            .any(|window| window[0].is_empty() && window[1].is_empty()));
    }

    #[test]
    fn plain_text_fast_path_treats_whitespace_only_lines_as_blank() {
        let lines = render_markdown_with_options(
            "alpha\n   \n\nbeta",
            None,
            MarkdownRenderOptions {
                max_width: Some(80),
                enable_hyperlinks: false,
            },
        )
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();

        assert_eq!(lines, vec!["alpha", "", "beta"]);
    }

    #[test]
    fn blockquote_content_is_rendered_italic_like_claude() {
        let lines = render_markdown_impl("> quoted text", None);
        let quote = lines.first().unwrap();
        assert!(quote
            .spans
            .iter()
            .any(|span| span.content.contains("quoted text")
                && span.style.add_modifier.contains(Modifier::ITALIC)));
    }

    #[test]
    fn paragraph_lines_collapse_into_single_markdown_block() {
        let lines = render_markdown_impl("first line\nsecond line\n\n- item", None);
        assert_eq!(lines[0].to_string(), "first line");
        assert_eq!(lines[1].to_string(), "second line");
        assert!(lines[2].to_string().is_empty());
        assert!(lines.iter().any(|line| line.to_string().contains("• item")));
    }

    #[test]
    fn plain_text_fast_path_preserves_wrapping_without_markdown_parse() {
        let lines = render_markdown_with_options(
            "plain text line one\nplain text line two with width",
            None,
            MarkdownRenderOptions {
                max_width: Some(20),
                enable_hyperlinks: false,
            },
        );
        assert!(lines.iter().all(|line| line_display_width(line) <= 20));
        assert!(lines
            .iter()
            .any(|line| line.to_string().contains("plain text line one")));
        assert!(lines
            .iter()
            .any(|line| line.to_string().contains("plain text line two")));
    }

    #[test]
    fn blank_lines_between_list_items_are_collapsed() {
        let lines = render_markdown_impl("1. one\n\n2. two\n\n3. three", None)
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>();
        assert!(lines.iter().any(|line| line.contains("1. one")));
        assert!(lines.iter().any(|line| line.contains("2. two")));
        assert!(lines.iter().any(|line| line.contains("3. three")));
        assert!(!lines
            .windows(2)
            .any(|window| window[0].contains("1. one") && window[1].is_empty()));
        assert!(!lines
            .windows(2)
            .any(|window| window[0].contains("2. two") && window[1].is_empty()));
    }

    #[test]
    fn soft_breaks_preserve_table_like_lines_separately() {
        let lines = render_markdown_impl("intro line.\n| a | b |\n| c | d |", None);
        assert!(lines
            .iter()
            .any(|line| line.to_string().contains("intro line.")));
        assert!(lines.iter().any(|line| line.to_string().contains("a")));
        assert!(lines.iter().any(|line| line.to_string().contains("c")));
    }
}
