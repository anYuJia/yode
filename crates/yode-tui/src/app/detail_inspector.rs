use crate::app::{App, ChatEntry, ChatRole, InspectorView, PendingConfirmation};
use crate::display_text::{compact_path_tail as compact_path, human_tool_display_name};
use crate::system_message::{
    format_system_detail_line, parse_system_message, system_message_summary,
};
use crate::tool_grouping::{
    describe_tool_call, detect_groupable_system_batch, detect_groupable_tool_batch,
    tool_batch_hint_text, tool_batch_progress_text, tool_batch_summary_text, SystemBatch,
    ToolBatch,
};
use crate::tool_output_summary::{parse_shell_output_sections, summarize_tool_result};
use crate::ui::chat::{
    render_markdown_ansi_dim_with_options, render_markdown_ansi_white_with_options,
};
use crate::ui::error_format::{parse_error_view, ErrorKind, ErrorView};
use crate::ui::inspector::{
    InspectorAction, InspectorDocument, InspectorPanel, InspectorState, InspectorTab,
};

pub(crate) const INSPECTOR_CONFIRM_ALLOW: &str = "__yode_confirm_allow__";
pub(crate) const INSPECTOR_CONFIRM_ALWAYS: &str = "__yode_confirm_always__";
pub(crate) const INSPECTOR_CONFIRM_DENY: &str = "__yode_confirm_deny__";

const INSPECTOR_MARKDOWN_WIDTH: usize = 100;
const RAW_PANEL_LABEL: &str = "Raw view";

pub(crate) fn open_pending_confirmation_inspector(app: &mut App) -> bool {
    let Some(confirm) = app.pending_confirmation.as_ref() else {
        return false;
    };
    let document = build_pending_confirmation_document(app, confirm);
    app.inspector.stack.push(document.state.title.clone());
    app.inspector.views.push(InspectorView { document });
    true
}

pub(crate) fn open_latest_tool_inspector(app: &mut App) -> bool {
    let Some(document) = build_latest_tool_document(app) else {
        return false;
    };
    app.inspector.stack.push(document.state.title.clone());
    app.inspector.views.push(InspectorView { document });
    true
}

fn build_pending_confirmation_document(
    app: &App,
    confirm: &PendingConfirmation,
) -> InspectorDocument {
    let args = parse_json(&confirm.arguments);
    let tool_label = tool_display_name(app, &confirm.name);
    let activity = describe_tool_call(&confirm.name, &args, true)
        .or_else(|| {
            app.tools
                .get(&confirm.name)
                .map(|tool| tool.activity_description(&args))
        })
        .unwrap_or_else(|| "Pending tool execution".to_string());
    let risk = tool_risk_hint(app, &confirm.name);

    let mut overview = vec![
        format!("Tool: {}", tool_label),
        format!("Activity: {}", activity),
    ];
    if let Some(risk) = risk {
        overview.push(format!("Risk: {}", risk));
    }
    overview.push(String::new());
    overview.push("Decision controls:".to_string());
    overview.push("  y / Enter  allow once".to_string());
    overview.push("  a          always allow tool".to_string());
    overview.push("  n          deny".to_string());

    let arguments = json_to_lines(&args);
    let panels = vec![
        PanelSpec {
            label: "Overview".to_string(),
            lines: overview,
            badges: Vec::new(),
            actions: confirm_actions(),
        },
        PanelSpec {
            label: "Arguments".to_string(),
            lines: if arguments.is_empty() {
                vec!["(no arguments)".to_string()]
            } else {
                arguments
            },
            badges: Vec::new(),
            actions: confirm_actions(),
        },
    ];
    build_document(
        format!("Confirm {}", tool_label),
        panels,
        Some("Esc close inspector · return to confirmation with y / a / n".to_string()),
    )
}

fn build_latest_tool_document(app: &App) -> Option<InspectorDocument> {
    let entries = &app.chat_entries;
    let mut latest = None;
    let mut index = 0;

    while index < entries.len() {
        if let Some(batch) = detect_groupable_tool_batch(entries, index) {
            latest = Some(build_tool_batch_document(app, entries, &batch));
            index = batch.next_index;
            continue;
        }

        if let Some(batch) = detect_groupable_system_batch(entries, index) {
            latest = Some(build_system_batch_document(entries, &batch));
            index = batch.next_index;
            continue;
        }

        match &entries[index].role {
            ChatRole::Assistant => {
                latest = Some(build_assistant_entry_document(&entries[index]));
                index += 1;
            }
            ChatRole::ToolCall { id, name } => {
                let result_index = entries[index + 1..]
                    .iter()
                    .position(
                        |entry| matches!(&entry.role, ChatRole::ToolResult { id: rid, .. } if rid == id),
                    )
                    .map(|offset| index + offset + 1);
                let result = result_index.and_then(|entry_index| entries.get(entry_index));
                latest = Some(build_tool_entry_document(
                    app,
                    name,
                    &entries[index].content,
                    result,
                ));
                index = result_index
                    .map(|entry_index| entry_index + 1)
                    .unwrap_or(index + 1);
            }
            ChatRole::ToolResult { id, .. } => {
                let has_preceding = index > 0
                    && entries[..index]
                        .iter()
                        .rev()
                        .any(|entry| matches!(&entry.role, ChatRole::ToolCall { id: tid, .. } if tid == id));
                if !has_preceding {
                    latest = Some(build_standalone_result_document(app, &entries[index]));
                }
                index += 1;
            }
            ChatRole::System => {
                latest = Some(build_system_entry_document(&entries[index]));
                index += 1;
            }
            ChatRole::Error => {
                latest = Some(build_error_entry_document(&entries[index]));
                index += 1;
            }
            _ => {
                index += 1;
            }
        }
    }

    latest
}

fn build_assistant_entry_document(entry: &ChatEntry) -> InspectorDocument {
    let overview_badges = assistant_overview_badges(entry);
    let mut panels = vec![PanelSpec {
        label: "Overview".to_string(),
        lines: vec![
            "Role: Assistant".to_string(),
            format!("Content lines: {}", entry.content.lines().count()),
            format!(
                "Reasoning: {}",
                entry
                    .reasoning
                    .as_deref()
                    .map(|reasoning| {
                        if reasoning.trim().is_empty() {
                            "none".to_string()
                        } else {
                            format!("{} lines", reasoning.lines().count())
                        }
                    })
                    .unwrap_or_else(|| "none".to_string())
            ),
        ],
        badges: overview_badges.clone(),
        actions: vec![status_action()],
    }];

    if let Some(panel) = markdown_panel(
        "Content",
        &entry.content,
        assistant_content_badges(entry),
        vec![InspectorAction {
            label: "continue from this".to_string(),
            command: "Summarize the latest assistant response and suggest the next best action."
                .to_string(),
        }],
    ) {
        panels.push(panel);
    }

    if let Some(reasoning) = entry
        .reasoning
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        if let Some(panel) = dim_markdown_panel(
            "Reasoning",
            reasoning,
            assistant_reasoning_badges(reasoning),
            vec![InspectorAction {
                label: "distill reasoning".to_string(),
                command:
                    "Distill the latest assistant reasoning into the key decisions and next step."
                        .to_string(),
            }],
        ) {
            panels.push(panel);
        }
    }

    build_document(
        "Assistant details".to_string(),
        panels,
        Some("Esc close inspector".to_string()),
    )
}

