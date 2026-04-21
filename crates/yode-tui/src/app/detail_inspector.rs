use crate::app::{App, ChatEntry, ChatRole, InspectorView, PendingConfirmation};
use crate::tool_grouping::{
    describe_tool_call, detect_groupable_tool_batch, tool_batch_summary_text, ToolBatch,
};
use crate::tool_output_summary::{parse_shell_output_sections, summarize_tool_result};
use crate::ui::inspector::{
    InspectorAction, InspectorDocument, InspectorPanel, InspectorState, InspectorTab,
};

pub(crate) const INSPECTOR_CONFIRM_ALLOW: &str = "__yode_confirm_allow__";
pub(crate) const INSPECTOR_CONFIRM_ALWAYS: &str = "__yode_confirm_always__";
pub(crate) const INSPECTOR_CONFIRM_DENY: &str = "__yode_confirm_deny__";

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

fn build_pending_confirmation_document(app: &App, confirm: &PendingConfirmation) -> InspectorDocument {
    let args = parse_json(&confirm.arguments);
    let tool_label = tool_display_name(app, &confirm.name);
    let activity = describe_tool_call(&confirm.name, &args, true)
        .or_else(|| app.tools.get(&confirm.name).map(|tool| tool.activity_description(&args)))
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
            actions: vec![
                InspectorAction {
                    label: "allow".to_string(),
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
            ],
        },
        PanelSpec {
            label: "Arguments".to_string(),
            lines: if arguments.is_empty() {
                vec!["(no arguments)".to_string()]
            } else {
                arguments
            },
            actions: vec![
                InspectorAction {
                    label: "allow".to_string(),
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
            ],
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
    for index in (0..entries.len()).rev() {
        if let Some(batch) = detect_groupable_tool_batch(entries, index) {
            if batch.next_index > index {
                return Some(build_tool_batch_document(app, entries, &batch));
            }
        }

        match &entries[index].role {
            ChatRole::ToolCall { id, name } => {
                let result = entries[index + 1..]
                    .iter()
                    .find(|entry| matches!(&entry.role, ChatRole::ToolResult { id: rid, .. } if rid == id));
                return Some(build_tool_entry_document(app, name, &entries[index].content, result));
            }
            ChatRole::ToolResult { id, .. } => {
                let has_preceding = index > 0
                    && entries[..index]
                        .iter()
                        .rev()
                        .any(|entry| matches!(&entry.role, ChatRole::ToolCall { id: tid, .. } if tid == id));
                if !has_preceding {
                    return Some(build_standalone_result_document(app, &entries[index]));
                }
            }
            _ => {}
        }
    }
    None
}

fn build_tool_batch_document(_app: &App, entries: &[ChatEntry], batch: &ToolBatch) -> InspectorDocument {
    let mut panels = vec![PanelSpec {
        label: "Overview".to_string(),
        lines: vec![
            format!("Summary: {}", tool_batch_summary_text(batch)),
            format!("Items: {}", batch.items.len()),
            format!(
                "State: {}",
                if batch.is_active { "active" } else { "completed" }
            ),
        ],
        actions: vec![
            InspectorAction {
                label: "status".to_string(),
                command: "/status".to_string(),
            },
            InspectorAction {
                label: "tools".to_string(),
                command: "/tools".to_string(),
            },
            InspectorAction {
                label: "summarize".to_string(),
                command: "Summarize the most important outcome from the recent tool activity and suggest the next best step.".to_string(),
            },
        ],
    }];

    for (item_index, item) in batch.items.iter().enumerate() {
        let call = &entries[item.call_index];
        let args = parse_json(&call.content);
        let result_entry = item.result_index.and_then(|idx| entries.get(idx));
        let summary = summarize_tool_result(
            &item.tool_name,
            &args,
            result_entry.and_then(|entry| entry.tool_metadata.as_ref()),
            result_entry.map(|entry| entry.content.as_str()).unwrap_or(""),
            result_entry.is_some_and(|entry| matches!(entry.role, ChatRole::ToolResult { is_error: true, .. })),
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

fn build_tool_entry_document(
    app: &App,
    tool_name: &str,
    args_json: &str,
    result_entry: Option<&ChatEntry>,
) -> InspectorDocument {
    let args = parse_json(args_json);
    let is_active = result_entry.is_none();
    let title = tool_display_name(app, tool_name);
    let activity = describe_tool_call(tool_name, &args, is_active)
        .or_else(|| app.tools.get(tool_name).map(|tool| tool.activity_description(&args)))
        .unwrap_or_else(|| tool_name.to_string());

    let mut summary = vec![
        format!("Tool: {}", title),
        format!("Activity: {}", activity),
        format!("State: {}", if is_active { "running" } else { "completed" }),
    ];
    if let Some(result_entry) = result_entry {
        if let Some(duration) = result_entry.duration {
            summary.push(format!("Duration: {}", crate::app::format_duration(duration)));
        }
        if let Some(error_type) = result_entry.tool_error_type.as_deref() {
            summary.push(format!("Error type: {}", error_type));
        }
    }

    let actions = tool_followup_actions(&title, tool_name, &args, result_entry);
    let mut panels = vec![
        PanelSpec {
            label: "Summary".to_string(),
            lines: summary,
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
            actions: vec![InspectorAction {
                label: "reuse".to_string(),
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
                    actions: actions.clone(),
                });
            }
        }
        let output_lines = render_result_content_lines(&result_entry.content, tool_name);
        if !output_lines.is_empty() {
            panels.push(PanelSpec {
                label: "Output".to_string(),
                lines: output_lines,
                actions: actions.clone(),
            });
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
                actions: Vec::new(),
            }],
            None,
        );
    };
    let title = tool_display_name(app, name);
    let mut summary = vec![format!("Tool: {}", title)];
    if let Some(error_type) = entry.tool_error_type.as_deref() {
        summary.push(format!("Error type: {}", error_type));
    }
    if let Some(duration) = entry.duration {
        summary.push(format!("Duration: {}", crate::app::format_duration(duration)));
    }

    let actions = vec![
        InspectorAction {
            label: "status".to_string(),
            command: "/status".to_string(),
        },
        InspectorAction {
            label: "tools".to_string(),
            command: "/tools".to_string(),
        },
        InspectorAction {
            label: "analyze".to_string(),
            command: format!(
                "Explain the most important details from the last {} result and suggest the next step.",
                title
            ),
        },
    ];
    let mut panels = vec![PanelSpec {
        label: "Summary".to_string(),
        lines: summary,
        actions: actions.clone(),
    }];
    if let Some(metadata) = entry.tool_metadata.as_ref() {
        let lines = json_to_lines(metadata);
        if !lines.is_empty() {
            panels.push(PanelSpec {
                label: "Metadata".to_string(),
                lines,
                actions: actions.clone(),
            });
        }
    }
    let output_lines = render_result_content_lines(&entry.content, name);
    if !output_lines.is_empty() {
        panels.push(PanelSpec {
            label: "Output".to_string(),
            lines: output_lines,
            actions: actions.clone(),
        });
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
            badges: Vec::new(),
            actions: panel.actions,
        })
        .collect::<Vec<_>>();

    InspectorDocument {
        state: InspectorState::new(title, tabs),
        panels: inspector_panels,
        footer,
    }
}

fn tool_followup_actions(
    title: &str,
    tool_name: &str,
    args: &serde_json::Value,
    result_entry: Option<&ChatEntry>,
) -> Vec<InspectorAction> {
    let mut actions = vec![
        InspectorAction {
            label: "status".to_string(),
            command: "/status".to_string(),
        },
        InspectorAction {
            label: "tools".to_string(),
            command: "/tools".to_string(),
        },
    ];

    if let Some(prompt) = primary_followup_prompt(title, tool_name, args, result_entry) {
        actions.push(InspectorAction {
            label: "follow-up".to_string(),
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
    if result_entry.is_some_and(|entry| matches!(entry.role, ChatRole::ToolResult { is_error: true, .. })) {
        return Some(format!(
            "Explain why the last {} step failed and suggest the safest next action.",
            title
        ));
    }

    if let Some(file_path) = args
        .get("file_path")
        .or_else(|| args.get("path"))
        .and_then(|value| value.as_str())
    {
        return Some(format!(
            "Inspect {} and summarize the most relevant details from the last {} step.",
            file_path, tool_name
        ));
    }

    if let Some(url) = args.get("url").and_then(|value| value.as_str()) {
        return Some(format!(
            "Summarize the most important findings related to {} from the last {} step.",
            url, tool_name
        ));
    }

    if let Some(query) = args.get("query").and_then(|value| value.as_str()) {
        return Some(format!(
            "Continue from the last web search for '{}' and summarize the highest-signal findings.",
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
        "Summarize the most important outcome from the last {} step and suggest the next best action.",
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
        content.lines().map(|line| line.to_string()).collect()
    }
}

fn tool_display_name(app: &App, tool_name: &str) -> String {
    if let Some(tool) = app.tools.get(tool_name) {
        let label = tool.user_facing_name();
        if !label.trim().is_empty() {
            return label.to_string();
        }
    }
    tool_name
        .split('_')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut chars = segment.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
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
    use yode_tools::builtin::{register_builtin_tools, register_skill_tool};
    use yode_tools::builtin::skill::SkillStore;
    use yode_tools::registry::ToolRegistry;

    use crate::app::{App, ChatEntry, ChatRole, PendingConfirmation};

    use super::{
        build_latest_tool_document, build_pending_confirmation_document,
        INSPECTOR_CONFIRM_ALLOW,
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
        assert!(doc.panels[0].lines.iter().any(|line| line.contains("Reading .../src/main.rs")));
        assert!(doc.panels[1].lines.iter().any(|line| line.contains("/tmp/src/main.rs")));
        assert!(doc.panels[0].actions.iter().any(|action| action.command == INSPECTOR_CONFIRM_ALLOW));
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
        assert!(doc.panels.iter().any(|panel| panel.tab.label == "Arguments"));
        assert!(doc.panels.iter().any(|panel| panel.tab.label == "Output"));
        assert!(doc.panels[0].actions.iter().any(|action| action.command == "/status"));
        assert!(doc.panels[0].actions.iter().any(|action| action.label == "follow-up"));
    }
}
