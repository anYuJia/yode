use crate::commands::context::CommandContext;
use crate::commands::info::permission_recovery_workspace::{
    render_permission_workspace, render_recovery_workspace,
};
use crate::commands::workspace_nav::{runtime_operator_jump_targets, workspace_jump_inventory};
use crate::commands::workspace_text::{workspace_bullets, WorkspaceText};
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use yode_core::permission::{
    tool_categories, PermissionConflictView, PermissionMode, PermissionRule, PermissionSourceView,
    RuleSource,
};

pub struct PermissionsCommand {
    meta: CommandMeta,
}

impl PermissionsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "permissions",
                description: "View or modify tool execution permissions and permission mode",
                aliases: &["perms"],
                args: vec![
                    ArgDef {
                        name: "subcommand".into(),
                        required: false,
                        hint: "<mode|tool-name|reset>".into(),
                        completions: ArgCompletionSource::Dynamic(|ctx| {
                            let mut names: Vec<String> = vec![
                                "mode".into(),
                                "reset".into(),
                                "explain".into(),
                                "denials".into(),
                                "governance".into(),
                                "scopes".into(),
                                "sources".into(),
                                "category".into(),
                            ];
                            names.extend(ctx.tools.definitions().iter().map(|d| d.name.clone()));
                            names.sort();
                            names
                        }),
                    },
                    ArgDef {
                        name: "action".into(),
                        required: false,
                        hint: "<allow|deny|ask|default|plan|auto|accept-edits|bypass>".into(),
                        completions: ArgCompletionSource::Static(vec![
                            "guide".into(),
                            "allow".into(),
                            "deny".into(),
                            "ask".into(),
                            "default".into(),
                            "plan".into(),
                            "auto".into(),
                            "accept-edits".into(),
                            "bypass".into(),
                        ]),
                    },
                ],
                category: CommandCategory::Tools,
                hidden: false,
            },
        }
    }
}
impl Command for PermissionsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let parts: Vec<&str> = args.split_whitespace().collect();

        let Ok(mut engine) = ctx.engine.try_lock() else {
            return Err("Engine is busy, try again.".into());
        };

        match parts.as_slice() {
            // No args: show current permissions and mode
            [] => {
                let mode = engine.permissions().mode();
                let tools = engine.permissions().confirmable_tools();
                let rules = engine.permissions().rules_snapshot();
                let source_views = engine.permissions().source_views_snapshot();
                let denials = engine.permissions().recent_denials(5);
                let denial_prefixes = engine.permissions().recent_denial_prefixes(5);
                let safe_prefixes = engine.permissions().safe_readonly_shell_prefixes();
                let confirmation_suggestions = engine.permissions().confirmation_rule_suggestions(3);
                let runtime = engine.runtime_state();
                let governance_artifact = write_permission_governance_artifact(
                    std::path::Path::new(&ctx.session.working_dir),
                    &ctx.session.session_id,
                    engine.permissions(),
                    None,
                );
                let denial_prefix_lines = denial_prefixes
                    .iter()
                    .map(|denial| {
                        format!(
                            "{} -> count={} consecutive={} last_at={}",
                            denial.prefix, denial.count, denial.consecutive, denial.last_at
                        )
                    })
                    .collect::<Vec<_>>();
                let denial_lines = if denials.is_empty() {
                    vec!["none".to_string()]
                } else {
                    denials
                        .into_iter()
                        .map(|denial| {
                            format!(
                                "{} x{} (consecutive {}, at {})",
                                denial.tool_name, denial.count, denial.consecutive, denial.last_at
                            )
                        })
                        .collect()
                };
                Ok(CommandOutput::Message(format!(
                    "{}\n\n{}",
                    render_permission_workspace(
                        mode,
                        &tools,
                        &rules,
                        &source_views,
                        &denial_lines,
                        &denial_prefix_lines,
                        &safe_prefixes.join(", "),
                        &confirmation_suggestions,
                        governance_artifact.as_deref(),
                        &runtime,
                    ),
                    render_recovery_workspace(&runtime),
                )))
            }
            // /permissions mode — show current mode
            ["mode"] | ["mode", "guide"] => {
                let mode = engine.permissions().mode();
                Ok(CommandOutput::Message(render_permission_mode_guide(mode)))
            }
            // /permissions mode <mode-name>
            ["mode", mode_str] => match mode_str.parse::<PermissionMode>() {
                Ok(mode) => {
                    engine.permissions_mut().set_mode(mode);
                    Ok(CommandOutput::Message(render_permission_mode_switch(mode)))
                }
                Err(e) => Err(e),
            },
            // Reset
            ["reset"] => {
                engine.permissions_mut().reset(vec![
                    "bash".into(),
                    "write_file".into(),
                    "edit_file".into(),
                ]);
                Ok(CommandOutput::Message(
                    "Permissions reset to defaults.".into(),
                ))
            }
            ["scopes"] | ["sources"] => {
                let views = engine.permissions().source_views_snapshot();
                let conflicts = engine.permissions().conflict_views_snapshot();
                Ok(CommandOutput::Messages(render_permission_sources_lines(
                    &views, &conflicts,
                )))
            }
            ["governance"] => {
                let path = write_permission_governance_artifact(
                    std::path::Path::new(&ctx.session.working_dir),
                    &ctx.session.session_id,
                    engine.permissions(),
                    None,
                )
                .ok_or_else(|| "Failed to write permission governance artifact.".to_string())?;
                Ok(CommandOutput::Message(format!(
                    "Permission governance artifact written: {}",
                    path
                )))
            }
            // /permissions denials [tool]
            ["denials"] | ["denials", _] => {
                let filter = parts.get(1).copied();
                let denials = engine.permissions().recent_denials(20);
                let filtered = denials
                    .into_iter()
                    .filter(|denial| {
                        filter
                            .map(|tool| denial.tool_name == tool)
                            .unwrap_or(true)
                    })
                    .collect::<Vec<_>>();
                if filtered.is_empty() {
                    let prefix_lines = engine.permissions().recent_denial_prefixes(10);
                    if prefix_lines.is_empty() {
                        return Ok(CommandOutput::Message(
                            "Recent denials: none".to_string(),
                        ));
                    }
                    let mut lines =
                        vec!["Recent bash denials grouped by command prefix:".to_string()];
                    for denial in prefix_lines {
                        lines.push(format!(
                            "  {} -> count={} consecutive={} last_at={}",
                            denial.prefix, denial.count, denial.consecutive, denial.last_at
                        ));
                    }
                    return Ok(CommandOutput::Messages(lines));
                }
                let mut lines = vec!["Recent denials grouped by tool:".to_string()];
                for denial in filtered {
                    lines.push(format!(
                        "  {} -> count={} consecutive={} last_at={}",
                        denial.tool_name, denial.count, denial.consecutive, denial.last_at
                    ));
                }
                Ok(CommandOutput::Messages(lines))
            }
            // /permissions explain <tool> [content]
            ["explain", tool, content @ ..] => {
                let content = (!content.is_empty()).then(|| content.join(" "));
                let explanation = engine
                    .permissions()
                    .explain_with_content(tool, content.as_deref());
                let artifact = write_permission_governance_artifact(
                    std::path::Path::new(&ctx.session.working_dir),
                    &ctx.session.session_id,
                    engine.permissions(),
                    Some((tool, content.as_deref())),
                );
                Ok(CommandOutput::Message(format!(
                    "Permission explanation for '{}':\n  Action:      {}\n  Mode:        {}\n  Reason:      {}\n  Matched rule: {}\n  Risk:        {}\n  Semantic:    {}{}\n  Categories:  {}\n  Denials:     {}{}\n  Precedence:  {}\n  Artifact:    {}\n",
                    tool,
                    explanation.action.label(),
                    explanation.mode,
                    explanation.reason,
                    explanation.matched_rule.as_deref().unwrap_or("none"),
                    explanation
                        .classifier_risk
                        .map(|risk| format!("{:?}", risk))
                        .unwrap_or_else(|| "none".to_string()),
                    explanation
                        .semantic_category
                        .map(|category| category.label().to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    explanation
                        .semantic_segment
                        .as_deref()
                        .map(|segment| format!(" / segment `{segment}`"))
                        .unwrap_or_default(),
                    tool_categories(tool).join(", "),
                    explanation.denial_count,
                    if explanation.auto_skip_due_to_denials {
                        " (auto-skip active)"
                    } else {
                        ""
                    },
                    if explanation.precedence_chain.is_empty() {
                        "none".to_string()
                    } else {
                        explanation.precedence_chain.join(" | ")
                    },
                    artifact.as_deref().unwrap_or("none")
                )))
            }
            // /permissions <tool> explain [content]
            [tool, "explain", content @ ..] => {
                let content = (!content.is_empty()).then(|| content.join(" "));
                let explanation = engine
                    .permissions()
                    .explain_with_content(tool, content.as_deref());
                let artifact = write_permission_governance_artifact(
                    std::path::Path::new(&ctx.session.working_dir),
                    &ctx.session.session_id,
                    engine.permissions(),
                    Some((tool, content.as_deref())),
                );
                Ok(CommandOutput::Message(format!(
                    "Permission explanation for '{}':\n  Action:      {}\n  Mode:        {}\n  Reason:      {}\n  Matched rule: {}\n  Risk:        {}\n  Semantic:    {}{}\n  Categories:  {}\n  Denials:     {}{}\n  Precedence:  {}\n  Artifact:    {}\n",
                    tool,
                    explanation.action.label(),
                    explanation.mode,
                    explanation.reason,
                    explanation.matched_rule.as_deref().unwrap_or("none"),
                    explanation
                        .classifier_risk
                        .map(|risk| format!("{:?}", risk))
                        .unwrap_or_else(|| "none".to_string()),
                    explanation
                        .semantic_category
                        .map(|category| category.label().to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    explanation
                        .semantic_segment
                        .as_deref()
                        .map(|segment| format!(" / segment `{segment}`"))
                        .unwrap_or_default(),
                    tool_categories(tool).join(", "),
                    explanation.denial_count,
                    if explanation.auto_skip_due_to_denials {
                        " (auto-skip active)"
                    } else {
                        ""
                    },
                    if explanation.precedence_chain.is_empty() {
                        "none".to_string()
                    } else {
                        explanation.precedence_chain.join(" | ")
                    },
                    artifact.as_deref().unwrap_or("none")
                )))
            }
            // /permissions category <name> allow|deny|ask
            ["category", category, "allow"] => {
                engine.permissions_mut().allow_category(category);
                Ok(CommandOutput::Message(format!(
                    "Permission category '{}' set to allow.",
                    category
                )))
            }
            ["category", category, "deny"] => {
                engine.permissions_mut().deny_category(category);
                Ok(CommandOutput::Message(format!(
                    "Permission category '{}' set to deny.",
                    category
                )))
            }
            ["category", category, "ask"] => {
                engine.permissions_mut().ask_category(category);
                Ok(CommandOutput::Message(format!(
                    "Permission category '{}' set to ask.",
                    category
                )))
            }
            // /permissions <tool> allow
            [tool, "allow"] => {
                engine.permissions_mut().allow(tool);
                Ok(CommandOutput::Message(format!(
                    "Tool '{tool}' set to auto-allow."
                )))
            }
            // /permissions <tool> deny
            [tool, "deny"] => {
                engine.permissions_mut().deny(tool);
                Ok(CommandOutput::Message(format!(
                    "Tool '{tool}' set to deny."
                )))
            }
            _ => Err("Usage: /permissions [mode <mode>] | [governance] | [sources] | [scopes] | [denials [tool]] | [explain <tool> [content]] | [tool allow|deny|explain] | [category <name> allow|deny|ask] | [reset]".into()),
        }
    }
}