fn build_system_entry_document(entry: &ChatEntry) -> InspectorDocument {
    let view = parse_system_message(&entry.content);
    let badges = system_entry_badges(&view);
    let actions = system_actions(&view, &entry.content);
    let detail_lines = system_detail_panel_lines(&view);
    let mut panels = vec![PanelSpec {
        label: "Overview".to_string(),
        lines: vec![
            "Role: System".to_string(),
            format!("Kind: {:?}", view.kind),
            format!("Summary: {}", system_message_summary(&view)),
        ],
        badges: badges.clone(),
        actions: actions.clone(),
    }];

    if !detail_lines.is_empty() {
        panels.push(PanelSpec {
            label: "Details".to_string(),
            lines: detail_lines,
            badges: badges.clone(),
            actions: actions.clone(),
        });
    }

    if let Some(raw_lines) = system_raw_panel_lines(entry, &view) {
        panels.push(PanelSpec {
            label: RAW_PANEL_LABEL.to_string(),
            lines: render_markdown_panel_lines(&raw_lines.join("\n")),
            badges,
            actions,
        });
    }

    build_document(
        "System details".to_string(),
        panels,
        Some("Esc close inspector".to_string()),
    )
}

fn build_error_entry_document(entry: &ChatEntry) -> InspectorDocument {
    let view = parse_error_view(&entry.content);
    let badges = error_badges(&view);
    let mut recovery_actions = error_recovery_actions(&view);
    recovery_actions.push(InspectorAction {
        label: "explain recovery".to_string(),
        command: "Explain the latest error and suggest the safest recovery step.".to_string(),
    });
    let mut panels = vec![PanelSpec {
        label: "Overview".to_string(),
        lines: std::iter::once(view.title.clone())
            .chain(view.detail_lines.iter().cloned())
            .collect(),
        badges: badges.clone(),
        actions: recovery_actions.clone(),
    }];
    panels.push(PanelSpec {
        label: RAW_PANEL_LABEL.to_string(),
        lines: render_markdown_panel_lines(&entry.content),
        badges,
        actions: recovery_actions,
    });

    build_document(
        "Error details".to_string(),
        panels,
        Some("Esc close inspector".to_string()),
    )
}

fn build_tool_batch_document(
    _app: &App,
    entries: &[ChatEntry],
    batch: &ToolBatch,
) -> InspectorDocument {
    let overview_badges = tool_batch_badges(entries, batch);
    let mut overview = vec![
        format!("Summary: {}", tool_batch_summary_text(batch)),
        format!("Items: {}", batch.items.len()),
        format!(
            "State: {}",
            if batch.is_active {
                "active"
            } else {
                "completed"
            }
        ),
    ];
    if let Some(progress) = tool_batch_progress_text(entries, batch) {
        overview.push(format!("Progress: {}", progress));
    } else if let Some(hint) = tool_batch_hint_text(entries, batch) {
        overview.push(format!("Hint: {}", hint));
    }

    let mut panels = vec![PanelSpec {
        label: "Overview".to_string(),
        lines: overview,
        badges: overview_badges,
        actions: vec![
            status_action(),
            tools_action(),
            InspectorAction {
                label: "summarize outcome".to_string(),
                command: "Summarize the most important outcome from the recent tool activity and suggest the next best step.".to_string(),
            },
        ],
    }];

    for (item_index, item) in batch.items.iter().enumerate() {
        let call = &entries[item.call_index];
        let args = parse_json(&call.content);
        let result_entry = item.result_index.and_then(|idx| entries.get(idx));
        let badges = combine_badges(
            tool_state_badges(result_entry),
            result_entry
                .and_then(|entry| entry.tool_metadata.as_ref())
                .map(tool_metadata_badges)
                .unwrap_or_default(),
        );
        let summary = summarize_tool_result(
            &item.tool_name,
            &args,
            result_entry.and_then(|entry| entry.tool_metadata.as_ref()),
            result_entry
                .map(|entry| entry.content.as_str())
                .unwrap_or(""),
            result_entry.is_some_and(|entry| {
                matches!(entry.role, ChatRole::ToolResult { is_error: true, .. })
            }),
        );
        let mut lines = vec![format!(
            "Activity: {}",
            describe_tool_call(&item.tool_name, &args, result_entry.is_none())
                .unwrap_or_else(|| item.tool_name.clone())
        )];
        for summary_line in summary.lines.iter() {
            lines.push(format!("Summary: {}", summary_line.text));
        }
        lines.push(String::new());
        lines.push("Arguments".to_string());
        lines.extend(json_to_lines(&args));
        if let Some(result_entry) = result_entry {
            let result_lines = render_result_content_lines(&result_entry.content, &item.tool_name);
            if !result_lines.is_empty() {
                lines.push(String::new());
                lines.push("Output".to_string());
                lines.extend(result_lines);
            }
        }

        panels.push(PanelSpec {
            label: format!("Item {}", item_index + 1),
            lines: if lines.is_empty() {
                vec!["(empty)".to_string()]
            } else {
                lines
            },
            badges,
            actions: tool_followup_actions(
                &format!("Item {}", item_index + 1),
                &item.tool_name,
                &args,
                result_entry,
            ),
        });
    }

    build_document(
        "Recent tool activity".to_string(),
        panels,
        Some("Esc close inspector".to_string()),
    )
}

fn build_system_batch_document(entries: &[ChatEntry], batch: &SystemBatch) -> InspectorDocument {
    let overview_badges = system_batch_badges(entries, batch);
    let latest_summary = batch
        .items
        .last()
        .and_then(|item| entries.get(item.entry_index))
        .map(|entry| system_message_summary(&parse_system_message(&entry.content)))
        .unwrap_or_else(|| "No recent system updates".to_string());
    let latest_actions = batch
        .items
        .last()
        .and_then(|item| entries.get(item.entry_index))
        .map(|entry| {
            let view = parse_system_message(&entry.content);
            system_actions(&view, &entry.content)
        })
        .unwrap_or_else(|| vec![status_action()]);

    let mut panels = vec![PanelSpec {
        label: "Overview".to_string(),
        lines: vec![
            format!("Summary: {} recent system updates", batch.items.len()),
            format!("Latest: {}", latest_summary),
        ],
        badges: overview_badges,
        actions: latest_actions,
    }];

    for (item_index, item) in batch.items.iter().enumerate() {
        let Some(entry) = entries.get(item.entry_index) else {
            continue;
        };
        let view = parse_system_message(&entry.content);
        let badges = system_entry_badges(&view);
        let detail_lines = system_detail_panel_lines(&view);
        let mut lines = vec![
            format!("Kind: {:?}", view.kind),
            format!("Summary: {}", system_message_summary(&view)),
        ];
        if !detail_lines.is_empty() {
            lines.push(String::new());
            lines.push("Details".to_string());
            lines.extend(detail_lines);
        }
        if let Some(raw_lines) = system_raw_panel_lines(entry, &view) {
            lines.push(String::new());
            lines.push("Raw view".to_string());
            lines.extend(render_markdown_panel_lines(&raw_lines.join("\n")));
        }

        panels.push(PanelSpec {
            label: format!("Item {}", item_index + 1),
            lines,
            badges,
            actions: system_actions(&view, &entry.content),
        });
    }

    build_document(
        "Recent system activity".to_string(),
        panels,
        Some("Esc close inspector".to_string()),
    )
}