fn render_permission_mode_guide(current: PermissionMode) -> String {
    WorkspaceText::new("Permission mode operator guide")
        .field("Current", current.to_string())
        .field(
            "Keyboard cycle",
            "TUI badge cycle covers default -> auto -> plan".to_string(),
        )
        .field("Slash-only modes", "accept-edits / bypass".to_string())
        .section(
            "Modes",
            workspace_bullets([
                "default: risky tools ask, ordinary read flow stays interactive",
                "plan: blocks mutations and keeps the session in inspection/planning mode",
                "auto: classifier auto-allows low-risk actions and falls back to ask on risk",
                "accept-edits: auto-allows file edits but still keeps shell escalation tighter",
                "bypass: skips permission prompts entirely; use only for trusted, short-lived runs",
            ]),
        )
        .section(
            "Recommended use",
            workspace_bullets([
                "default for daily coding with normal guardrails",
                "plan when you want analysis-only or review-only sessions",
                "auto when you want faster read-heavy execution with safety fallback",
                "accept-edits for concentrated refactor/edit loops where file writes dominate",
                "bypass only inside tightly controlled local workflows",
            ]),
        )
        .section(
            "Examples",
            workspace_bullets([
                "/permissions mode default",
                "/permissions mode auto",
                "/permissions mode plan",
                "/permissions mode accept-edits",
                "/permissions mode bypass",
            ]),
        )
        .footer(workspace_jump_inventory(runtime_operator_jump_targets(
            None,
        )))
        .render()
}

fn render_permission_mode_switch(mode: PermissionMode) -> String {
    let operator_note = match mode {
        PermissionMode::Default => {
            "Guardrails restored for risky tools; keep using Shift+Tab if you only need the TUI cycle."
        }
        PermissionMode::Plan => {
            "Mutation tools now stay blocked; use this for inspection, review, and planning passes."
        }
        PermissionMode::Auto => {
            "Classifier-driven auto-allow is active; inspect `/permissions governance` if a tool still falls back to ask."
        }
        PermissionMode::AcceptEdits => {
            "Write-heavy flows can run faster now; shell-sensitive work may still require confirmation."
        }
        PermissionMode::Bypass => {
            "All permission prompts are bypassed; keep the session short and verify outputs aggressively."
        }
    };

    format!(
        "Permission mode set to: {}\n{}\nNext: /permissions mode | /permissions governance | /permissions explain bash",
        mode, operator_note
    )
}

fn render_permission_sources_lines(
    views: &[PermissionSourceView],
    conflicts: &[PermissionConflictView],
) -> Vec<String> {
    let mut lines = vec![
        "Permission sources:".to_string(),
        "  precedence: cli > managed > session > local > project > user".to_string(),
    ];
    if views.is_empty() {
        lines.push("  scopes: none".to_string());
    } else {
        let mut sorted = views.to_vec();
        sorted.sort_by_key(|view| std::cmp::Reverse(view.source));
        for view in sorted {
            lines.push(format!(
                "  {} path={} mode={} rules={}",
                rule_source_name(view.source),
                view.path.as_deref().unwrap_or("none"),
                view.default_mode.as_deref().unwrap_or("inherit"),
                view.rules.len(),
            ));
        }
    }

    if conflicts.is_empty() {
        lines.push("  conflicts: none".to_string());
    } else {
        lines.push("  conflicts:".to_string());
        for conflict in conflicts {
            lines.push(format!(
                "    {} {} overrides {} {} for tool={}{}{}",
                rule_source_name(conflict.higher_source),
                conflict.higher_behavior.label(),
                rule_source_name(conflict.lower_source),
                conflict.lower_behavior.label(),
                conflict.tool_name,
                conflict
                    .category
                    .as_deref()
                    .map(|category| format!(" category={category}"))
                    .unwrap_or_default(),
                conflict
                    .pattern
                    .as_deref()
                    .map(|pattern| format!(" pattern={pattern}"))
                    .unwrap_or_default(),
            ));
        }
    }
    lines
}