fn build_tool_entry_document(
    app: &App,
    tool_name: &str,
    args_json: &str,
    result_entry: Option<&ChatEntry>,
) -> InspectorDocument {
    let args = parse_json(args_json);
    let is_active = result_entry.is_none();
    let title = tool_display_name(app, tool_name);
    let summary_badges = combine_badges(
        tool_state_badges(result_entry),
        result_entry
            .and_then(|entry| entry.tool_metadata.as_ref())
            .map(tool_metadata_badges)
            .unwrap_or_default(),
    );
    let activity = describe_tool_call(tool_name, &args, is_active)
        .or_else(|| {
            app.tools
                .get(tool_name)
                .map(|tool| tool.activity_description(&args))
        })
        .unwrap_or_else(|| tool_name.to_string());

    let mut summary = vec![
        format!("Tool: {}", title),
        format!("Activity: {}", activity),
        format!("State: {}", if is_active { "running" } else { "completed" }),
    ];
    if is_active {
        if let Some(progress) = parse_progress_summary(app.chat_entries.iter().rev().find(
            |entry| matches!(&entry.role, ChatRole::ToolCall { name, .. } if name == tool_name),
        )) {
            summary.push(format!("Progress: {}", progress));
        }
    }
    if let Some(result_entry) = result_entry {
        if let Some(duration) = result_entry.duration {
            summary.push(format!(
                "Duration: {}",
                crate::app::format_duration(duration)
            ));
        }
        if let Some(error_type) = result_entry.tool_error_type.as_deref() {
            summary.push(format!("Error type: {}", error_type));
        }
    }

    let actions = tool_followup_actions(&title, tool_name, &args, result_entry);
    let mut panels = vec![
        PanelSpec {
            label: "Overview".to_string(),
            lines: summary,
            badges: summary_badges.clone(),
            actions: actions.clone(),
        },
        PanelSpec {
            label: "Arguments".to_string(),
            lines: {
                let lines = json_to_lines(&args);
                if lines.is_empty() {
                    vec!["(no arguments)".to_string()]
                } else {
                    lines
                }
            },
            badges: summary_badges.clone(),
            actions: vec![InspectorAction {
                label: "reuse args".to_string(),
                command: serde_json::to_string_pretty(&args).unwrap_or_else(|_| args.to_string()),
            }],
        },
    ];

    if let Some(result_entry) = result_entry {
        if let Some(metadata) = result_entry.tool_metadata.as_ref() {
            let lines = json_to_lines(metadata);
            if !lines.is_empty() {
                panels.push(PanelSpec {
                    label: "Metadata".to_string(),
                    lines,
                    badges: summary_badges.clone(),
                    actions: actions.clone(),
                });
            }
        }
        if let Some(panel) = tool_output_panel(
            "Output",
            &result_entry.content,
            tool_name,
            summary_badges.clone(),
            actions.clone(),
        ) {
            panels.push(panel);
        }
    }

    build_document(
        format!("{} details", title),
        panels,
        Some("Esc close inspector".to_string()),
    )
}

fn build_standalone_result_document(app: &App, entry: &ChatEntry) -> InspectorDocument {
    let ChatRole::ToolResult { name, .. } = &entry.role else {
        return build_document(
            "Tool result".to_string(),
            vec![PanelSpec {
                label: "Output".to_string(),
                lines: vec![entry.content.clone()],
                badges: Vec::new(),
                actions: Vec::new(),
            }],
            None,
        );
    };
    let title = tool_display_name(app, name);
    let summary_badges = combine_badges(
        tool_state_badges(Some(entry)),
        entry
            .tool_metadata
            .as_ref()
            .map(tool_metadata_badges)
            .unwrap_or_default(),
    );
    let mut summary = vec![format!("Tool: {}", title)];
    if let Some(error_type) = entry.tool_error_type.as_deref() {
        summary.push(format!("Error type: {}", error_type));
    }
    if let Some(duration) = entry.duration {
        summary.push(format!(
            "Duration: {}",
            crate::app::format_duration(duration)
        ));
    }

    let actions = vec![
        status_action(),
        tools_action(),
        InspectorAction {
            label: "analyze result".to_string(),
            command: format!(
                "Explain the most important details from the last {} result and suggest the next step.",
                title
            ),
        },
    ];
    let mut panels = vec![PanelSpec {
        label: "Overview".to_string(),
        lines: summary,
        badges: summary_badges.clone(),
        actions: actions.clone(),
    }];
    if let Some(metadata) = entry.tool_metadata.as_ref() {
        let lines = json_to_lines(metadata);
        if !lines.is_empty() {
            panels.push(PanelSpec {
                label: "Metadata".to_string(),
                lines,
                badges: summary_badges.clone(),
                actions: actions.clone(),
            });
        }
    }
    if let Some(panel) = tool_output_panel(
        "Output",
        &entry.content,
        name,
        summary_badges,
        actions.clone(),
    ) {
        panels.push(panel);
    }

    build_document(
        format!("{} result", title),
        panels,
        Some("Esc close inspector".to_string()),
    )
}

struct PanelSpec {
    label: String,
    lines: Vec<String>,
    badges: Vec<(String, String)>,
    actions: Vec<InspectorAction>,
}

fn build_document(
    title: String,
    panels: Vec<PanelSpec>,
    footer: Option<String>,
) -> InspectorDocument {
    let tabs = panels
        .iter()
        .enumerate()
        .map(|(index, panel)| InspectorTab {
            id: format!("detail-{}", index),
            label: panel.label.clone(),
            item_count: Some(panel.lines.len()),
        })
        .collect::<Vec<_>>();
    let inspector_panels = panels
        .into_iter()
        .zip(tabs.iter().cloned())
        .map(|(panel, tab)| InspectorPanel {
            tab: InspectorTab {
                label: panel.label,
                ..tab
            },
            lines: panel.lines,
            badges: panel.badges,
            actions: panel.actions,
        })
        .collect::<Vec<_>>();

    InspectorDocument {
        state: InspectorState::new(title, tabs),
        panels: inspector_panels,
        footer,
    }
}

fn markdown_panel(
    label: &str,
    content: &str,
    badges: Vec<(String, String)>,
    actions: Vec<InspectorAction>,
) -> Option<PanelSpec> {
    let lines = render_markdown_panel_lines(content);
    (!lines.is_empty()).then(|| PanelSpec {
        label: label.to_string(),
        lines,
        badges,
        actions,
    })
}

fn dim_markdown_panel(
    label: &str,
    content: &str,
    badges: Vec<(String, String)>,
    actions: Vec<InspectorAction>,
) -> Option<PanelSpec> {
    let lines = render_dim_markdown_panel_lines(content);
    (!lines.is_empty()).then(|| PanelSpec {
        label: label.to_string(),
        lines,
        badges,
        actions,
    })
}

fn tool_output_panel(
    label: &str,
    content: &str,
    tool_name: &str,
    badges: Vec<(String, String)>,
    actions: Vec<InspectorAction>,
) -> Option<PanelSpec> {
    let lines = render_result_content_lines(content, tool_name);
    (!lines.is_empty()).then(|| PanelSpec {
        label: label.to_string(),
        lines,
        badges,
        actions,
    })
}

fn render_markdown_panel_lines(content: &str) -> Vec<String> {
    render_markdown_ansi_white_with_options(content, Some(INSPECTOR_MARKDOWN_WIDTH), true)
}

fn render_dim_markdown_panel_lines(content: &str) -> Vec<String> {
    render_markdown_ansi_dim_with_options(content, Some(INSPECTOR_MARKDOWN_WIDTH), true)
}

fn system_detail_panel_lines(view: &crate::system_message::SystemMessageView) -> Vec<String> {
    view.detail_lines
        .iter()
        .map(|line| format_system_detail_line(line))
        .collect()
}

fn assistant_overview_badges(entry: &ChatEntry) -> Vec<(String, String)> {
    let mut badges = vec![(
        "content".to_string(),
        format!("{} lines", entry.content.lines().count()),
    )];
    if let Some(reasoning) = entry
        .reasoning
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        badges.push((
            "reasoning".to_string(),
            format!("{} lines", reasoning.lines().count()),
        ));
    } else {
        badges.push(("reasoning".to_string(), "none".to_string()));
    }
    badges
}

fn assistant_content_badges(entry: &ChatEntry) -> Vec<(String, String)> {
    vec![
        ("kind".to_string(), "response".to_string()),
        (
            "content".to_string(),
            format!("{} lines", entry.content.lines().count()),
        ),
    ]
}

fn assistant_reasoning_badges(reasoning: &str) -> Vec<(String, String)> {
    vec![
        ("kind".to_string(), "reasoning".to_string()),
        (
            "summary".to_string(),
            format!("{} lines", reasoning.lines().count()),
        ),
    ]
}

fn system_raw_panel_lines(
    entry: &ChatEntry,
    view: &crate::system_message::SystemMessageView,
) -> Option<Vec<String>> {
    let raw_lines = entry
        .content
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    let has_structured_split = !view.detail_lines.is_empty();
    let has_multiline_raw = raw_lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .count()
        > 1;
    (has_structured_split || has_multiline_raw).then_some(raw_lines)
}

fn confirm_actions() -> Vec<InspectorAction> {
    vec![
        InspectorAction {
            label: "allow once".to_string(),
            command: INSPECTOR_CONFIRM_ALLOW.to_string(),
        },
        InspectorAction {
            label: "always allow".to_string(),
            command: INSPECTOR_CONFIRM_ALWAYS.to_string(),
        },
        InspectorAction {
            label: "deny".to_string(),
            command: INSPECTOR_CONFIRM_DENY.to_string(),
        },
    ]
}

fn status_action() -> InspectorAction {
    InspectorAction {
        label: "show status".to_string(),
        command: "/status".to_string(),
    }
}

fn tools_action() -> InspectorAction {
    InspectorAction {
        label: "show tools".to_string(),
        command: "/tools".to_string(),
    }
}

fn timeline_action() -> InspectorAction {
    InspectorAction {
        label: "open timeline".to_string(),
        command: "/inspect artifact history runtime".to_string(),
    }
}

fn bundle_action() -> InspectorAction {
    InspectorAction {
        label: "open bundle".to_string(),
        command: "/inspect artifact bundle".to_string(),
    }
}

fn diagnostics_action() -> InspectorAction {
    InspectorAction {
        label: "show diagnostics".to_string(),
        command: "/diagnostics".to_string(),
    }
}

fn help_action() -> InspectorAction {
    InspectorAction {
        label: "open help".to_string(),
        command: "/help".to_string(),
    }
}

fn compact_action() -> InspectorAction {
    InspectorAction {
        label: "run /compact".to_string(),
        command: "/compact".to_string(),
    }
}

fn clear_action() -> InspectorAction {
    InspectorAction {
        label: "run /clear".to_string(),
        command: "/clear".to_string(),
    }
}

fn model_action() -> InspectorAction {
    InspectorAction {
        label: "open model".to_string(),
        command: "/model".to_string(),
    }
}

fn provider_action() -> InspectorAction {
    InspectorAction {
        label: "open provider".to_string(),
        command: "/provider".to_string(),
    }
}

fn doctor_action() -> InspectorAction {
    InspectorAction {
        label: "run /doctor".to_string(),
        command: "/doctor".to_string(),
    }
}

fn system_actions(
    view: &crate::system_message::SystemMessageView,
    raw_content: &str,
) -> Vec<InspectorAction> {
    match view.kind {
        crate::system_message::SystemMessageKind::Export
            if view.title.to_ascii_lowercase().contains("bundle exported") =>
        {
            vec![bundle_action(), status_action()]
        }
        crate::system_message::SystemMessageKind::Export => vec![status_action()],
        crate::system_message::SystemMessageKind::Turn => {
            vec![status_action(), diagnostics_action(), timeline_action()]
        }
        crate::system_message::SystemMessageKind::Warning
            if raw_content.starts_with("Unknown command:") =>
        {
            vec![help_action(), status_action()]
        }
        crate::system_message::SystemMessageKind::Task
            if raw_content.to_ascii_lowercase().contains("hook") =>
        {
            vec![
                InspectorAction {
                    label: "open hooks".to_string(),
                    command: "/hooks".to_string(),
                },
                status_action(),
            ]
        }
        crate::system_message::SystemMessageKind::Lifecycle => vec![status_action()],
        _ => vec![status_action(), timeline_action()],
    }
}

fn error_recovery_actions(view: &ErrorView) -> Vec<InspectorAction> {
    match view.kind {
        ErrorKind::ContextLimit => vec![compact_action(), clear_action(), status_action()],
        ErrorKind::Authentication => vec![provider_action(), model_action(), doctor_action()],
        ErrorKind::RateLimit => vec![model_action(), provider_action(), status_action()],
        ErrorKind::ProviderRejected => vec![provider_action(), doctor_action(), status_action()],
        ErrorKind::ProviderTransport => vec![provider_action(), model_action(), status_action()],
        ErrorKind::Timeout => vec![model_action(), compact_action(), status_action()],
        ErrorKind::Generic => vec![status_action(), doctor_action()],
    }
}

fn tool_followup_actions(
    title: &str,
    tool_name: &str,
    args: &serde_json::Value,
    result_entry: Option<&ChatEntry>,
) -> Vec<InspectorAction> {
    let mut actions = vec![status_action(), tools_action()];

    if let Some(prompt) = primary_followup_prompt(title, tool_name, args, result_entry) {
        actions.push(InspectorAction {
            label: "plan next step".to_string(),
            command: prompt,
        });
    }

    actions
}

fn primary_followup_prompt(
    title: &str,
    tool_name: &str,
    args: &serde_json::Value,
    result_entry: Option<&ChatEntry>,
) -> Option<String> {
    if result_entry
        .is_some_and(|entry| matches!(entry.role, ChatRole::ToolResult { is_error: true, .. }))
    {
        return Some(format!(
            "Why did the last {} step fail? Give the safest next action.",
            title
        ));
    }

    if let Some(file_path) = args
        .get("file_path")
        .or_else(|| args.get("path"))
        .and_then(|value| value.as_str())
    {
        return Some(format!(
            "Inspect {}. Summarize the last {} step and the next action.",
            compact_path(file_path),
            tool_name
        ));
    }

    if let Some(url) = args.get("url").and_then(|value| value.as_str()) {
        return Some(format!("Summarize the key findings from {}.", url));
    }

    if let Some(query) = args.get("query").and_then(|value| value.as_str()) {
        return Some(format!(
            "Continue the web search for '{}' and summarize the best findings.",
            query
        ));
    }

    if let Some(command) = args.get("command").and_then(|value| value.as_str()) {
        return Some(format!(
            "Review this command and suggest the safest next step:\n{}",
            command
        ));
    }

    Some(format!(
        "Summarize the last {} step and give the next best action.",
        title
    ))
}

fn parse_json(raw: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or(serde_json::Value::Null)
}