fn rule_source_name(source: RuleSource) -> &'static str {
    match source {
        RuleSource::ManagedConfig => "managed",
        RuleSource::UserConfig => "user",
        RuleSource::ProjectConfig => "project",
        RuleSource::LocalConfig => "local",
        RuleSource::Session => "session",
        RuleSource::CliArg => "cli",
    }
}

fn write_permission_governance_artifact(
    project_root: &std::path::Path,
    session_id: &str,
    permissions: &yode_core::permission::PermissionManager,
    explanation_target: Option<(&str, Option<&str>)>,
) -> Option<String> {
    let dir = project_root.join(".yode").join("hooks");
    std::fs::create_dir_all(&dir).ok()?;
    let short_session = session_id.chars().take(8).collect::<String>();
    let path = dir.join(format!("{}-permission-governance.json", short_session));
    let target_payload = explanation_target.map(|(tool, content)| {
        let explanation = permissions.explain_with_content(tool, content);
        serde_json::json!({
            "tool": tool,
            "content": content,
            "action": explanation.action.label(),
            "mode": explanation.mode.to_string(),
            "reason": explanation.reason,
            "matched_rule": explanation.matched_rule,
            "risk": explanation.classifier_risk.map(|risk| format!("{:?}", risk)),
            "semantic_category": explanation.semantic_category.map(|category| category.label()),
            "semantic_segment": explanation.semantic_segment,
            "categories": tool_categories(tool),
            "precedence_chain": explanation.precedence_chain,
        })
    });
    let payload = serde_json::json!({
        "updated_at": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        "mode": permissions.mode().to_string(),
        "confirmable_tools": permissions.confirmable_tools(),
        "safe_readonly_shell_prefixes": permissions.safe_readonly_shell_prefixes(),
        "source_views": permissions.source_views_snapshot().into_iter().map(|view| {
            serde_json::json!({
                "source": format!("{:?}", view.source),
                "path": view.path,
                "default_mode": view.default_mode,
                "rules": view.rules.into_iter().map(|rule| serialize_permission_rule(&rule)).collect::<Vec<_>>(),
            })
        }).collect::<Vec<_>>(),
        "conflicts": permissions.conflict_views_snapshot().into_iter().map(|conflict| {
            serde_json::json!({
                "higher_source": format!("{:?}", conflict.higher_source),
                "lower_source": format!("{:?}", conflict.lower_source),
                "tool": conflict.tool_name,
                "category": conflict.category,
                "pattern": conflict.pattern,
                "higher_behavior": conflict.higher_behavior.label(),
                "lower_behavior": conflict.lower_behavior.label(),
            })
        }).collect::<Vec<_>>(),
        "rules": permissions.rules_snapshot().into_iter().map(|rule| serialize_permission_rule(&rule)).collect::<Vec<_>>(),
        "explanation_target": target_payload,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&payload).ok()?).ok()?;
    Some(path.display().to_string())
}