fn json_to_lines(value: &serde_json::Value) -> Vec<String> {
    match value {
        serde_json::Value::Null => Vec::new(),
        _ => serde_json::to_string_pretty(value)
            .unwrap_or_else(|_| value.to_string())
            .lines()
            .map(|line| line.to_string())
            .collect(),
    }
}

fn render_result_content_lines(content: &str, tool_name: &str) -> Vec<String> {
    if matches!(tool_name, "bash" | "powershell") {
        let sections = parse_shell_output_sections(content);
        let mut lines = Vec::new();
        if !sections.stdout_lines.is_empty() {
            lines.push("stdout".to_string());
            lines.extend(sections.stdout_lines);
        }
        if !sections.stderr_lines.is_empty() {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push("stderr".to_string());
            lines.extend(sections.stderr_lines);
        }
        if let Some(exit_code) = sections.exit_code {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.push(format!("exit code {}", exit_code));
        }
        lines
    } else {
        render_markdown_panel_lines(content)
    }
}

fn combine_badges(
    mut badges: Vec<(String, String)>,
    extra: Vec<(String, String)>,
) -> Vec<(String, String)> {
    badges.extend(extra);
    badges
}

fn tool_state_badges(result_entry: Option<&ChatEntry>) -> Vec<(String, String)> {
    let state = if result_entry.is_none() {
        "running"
    } else if result_entry.is_some_and(tool_result_failed) {
        "failed"
    } else {
        "completed"
    };

    let mut badges = vec![("state".to_string(), state.to_string())];
    if result_entry.is_some_and(tool_result_failed) {
        badges.push(("severity".to_string(), "error".to_string()));
    }
    badges
}

fn tool_batch_badges(entries: &[ChatEntry], batch: &ToolBatch) -> Vec<(String, String)> {
    let mut badges = vec![(
        "state".to_string(),
        if batch.is_active {
            "active".to_string()
        } else {
            "completed".to_string()
        },
    )];
    if batch
        .items
        .iter()
        .filter_map(|item| item.result_index.and_then(|index| entries.get(index)))
        .any(tool_result_failed)
    {
        badges.push(("severity".to_string(), "error".to_string()));
    }
    badges
}

fn system_entry_badges(view: &crate::system_message::SystemMessageView) -> Vec<(String, String)> {
    vec![
        (
            "kind".to_string(),
            system_kind_badge_value(view.kind).to_string(),
        ),
        (
            "severity".to_string(),
            system_severity_badge_value(view).to_string(),
        ),
    ]
}

fn system_batch_badges(entries: &[ChatEntry], batch: &SystemBatch) -> Vec<(String, String)> {
    let has_warning = batch.items.iter().any(|item| {
        entries
            .get(item.entry_index)
            .map(|entry| parse_system_message(&entry.content))
            .is_some_and(|view| system_severity_badge_value(&view) == "warning")
    });
    vec![
        ("state".to_string(), "batched".to_string()),
        (
            "severity".to_string(),
            if has_warning { "warning" } else { "info" }.to_string(),
        ),
    ]
}

fn error_badges(view: &ErrorView) -> Vec<(String, String)> {
    vec![
        ("state".to_string(), "failed".to_string()),
        (
            "severity".to_string(),
            error_severity_badge_value(view).to_string(),
        ),
    ]
}

fn tool_result_failed(entry: &ChatEntry) -> bool {
    matches!(entry.role, ChatRole::ToolResult { is_error: true, .. })
}

fn system_kind_badge_value(kind: crate::system_message::SystemMessageKind) -> &'static str {
    match kind {
        crate::system_message::SystemMessageKind::Context => "context",
        crate::system_message::SystemMessageKind::Memory => "memory",
        crate::system_message::SystemMessageKind::Budget => "budget",
        crate::system_message::SystemMessageKind::Export => "export",
        crate::system_message::SystemMessageKind::Task => "task",
        crate::system_message::SystemMessageKind::Turn => "turn",
        crate::system_message::SystemMessageKind::Warning => "warning",
        crate::system_message::SystemMessageKind::Lifecycle => "lifecycle",
        crate::system_message::SystemMessageKind::Plan => "plan",
        crate::system_message::SystemMessageKind::Update => "update",
        crate::system_message::SystemMessageKind::Generic => "generic",
    }
}

fn system_severity_badge_value(view: &crate::system_message::SystemMessageView) -> &'static str {
    match view.kind {
        crate::system_message::SystemMessageKind::Budget
        | crate::system_message::SystemMessageKind::Warning => "warning",
        crate::system_message::SystemMessageKind::Task
            if view.title.to_ascii_lowercase().contains("warn") =>
        {
            "warning"
        }
        _ => "info",
    }
}

fn error_severity_badge_value(view: &ErrorView) -> &'static str {
    match view.kind {
        ErrorKind::ContextLimit
        | ErrorKind::RateLimit
        | ErrorKind::ProviderTransport
        | ErrorKind::Timeout => "warning",
        ErrorKind::Authentication | ErrorKind::ProviderRejected | ErrorKind::Generic => "error",
    }
}

fn tool_metadata_badges(metadata: &serde_json::Value) -> Vec<(String, String)> {
    let mut badges = Vec::new();

    if metadata
        .get("read_only_reason")
        .and_then(|value| value.as_str())
        .is_some()
        || metadata
            .get("read_only")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
    {
        badges.push(("access".to_string(), "read-only".to_string()));
    }

    if let Some(command_type) = metadata
        .get("command_type")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty() && *value != "generic")
    {
        badges.push(("mode".to_string(), command_type.to_string()));
    }

    if metadata
        .get("destructive_warning")
        .and_then(|value| value.as_str())
        .is_some()
    {
        badges.push(("warning".to_string(), "destructive".to_string()));
    }

    if metadata
        .get("rewrite_suggestion")
        .and_then(|value| value.as_str())
        .is_some()
    {
        badges.push(("hint".to_string(), "rewrite".to_string()));
    }

    if let Some(diff) = diff_preview_badge(metadata) {
        badges.push(("diff".to_string(), diff));
    }

    if metadata
        .get("tool_runtime")
        .and_then(|value| value.get("truncation"))
        .and_then(|value| value.get("reason"))
        .and_then(|value| value.as_str())
        .is_some()
    {
        badges.push(("output".to_string(), "truncated".to_string()));
    }

    badges
}

fn diff_preview_badge(metadata: &serde_json::Value) -> Option<String> {
    let diff = metadata.get("diff_preview")?.as_object()?;
    let removed = diff
        .get("removed")
        .and_then(|value| value.as_array())
        .map(|items| items.len())
        .unwrap_or(0);
    let added = diff
        .get("added")
        .and_then(|value| value.as_array())
        .map(|items| items.len())
        .unwrap_or(0);

    if added == 0 && removed == 0 {
        None
    } else {
        Some(format!("+{}/-{}", added, removed))
    }
}

fn parse_progress_summary(entry: Option<&ChatEntry>) -> Option<String> {
    let progress = entry?.progress.as_ref()?;
    let mut text = progress.message.clone();
    if let Some(percent) = progress.percent {
        text.push_str(&format!(" {}%", percent));
    }
    Some(text)
}

fn tool_display_name(app: &App, tool_name: &str) -> String {
    if let Some(tool) = app.tools.get(tool_name) {
        let label = tool.user_facing_name();
        if !label.trim().is_empty() {
            return label.to_string();
        }
    }
    human_tool_display_name(tool_name)
}

fn tool_risk_hint(app: &App, tool_name: &str) -> Option<String> {
    if let Some(tool) = app.tools.get(tool_name) {
        if tool.capabilities().read_only {
            return Some("read-only".to_string());
        }
    }

    let hint = match tool_name {
        "edit_file" | "write_file" | "multi_edit" | "notebook_edit" => "changes files",
        "bash" | "powershell" => "shell access",
        "web_search" | "web_fetch" | "web_browser" => "network access",
        "git_commit" => "git write",
        "agent" | "send_message" | "team_create" => "agent action",
        _ => return None,
    };
    Some(hint.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use tokio::sync::Mutex;
    use yode_llm::registry::ProviderRegistry;
    use yode_tools::builtin::skill::SkillStore;
    use yode_tools::builtin::{register_builtin_tools, register_skill_tool};
    use yode_tools::registry::ToolRegistry;

    use crate::app::rendering::strip_ansi;
    use crate::app::{App, ChatEntry, ChatRole, PendingConfirmation};

    use super::{
        build_latest_tool_document, build_pending_confirmation_document, help_action,
        primary_followup_prompt, status_action, INSPECTOR_CONFIRM_ALLOW,
    };

    fn test_app() -> App {
        let registry = Arc::new(ToolRegistry::new());
        register_builtin_tools(&registry);
        register_skill_tool(&registry, Arc::new(Mutex::new(SkillStore::new())));
        App::new(
            "test-model".to_string(),
            "session-1234".to_string(),
            "/tmp".to_string(),
            "test".to_string(),
            Vec::new(),
            HashMap::new(),
            Arc::new(ProviderRegistry::new()),
            registry,
        )
    }

    #[test]
    fn pending_confirmation_document_surfaces_activity_and_args() {
        let app = test_app();
        let confirm = PendingConfirmation {
            id: "a".to_string(),
            name: "read_file".to_string(),
            arguments: r#"{"file_path":"/tmp/src/main.rs"}"#.to_string(),
        };
        let doc = build_pending_confirmation_document(&app, &confirm);
        assert_eq!(doc.panels[0].tab.label, "Overview");
        assert!(doc.panels[0]
            .lines
            .iter()
            .any(|line| line.contains("Reading .../src/main.rs")));
        assert!(doc.panels[1]
            .lines
            .iter()
            .any(|line| line.contains("/tmp/src/main.rs")));
        assert!(doc.panels[0]
            .actions
            .iter()
            .any(|action| action.command == INSPECTOR_CONFIRM_ALLOW));
    }

    #[test]
    fn latest_tool_document_builds_from_recent_tool_call() {
        let mut app = test_app();
        let mut result = ChatEntry::new(
            ChatRole::ToolResult {
                id: "a".to_string(),
                name: "bash".to_string(),
                is_error: true,
            },
            "ok\n[stderr]\nwarn\n[exit code: 2]".to_string(),
        );
        result.tool_metadata = Some(serde_json::json!({
            "rewrite_suggestion": "Prefer read_file"
        }));
        app.chat_entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "bash".to_string(),
                },
                r#"{"command":"cat Cargo.toml"}"#.to_string(),
            ),
            result,
        ];

        let doc = build_latest_tool_document(&app).unwrap();
        assert!(doc.state.title.contains("Bash"));
        assert_eq!(doc.panels[0].tab.label, "Overview");
        assert!(doc
            .panels
            .iter()
            .any(|panel| panel.tab.label == "Arguments"));
        assert!(doc.panels.iter().any(|panel| panel.tab.label == "Output"));
        assert!(doc.panels[0]
            .actions
            .iter()
            .any(|action| action.command == "/status"));
        assert!(doc.panels[0]
            .badges
            .contains(&("state".to_string(), "failed".to_string())));
        assert!(doc.panels[0]
            .badges
            .contains(&("severity".to_string(), "error".to_string())));
        assert!(doc.panels[0]
            .actions
            .iter()
            .any(|action| action.label == "plan next step"));
    }

    #[test]
    fn latest_tool_document_surfaces_metadata_badges_across_panels() {
        let mut app = test_app();
        let mut result = ChatEntry::new(
            ChatRole::ToolResult {
                id: "a".to_string(),
                name: "bash".to_string(),
                is_error: false,
            },
            "ok".to_string(),
        );
        result.tool_metadata = Some(serde_json::json!({
            "read_only_reason": "validated git status",
            "command_type": "read",
            "destructive_warning": "may discard changes",
            "rewrite_suggestion": "Prefer read_file",
            "diff_preview": {
                "removed": ["old-a", "old-b"],
                "added": ["new-a"]
            },
            "tool_runtime": {
                "truncation": {
                    "reason": "line budget"
                }
            }
        }));
        app.chat_entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "bash".to_string(),
                },
                r#"{"command":"cat Cargo.toml"}"#.to_string(),
            ),
            result,
        ];

        let doc = build_latest_tool_document(&app).unwrap();
        let summary = &doc.panels[0];
        assert!(summary
            .badges
            .contains(&("state".to_string(), "completed".to_string())));
        assert!(summary
            .badges
            .contains(&("access".to_string(), "read-only".to_string())));
        assert!(summary
            .badges
            .contains(&("mode".to_string(), "read".to_string())));
        assert!(summary
            .badges
            .contains(&("warning".to_string(), "destructive".to_string())));
        assert!(summary
            .badges
            .contains(&("hint".to_string(), "rewrite".to_string())));
        assert!(summary
            .badges
            .contains(&("diff".to_string(), "+1/-2".to_string())));
        assert!(summary
            .badges
            .contains(&("output".to_string(), "truncated".to_string())));

        let output = doc
            .panels
            .iter()
            .find(|panel| panel.tab.label == "Output")
            .expect("output panel");
        assert_eq!(output.badges, summary.badges);
    }

    #[test]
    fn tool_output_panel_renders_markdown_aware_lines() {
        let mut app = test_app();
        app.chat_entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                },
                r#"{"file_path":"/tmp/src/main.rs"}"#.to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "# Heading\n\n| A | B |\n| --- | --- |\n| 1 | 2 |".to_string(),
            ),
        ];

        let doc = build_latest_tool_document(&app).unwrap();
        let output_panel = doc
            .panels
            .iter()
            .find(|panel| panel.tab.label == "Output")
            .expect("output panel");
        let rendered = output_panel
            .lines
            .iter()
            .map(|line| strip_ansi(line))
            .collect::<Vec<_>>();
        assert!(rendered.iter().any(|line| line.contains("Heading")));
        assert!(rendered.iter().all(|line| line != "| A | B |"));
        assert!(rendered.iter().all(|line| line != "| 1 | 2 |"));
    }

    #[test]
    fn latest_tool_document_surfaces_active_batch_progress() {
        let mut app = test_app();
        let mut active = ChatEntry::new(
            ChatRole::ToolCall {
                id: "a".to_string(),
                name: "read_file".to_string(),
            },
            r#"{"file_path":"/tmp/src/main.rs"}"#.to_string(),
        );
        active.progress = Some(yode_tools::tool::ToolProgress {
            message: "chunk 2/4".to_string(),
            percent: Some(50),
        });
        app.chat_entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "z".to_string(),
                    name: "grep".to_string(),
                },
                r#"{"pattern":"retry"}"#.to_string(),
            ),
            active,
        ];

        let doc = build_latest_tool_document(&app).unwrap();
        assert_eq!(doc.state.title, "Recent tool activity");
        assert!(doc.panels[0]
            .badges
            .contains(&("state".to_string(), "active".to_string())));
        assert!(doc.panels.first().is_some_and(|panel| panel
            .lines
            .iter()
            .any(|line| line.contains("Progress: chunk 2/4 50%"))));
    }

    #[test]
    fn latest_tool_document_prefers_grouped_tool_batch_for_completed_recent_batch() {
        let mut app = test_app();
        app.chat_entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                },
                r#"{"file_path":"/tmp/src/a.rs"}"#.to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "alpha".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                },
                r#"{"file_path":"/tmp/src/b.rs"}"#.to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "beta".to_string(),
            ),
        ];

        let doc = build_latest_tool_document(&app).unwrap();
        assert_eq!(doc.state.title, "Recent tool activity");
        assert_eq!(doc.panels[0].tab.label, "Overview");
        assert!(doc.panels[0]
            .badges
            .contains(&("state".to_string(), "completed".to_string())));
        assert!(doc.panels.iter().any(|panel| panel.tab.label == "Item 1"));
        assert!(doc.panels.iter().any(|panel| panel.tab.label == "Item 2"));
        assert!(doc.panels[0]
            .lines
            .iter()
            .any(|line| line.contains("Items: 2")));
    }

    #[test]
    fn latest_tool_document_prefers_grouped_system_batch_for_recent_updates() {
        let mut app = test_app();
        app.chat_entries = vec![
            ChatEntry::new(
                ChatRole::System,
                "Session memory updated · summary · /tmp/project/live.md".to_string(),
            ),
            ChatEntry::new(
                ChatRole::System,
                "Diagnostics bundle exported to: /tmp/project/.yode/exports/diag.zip".to_string(),
            ),
        ];

        let doc = build_latest_tool_document(&app).unwrap();
        assert_eq!(doc.state.title, "Recent system activity");
        assert_eq!(doc.panels[0].tab.label, "Overview");
        assert!(doc.panels[0]
            .badges
            .contains(&("state".to_string(), "batched".to_string())));
        assert!(doc.panels.iter().any(|panel| panel.tab.label == "Item 1"));
        assert!(doc.panels.iter().any(|panel| panel.tab.label == "Item 2"));
        assert!(doc.panels[0]
            .lines
            .iter()
            .any(|line| line.contains("Latest: Diagnostics bundle exported")));
    }

    #[test]
    fn latest_tool_document_prefers_latest_logical_message_after_batches() {
        let mut app = test_app();
        app.chat_entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                },
                r#"{"file_path":"/tmp/src/a.rs"}"#.to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "alpha".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                },
                r#"{"file_path":"/tmp/src/b.rs"}"#.to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "beta".to_string(),
            ),
            ChatEntry::new(ChatRole::Assistant, "Final answer".to_string()),
        ];

        let doc = build_latest_tool_document(&app).unwrap();
        assert_eq!(doc.state.title, "Assistant details");
    }

    #[test]
    fn latest_tool_document_prefers_recent_assistant_reasoning_when_present() {
        let mut app = test_app();
        app.chat_entries = vec![ChatEntry::new_with_reasoning(
            ChatRole::Assistant,
            "Final answer".to_string(),
            Some("## Plan\n- inspect\n- patch".to_string()),
        )];

        let doc = build_latest_tool_document(&app).unwrap();
        assert_eq!(doc.state.title, "Assistant details");
        assert_eq!(doc.panels[0].tab.label, "Overview");
        assert!(doc.panels[0]
            .badges
            .contains(&("reasoning".to_string(), "3 lines".to_string())));
        assert!(doc
            .panels
            .iter()
            .any(|panel| panel.tab.label == "Reasoning"));
        assert!(doc
            .panels
            .iter()
            .find(|panel| panel.tab.label == "Reasoning")
            .is_some_and(|panel| panel
                .badges
                .contains(&("summary".to_string(), "3 lines".to_string()))));
        assert!(doc
            .panels
            .iter()
            .any(|panel| panel.lines.iter().any(|line| line.contains("Plan"))));
        assert!(doc
            .panels
            .iter()
            .any(|panel| panel.lines.iter().any(|line| line.contains("inspect"))));
    }

    #[test]
    fn latest_tool_document_supports_recent_assistant_without_reasoning() {
        let mut app = test_app();
        app.chat_entries = vec![ChatEntry::new(
            ChatRole::Assistant,
            "# Title\n\nBody".to_string(),
        )];

        let doc = build_latest_tool_document(&app).unwrap();
        assert_eq!(doc.state.title, "Assistant details");
        assert_eq!(doc.panels[0].tab.label, "Overview");
        assert!(doc.panels[0]
            .badges
            .contains(&("reasoning".to_string(), "none".to_string())));
        assert!(doc.panels.iter().any(|panel| panel.tab.label == "Content"));
        assert!(doc
            .panels
            .iter()
            .find(|panel| panel.tab.label == "Content")
            .is_some_and(|panel| panel
                .badges
                .contains(&("kind".to_string(), "response".to_string()))));
        assert!(doc
            .panels
            .iter()
            .any(|panel| panel.lines.iter().any(|line| line.contains("Title"))));
    }

    #[test]
    fn latest_tool_document_supports_recent_system_entry() {
        let mut app = test_app();
        app.chat_entries = vec![ChatEntry::new(
            ChatRole::System,
            "Session memory updated · summary · /Users/pyu/code/yode/.yode/memory/live.md"
                .to_string(),
        )];

        let doc = build_latest_tool_document(&app).unwrap();
        assert_eq!(doc.state.title, "System details");
        assert_eq!(doc.panels[0].tab.label, "Overview");
        assert!(doc.panels.iter().any(|panel| panel.tab.label == "Details"));
        assert!(doc.panels.iter().any(|panel| panel.tab.label == "Raw view"));
        assert!(doc.panels[0]
            .badges
            .contains(&("kind".to_string(), "memory".to_string())));
        let details = doc
            .panels
            .iter()
            .find(|panel| panel.tab.label == "Details")
            .expect("details");
        assert!(details
            .lines
            .iter()
            .any(|line| line.contains("summary · .../memory/live.md")));
        let raw = doc
            .panels
            .iter()
            .find(|panel| panel.tab.label == "Raw view")
            .expect("raw");
        assert!(raw
            .lines
            .iter()
            .any(|line| line.contains("/Users/pyu/code/yode/.yode/memory/live.md")));
        assert!(doc.panels[0]
            .actions
            .iter()
            .any(|action| action.label == "show status"));
    }

    #[test]
    fn latest_tool_document_hyperlinks_system_raw_urls() {
        let mut app = test_app();
        app.chat_entries = vec![ChatEntry::new(
            ChatRole::System,
            "Session memory updated · ref · https://example.com/docs".to_string(),
        )];

        let doc = build_latest_tool_document(&app).unwrap();
        let raw = doc
            .panels
            .iter()
            .find(|panel| panel.tab.label == "Raw view")
            .expect("raw");
        assert!(raw
            .lines
            .iter()
            .any(|line| line.contains("\u{1b}]8;;https://example.com/docs")));
    }

    #[test]
    fn latest_tool_document_adds_bundle_action_for_export_system_entry() {
        let mut app = test_app();
        app.chat_entries = vec![ChatEntry::new(
            ChatRole::System,
            "Diagnostics bundle exported to: /tmp/bundle".to_string(),
        )];

        let doc = build_latest_tool_document(&app).unwrap();
        assert!(doc.panels[0]
            .actions
            .iter()
            .any(|action| action.label == "open bundle"));
    }

    #[test]
    fn latest_tool_document_adds_diagnostics_action_for_turn_summary() {
        let mut app = test_app();
        app.chat_entries = vec![ChatEntry::new(
            ChatRole::System,
            "Turn completed · 1.4s · 3 tools · 1.2k↑ 180↓ tok".to_string(),
        )];

        let doc = build_latest_tool_document(&app).unwrap();
        assert!(doc.panels[0]
            .actions
            .iter()
            .any(|action| action.label == "show diagnostics"));
    }

    #[test]
    fn latest_tool_document_adds_hooks_action_for_hook_task_system_entry() {
        let mut app = test_app();
        app.chat_entries = vec![ChatEntry::new(
            ChatRole::System,
            "[Task:warn] hook timeout: scripts/pre-tool".to_string(),
        )];

        let doc = build_latest_tool_document(&app).unwrap();
        assert!(doc.panels[0]
            .actions
            .iter()
            .any(|action| action.label == "open hooks"));
    }

    #[test]
    fn latest_tool_document_supports_recent_error_entry() {
        let mut app = test_app();
        app.chat_entries = vec![ChatEntry::new(
            ChatRole::Error,
            "OpenAI API error (400): This model's maximum context length is 128000 tokens."
                .to_string(),
        )];

        let doc = build_latest_tool_document(&app).unwrap();
        assert_eq!(doc.state.title, "Error details");
        assert_eq!(doc.panels[0].tab.label, "Overview");
        assert!(doc.panels[0]
            .badges
            .contains(&("state".to_string(), "failed".to_string())));
        assert!(doc.panels[0]
            .badges
            .contains(&("severity".to_string(), "warning".to_string())));
        assert!(doc.panels[0]
            .actions
            .iter()
            .any(|action| action.label == "run /compact"));
        assert!(doc.panels[0]
            .actions
            .iter()
            .any(|action| action.label == "run /clear"));
        assert!(doc.panels.iter().any(|panel| panel
            .lines
            .iter()
            .any(|line| line.contains("Context limit reached"))));
    }

    #[test]
    fn latest_tool_document_hyperlinks_error_raw_urls() {
        let mut app = test_app();
        app.chat_entries = vec![ChatEntry::new(
            ChatRole::Error,
            "something odd happened\nsee https://example.com/help".to_string(),
        )];

        let doc = build_latest_tool_document(&app).unwrap();
        let raw = doc
            .panels
            .iter()
            .find(|panel| panel.tab.label == "Raw view")
            .expect("raw");
        assert!(raw
            .lines
            .iter()
            .any(|line| line.contains("\u{1b}]8;;https://example.com/help")));
    }

    #[test]
    fn latest_tool_document_specializes_auth_recovery_actions() {
        let mut app = test_app();
        app.chat_entries = vec![ChatEntry::new(
            ChatRole::Error,
            "Anthropic API error (401): invalid api key".to_string(),
        )];

        let doc = build_latest_tool_document(&app).unwrap();
        assert!(doc.panels[0]
            .actions
            .iter()
            .any(|action| action.label == "open provider"));
        assert!(doc.panels[0]
            .actions
            .iter()
            .any(|action| action.label == "open model"));
        assert!(doc.panels[0]
            .actions
            .iter()
            .any(|action| action.label == "run /doctor"));
    }

    #[test]
    fn follow_up_prompt_compacts_file_paths() {
        let args = serde_json::json!({"file_path": "/tmp/src/main.rs"});
        let prompt = primary_followup_prompt("Read", "read_file", &args, None).unwrap();
        assert!(prompt.contains(".../src/main.rs"));
    }

    #[test]
    fn inspector_status_and_help_actions_use_consistent_casing() {
        let status = status_action();
        let help = help_action();
        assert_eq!(status.label, "show status");
        assert_eq!(status.command, "/status");
        assert_eq!(help.label, "open help");
        assert_eq!(help.command, "/help");
    }

    #[test]
    fn follow_up_prompts_are_compact_in_transcript_style() {
        let file_args = serde_json::json!({"file_path": "/tmp/src/main.rs"});
        let file_prompt = primary_followup_prompt("Read", "read_file", &file_args, None).unwrap();
        assert!(file_prompt.contains("Summarize the last read_file step"));
        assert!(!file_prompt.contains("most relevant details"));

        let query_args = serde_json::json!({"query": "ratatui tables"});
        let query_prompt =
            primary_followup_prompt("Web Search", "web_search", &query_args, None).unwrap();
        assert!(query_prompt.contains("Continue the web search"));
        assert!(!query_prompt.contains("highest-signal"));
    }

    #[test]
    fn print_inspector_regression_snapshot() {
        let mut assistant_app = test_app();
        assistant_app.chat_entries = vec![ChatEntry::new_with_reasoning(
            ChatRole::Assistant,
            "Final answer".to_string(),
            Some("## Plan\n- inspect\n- patch".to_string()),
        )];
        let assistant = build_latest_tool_document(&assistant_app).unwrap();

        let mut tool_app = test_app();
        let mut tool_result = ChatEntry::new(
            ChatRole::ToolResult {
                id: "a".to_string(),
                name: "bash".to_string(),
                is_error: false,
            },
            "ok".to_string(),
        );
        tool_result.tool_metadata = Some(serde_json::json!({
            "read_only_reason": "validated git status",
            "rewrite_suggestion": "Prefer read_file"
        }));
        tool_app.chat_entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "bash".to_string(),
                },
                r#"{"command":"cat Cargo.toml"}"#.to_string(),
            ),
            tool_result,
        ];
        let tool = build_latest_tool_document(&tool_app).unwrap();

        let mut system_app = test_app();
        system_app.chat_entries = vec![ChatEntry::new(
            ChatRole::System,
            "Diagnostics bundle exported to: /tmp/bundle".to_string(),
        )];
        let system = build_latest_tool_document(&system_app).unwrap();

        let mut error_app = test_app();
        error_app.chat_entries = vec![ChatEntry::new(
            ChatRole::Error,
            "OpenAI API error (400): This model's maximum context length is 128000 tokens."
                .to_string(),
        )];
        let error = build_latest_tool_document(&error_app).unwrap();

        println!("# Inspector Regression Snapshot\n");
        print_doc("Assistant", &assistant);
        print_doc("Tool", &tool);
        print_doc("System", &system);
        print_doc("Error", &error);
    }

    fn print_doc(label: &str, doc: &crate::ui::inspector::InspectorDocument) {
        println!("## {}\n", label);
        println!("title: {}", doc.state.title);
        for panel in &doc.panels {
            println!("panel: {}", panel.tab.label);
            if !panel.badges.is_empty() {
                println!(
                    "badges: {}",
                    panel
                        .badges
                        .iter()
                        .map(|(k, v)| format!("{}={}", k, v))
                        .collect::<Vec<_>>()
                        .join(" | ")
                );
            }
            if !panel.actions.is_empty() {
                println!(
                    "actions: {}",
                    panel
                        .actions
                        .iter()
                        .map(|action| action.label.clone())
                        .collect::<Vec<_>>()
                        .join(" | ")
                );
            }
            if let Some(first) = panel.lines.first() {
                println!("first: {}", crate::app::rendering::strip_ansi(first));
            }
            println!();
        }
    }
}