fn serialize_permission_rule(rule: &PermissionRule) -> serde_json::Value {
    serde_json::json!({
        "source": format!("{:?}", rule.source),
        "tool": rule.tool_name,
        "category": rule.category,
        "behavior": rule.behavior.label(),
        "pattern": rule.pattern,
        "description": rule.description,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        render_permission_mode_guide, render_permission_mode_switch,
        render_permission_sources_lines,
    };
    use yode_core::permission::{
        PermissionConflictView, PermissionMode, PermissionSourceView, RuleBehavior, RuleSource,
    };

    #[test]
    fn permission_mode_guide_mentions_keyboard_and_slash_only_modes() {
        let rendered = render_permission_mode_guide(PermissionMode::Default);
        assert!(rendered.contains("default -> auto -> plan"));
        assert!(rendered.contains("accept-edits / bypass"));
    }

    #[test]
    fn permission_mode_switch_mentions_next_steps() {
        let rendered = render_permission_mode_switch(PermissionMode::Bypass);
        assert!(rendered.contains("Permission mode set to: bypass"));
        assert!(rendered.contains("/permissions governance"));
    }

    #[test]
    fn permission_sources_lines_show_precedence_paths_and_conflicts() {
        let lines = render_permission_sources_lines(
            &[PermissionSourceView {
                source: RuleSource::ManagedConfig,
                path: Some("/tmp/managed.toml".to_string()),
                default_mode: Some("auto".to_string()),
                rules: vec![],
            }],
            &[PermissionConflictView {
                higher_source: RuleSource::ManagedConfig,
                lower_source: RuleSource::UserConfig,
                tool_name: "bash".to_string(),
                category: None,
                pattern: Some("git push *".to_string()),
                higher_behavior: RuleBehavior::Deny,
                lower_behavior: RuleBehavior::Allow,
            }],
        );

        assert!(lines.iter().any(|line| line.contains("cli > managed")));
        assert!(lines.iter().any(|line| line.contains("/tmp/managed.toml")));
        assert!(lines
            .iter()
            .any(|line| line.contains("managed deny overrides user allow")));
    }
}
