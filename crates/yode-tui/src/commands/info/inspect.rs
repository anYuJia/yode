use std::path::PathBuf;

use crate::commands::artifact_nav::{
    artifact_display_line, artifact_history_lines, latest_artifact_by_suffix,
    latest_bundle_workspace_index, latest_coordinator_artifact,
    latest_runtime_orchestration_artifact, latest_workflow_execution_artifact,
    open_artifact_inspector, recent_artifacts_by_suffix, recent_bundle_workspace_indexes,
    resolve_artifact_basename, stale_artifact_actions,
};
use crate::commands::context::CommandContext;
use crate::commands::inspector_bridge::document_from_command_output;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct InspectCommand {
    meta: CommandMeta,
}

impl InspectCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "inspect",
                description: "Open an inspector view for an existing command output",
                aliases: &[],
                args: vec![ArgDef {
                    name: "target".to_string(),
                    required: false,
                    hint: "[tasks|memory|reviews|status|diagnostics|doctor|hooks|permissions|workflows|coordinate|artifact]".to_string(),
                    completions: ArgCompletionSource::Dynamic(inspect_completion_targets),
                }],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for InspectCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let trimmed = args.trim();
        if let Some(target) = trimmed.strip_prefix("artifact") {
            return inspect_artifact_target(target.trim(), ctx);
        }
        let (command, command_args, title) = match trimmed {
            "" => ("status", "", "Status inspector".to_string()),
            value if value.starts_with("workflows") => (
                "workflows",
                value.strip_prefix("workflows").unwrap_or("").trim(),
                "Workflow inspector".to_string(),
            ),
            value if value.starts_with("coordinate") => (
                "coordinate",
                value.strip_prefix("coordinate").unwrap_or("").trim(),
                "Coordinator inspector".to_string(),
            ),
            value if value.starts_with("tasks") => (
                "tasks",
                value.strip_prefix("tasks").unwrap_or("").trim(),
                "Task inspector".to_string(),
            ),
            value if value.starts_with("memory") => (
                "memory",
                value.strip_prefix("memory").unwrap_or("").trim(),
                "Memory inspector".to_string(),
            ),
            value if value.starts_with("reviews") => (
                "reviews",
                value.strip_prefix("reviews").unwrap_or("").trim(),
                "Review inspector".to_string(),
            ),
            value if value.starts_with("doctor") => (
                "doctor",
                value.strip_prefix("doctor").unwrap_or("").trim(),
                "Doctor inspector".to_string(),
            ),
            "status" => ("status", "", "Status inspector".to_string()),
            "diagnostics" => ("diagnostics", "", "Diagnostics inspector".to_string()),
            "hooks" => ("hooks", "", "Hook inspector".to_string()),
            value if value.starts_with("permissions") => (
                "permissions",
                value.strip_prefix("permissions").unwrap_or("").trim(),
                "Permission inspector".to_string(),
            ),
            other => return Err(format!("Unknown inspect target '{}'.", other)),
        };

        let output = ctx
            .cmd_registry
            .execute_command(command, command_args, ctx)
            .ok_or_else(|| format!("Command '{}' not found.", command))??;

        match output {
            CommandOutput::Message(body) => Ok(CommandOutput::OpenInspector(
                document_from_command_output(&title, body.lines().map(str::to_string).collect()),
            )),
            CommandOutput::Messages(lines) => {
                Ok(CommandOutput::OpenInspector(document_from_command_output(&title, lines)))
            }
            CommandOutput::OpenInspector(doc) => Ok(CommandOutput::OpenInspector(doc)),
            CommandOutput::Silent => Err("Inspect target produced no output.".to_string()),
            CommandOutput::StartWizard(_) | CommandOutput::ReloadProvider { .. } => {
                Err("Inspect target is not viewable as an inspector.".to_string())
            }
        }
    }
}

fn inspect_artifact_target(args: &str, ctx: &mut CommandContext) -> CommandResult {
    let project_root = PathBuf::from(&ctx.session.working_dir);
    let status_dir = project_root.join(".yode").join("status");
    let remote_dir = project_root.join(".yode").join("remote");
    let startup_dir = project_root.join(".yode").join("startup");
    let review_dir = project_root.join(".yode").join("reviews");
    let transcript_dir = project_root.join(".yode").join("transcripts");
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let runtime = ctx.engine.try_lock().ok().map(|engine| engine.runtime_state());

    if args == "list" {
        let lines = artifact_inventory_lines(&project_root, &cwd);
        return Ok(CommandOutput::OpenInspector(document_from_command_output(
            "Artifact inventory",
            lines,
        )));
    }
    if args == "summary" {
        let lines = artifact_summary_lines(&project_root, &cwd);
        return Ok(CommandOutput::OpenInspector(document_from_command_output(
            "Artifact summary",
            lines,
        )));
    }
    if args == "history" || args.starts_with("history ") {
        let family = args.strip_prefix("history").unwrap_or("").trim();
        let family = if family.is_empty() { "status" } else { family };
        let lines = artifact_history_family_lines(family, &project_root, &cwd, runtime.as_ref())?;
        return Ok(CommandOutput::OpenInspector(document_from_command_output(
            &format!("Artifact history [{}]", family),
            lines,
        )));
    }

    let (path, title, kind, refresh) = match args {
        "" | "latest-orchestration" => (
            latest_runtime_orchestration_artifact(&project_root)
                .ok_or_else(|| "No orchestration timeline artifact found.".to_string())?,
            "Runtime orchestration timeline".to_string(),
            "orchestration".to_string(),
            vec![
                "/inspect workflows timeline".to_string(),
                "/coordinate timeline".to_string(),
            ],
        ),
        "latest-workflow" => (
            latest_workflow_execution_artifact(&project_root)
                .ok_or_else(|| "No workflow execution artifact found.".to_string())?,
            "Workflow execution inspector".to_string(),
            "workflow".to_string(),
            vec![
                "/inspect workflows latest".to_string(),
                "/workflows preview latest".to_string(),
                "/workflows run latest".to_string(),
            ],
        ),
        "latest-coordinate" => (
            latest_coordinator_artifact(&project_root)
                .ok_or_else(|| "No coordinator artifact found.".to_string())?,
            "Coordinator inspector".to_string(),
            "coordinate".to_string(),
            vec!["/coordinate latest".to_string(), "/coordinate timeline".to_string()],
        ),
        "latest-remote-capability" => (
            latest_artifact_by_suffix(&remote_dir, "remote-workflow-capability.json")
                .ok_or_else(|| "No remote capability artifact found.".to_string())?,
            "Remote capability artifact".to_string(),
            "remote".to_string(),
            vec!["/doctor remote-review".to_string()],
        ),
        "latest-remote-execution" => (
            latest_artifact_by_suffix(&remote_dir, "remote-execution-state.json")
                .ok_or_else(|| "No remote execution state artifact found.".to_string())?,
            "Remote execution artifact".to_string(),
            "remote".to_string(),
            vec!["/doctor remote-artifacts".to_string()],
        ),
        "bundle" | "latest-bundle" => (
            latest_bundle_workspace_index(&cwd)
                .ok_or_else(|| "No diagnostics workspace index found.".to_string())?,
            "Diagnostics bundle workspace index".to_string(),
            "bundle".to_string(),
            vec!["/export diagnostics".to_string()],
        ),
        "latest-runtime-timeline" => (
            latest_artifact_by_suffix(&status_dir, "runtime-timeline.md")
                .ok_or_else(|| "No runtime timeline artifact found.".to_string())?,
            "Runtime timeline artifact".to_string(),
            "runtime".to_string(),
            vec!["/doctor bundle".to_string(), "/inspect artifact history runtime".to_string()],
        ),
        "latest-runtime-tasks" => (
            latest_artifact_by_suffix(&status_dir, "runtime-tasks.md")
                .ok_or_else(|| "No runtime task artifact found.".to_string())?,
            "Runtime task inventory artifact".to_string(),
            "runtime".to_string(),
            vec!["/tasks latest".to_string()],
        ),
        "latest-hook-failures" => (
            latest_artifact_by_suffix(&status_dir, "hook-failures.md")
                .ok_or_else(|| "No hook failure artifact found.".to_string())?,
            "Hook failure artifact".to_string(),
            "hook".to_string(),
            vec!["/hooks".to_string()],
        ),
        "latest-startup-profile" => (
            latest_artifact_by_suffix(&startup_dir, "startup-profile.txt")
                .ok_or_else(|| "No startup profile artifact found.".to_string())?,
            "Startup profile artifact".to_string(),
            "startup".to_string(),
            vec!["/status".to_string()],
        ),
        "latest-startup-manifest" => (
            latest_artifact_by_suffix(&startup_dir, "startup-bundle-manifest.json")
                .ok_or_else(|| "No startup manifest artifact found.".to_string())?,
            "Startup manifest artifact".to_string(),
            "startup".to_string(),
            vec!["/status".to_string()],
        ),
        "latest-provider-inventory" => (
            latest_artifact_by_suffix(&startup_dir, "provider-inventory.json")
                .ok_or_else(|| "No provider inventory artifact found.".to_string())?,
            "Provider inventory artifact".to_string(),
            "startup".to_string(),
            vec!["/status".to_string()],
        ),
        "latest-mcp-failures" => (
            latest_artifact_by_suffix(&startup_dir, "mcp-startup-failures.json")
                .ok_or_else(|| "No MCP failure artifact found.".to_string())?,
            "MCP startup failures artifact".to_string(),
            "startup".to_string(),
            vec!["/status".to_string()],
        ),
        "latest-review" => (
            recent_artifacts_by_suffix(&review_dir, ".md", 1)
                .into_iter()
                .next()
                .ok_or_else(|| "No review artifact found.".to_string())?,
            "Latest review artifact".to_string(),
            "review".to_string(),
            vec!["/reviews latest".to_string()],
        ),
        "latest-transcript" => (
            recent_artifacts_by_suffix(&transcript_dir, ".md", 1)
                .into_iter()
                .next()
                .ok_or_else(|| "No transcript artifact found.".to_string())?,
            "Latest transcript artifact".to_string(),
            "transcript".to_string(),
            vec!["/memory latest".to_string()],
        ),
        "latest-session-memory" => {
            let runtime_path = runtime
                .as_ref()
                .and_then(|state| {
                    state
                        .last_compaction_session_memory_path
                        .clone()
                        .or_else(|| state.last_session_memory_update_path.clone())
                })
                .map(PathBuf::from);
            let session_path = yode_core::session_memory::session_memory_path(&project_root);
            let path = runtime_path
                .filter(|path| path.exists())
                .or_else(|| session_path.exists().then_some(session_path))
                .ok_or_else(|| "No session memory artifact found.".to_string())?;
            (
                path,
                "Session memory artifact".to_string(),
                "memory".to_string(),
                vec!["/memory latest".to_string()],
            )
        }
        "latest-tool" => (
            runtime
                .as_ref()
                .and_then(|state| state.last_tool_turn_artifact_path.as_ref().map(PathBuf::from))
                .filter(|path| path.exists())
                .ok_or_else(|| "No tool artifact found.".to_string())?,
            "Tool artifact".to_string(),
            "runtime".to_string(),
            vec!["/tools".to_string(), "/brief".to_string()],
        ),
        "latest-recovery" => (
            runtime
                .as_ref()
                .and_then(|state| state.last_recovery_artifact_path.as_ref().map(PathBuf::from))
                .filter(|path| path.exists())
                .ok_or_else(|| "No recovery artifact found.".to_string())?,
            "Recovery artifact".to_string(),
            "recovery".to_string(),
            vec!["/hooks".to_string(), "/brief".to_string()],
        ),
        "latest-permission" => (
            runtime
                .as_ref()
                .and_then(|state| state.last_permission_artifact_path.as_ref().map(PathBuf::from))
                .filter(|path| path.exists())
                .ok_or_else(|| "No permission artifact found.".to_string())?,
            "Permission artifact".to_string(),
            "permission".to_string(),
            vec!["/permissions".to_string()],
        ),
        other => {
            let path = PathBuf::from(other);
            if path.exists() {
                (
                    path,
                    "Artifact inspector".to_string(),
                    "artifact".to_string(),
                    Vec::new(),
                )
            } else if let Some(path) = resolve_artifact_basename(&project_root, other) {
                (
                    path,
                    "Artifact inspector".to_string(),
                    "artifact".to_string(),
                    Vec::new(),
                )
            } else {
                return Err(format!("Artifact path not found: {}", other));
            }
        }
    };

    let mut footer_lines = Vec::new();
    if !refresh.is_empty() {
        footer_lines.push(refresh.join(" | "));
    }
    if let Some(stale) = stale_artifact_actions(&path, &refresh) {
        footer_lines.push(stale);
    }
    let doc = open_artifact_inspector(
        &title,
        &path,
        (!footer_lines.is_empty()).then(|| footer_lines.join("\n")),
        vec![("kind".into(), kind)],
    )
    .ok_or_else(|| format!("Failed to open artifact {}.", path.display()))?;
    Ok(CommandOutput::OpenInspector(doc))
}

fn inspect_completion_targets(ctx: &crate::commands::context::CompletionContext) -> Vec<String> {
    let project_root = PathBuf::from(ctx.working_dir);
    let mut values = vec![
        "status".to_string(),
        "diagnostics".to_string(),
        "doctor".to_string(),
        "hooks".to_string(),
        "permissions".to_string(),
        "tasks".to_string(),
        "memory".to_string(),
        "reviews".to_string(),
        "workflows".to_string(),
        "coordinate".to_string(),
        "artifact list".to_string(),
        "artifact summary".to_string(),
        "artifact history".to_string(),
        "artifact history status".to_string(),
        "artifact history remote".to_string(),
        "artifact history startup".to_string(),
        "artifact history reviews".to_string(),
        "artifact history transcripts".to_string(),
        "artifact history bundles".to_string(),
        "artifact history workflow".to_string(),
        "artifact history coordinate".to_string(),
        "artifact history runtime".to_string(),
        "artifact latest-workflow".to_string(),
        "artifact latest-coordinate".to_string(),
        "artifact latest-orchestration".to_string(),
        "artifact latest-runtime-timeline".to_string(),
        "artifact latest-runtime-tasks".to_string(),
        "artifact latest-hook-failures".to_string(),
        "artifact latest-startup-profile".to_string(),
        "artifact latest-startup-manifest".to_string(),
        "artifact latest-provider-inventory".to_string(),
        "artifact latest-mcp-failures".to_string(),
        "artifact latest-review".to_string(),
        "artifact latest-transcript".to_string(),
        "artifact latest-session-memory".to_string(),
        "artifact latest-tool".to_string(),
        "artifact latest-recovery".to_string(),
        "artifact latest-permission".to_string(),
        "artifact latest-remote-capability".to_string(),
        "artifact latest-remote-execution".to_string(),
        "artifact bundle".to_string(),
    ];
    let status_dir = project_root.join(".yode").join("status");
    let remote_dir = project_root.join(".yode").join("remote");
    let startup_dir = project_root.join(".yode").join("startup");
    let review_dir = project_root.join(".yode").join("reviews");
    let transcript_dir = project_root.join(".yode").join("transcripts");
    for path in recent_artifacts_by_suffix(&status_dir, ".md", 6)
        .into_iter()
        .chain(recent_artifacts_by_suffix(&remote_dir, ".json", 4))
        .chain(recent_artifacts_by_suffix(&startup_dir, ".json", 4))
        .chain(recent_artifacts_by_suffix(&startup_dir, ".txt", 2))
        .chain(recent_artifacts_by_suffix(&review_dir, ".md", 3))
        .chain(recent_artifacts_by_suffix(&transcript_dir, ".md", 3))
    {
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            values.push(format!("artifact {}", name));
        }
    }
    values
}

fn artifact_inventory_lines(project_root: &std::path::Path, cwd: &std::path::Path) -> Vec<String> {
    let mut lines = vec![
        "Aliases:".to_string(),
        "latest-workflow | latest-coordinate | latest-orchestration".to_string(),
        "latest-runtime-timeline | latest-runtime-tasks | latest-hook-failures".to_string(),
        "latest-startup-profile | latest-startup-manifest | latest-provider-inventory | latest-mcp-failures".to_string(),
        "latest-review | latest-transcript | latest-session-memory | latest-tool | latest-recovery | latest-permission".to_string(),
        "latest-remote-capability | latest-remote-execution | bundle".to_string(),
        "Recent status artifacts:".to_string(),
    ];
    lines.extend(artifact_history_lines(recent_artifacts_by_suffix(
        &project_root.join(".yode").join("status"),
        ".md",
        8,
    )));
    lines.push("Recent startup artifacts:".to_string());
    lines.extend(artifact_history_lines(
        recent_artifacts_by_suffix(&project_root.join(".yode").join("startup"), ".json", 6)
            .into_iter()
            .chain(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("startup"),
                ".txt",
                2,
            ))
            .collect::<Vec<_>>(),
    ));
    lines.push("Recent review artifacts:".to_string());
    lines.extend(artifact_history_lines(recent_artifacts_by_suffix(
        &project_root.join(".yode").join("reviews"),
        ".md",
        4,
    )));
    lines.push("Recent transcript artifacts:".to_string());
    lines.extend(artifact_history_lines(recent_artifacts_by_suffix(
        &project_root.join(".yode").join("transcripts"),
        ".md",
        4,
    )));
    lines.push("Recent remote artifacts:".to_string());
    lines.extend(artifact_history_lines(
        recent_artifacts_by_suffix(&project_root.join(".yode").join("remote"), ".json", 8)
            .into_iter()
            .chain(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("remote"),
                ".md",
                4,
            ))
            .collect::<Vec<_>>(),
    ));
    lines.push("Recent bundles:".to_string());
    for path in recent_bundle_workspace_indexes(cwd, 4) {
        lines.push(artifact_display_line(&path));
    }
    if latest_bundle_workspace_index(cwd).is_none() {
        lines.push("none".to_string());
    }
    lines
}

fn artifact_summary_lines(project_root: &std::path::Path, cwd: &std::path::Path) -> Vec<String> {
    vec![
        "Counts:".to_string(),
        format!(
            "status={} startup={} remote={} reviews={} transcripts={} bundles={}",
            recent_artifacts_by_suffix(&project_root.join(".yode").join("status"), ".md", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("startup"), ".json", usize::MAX).len()
                + recent_artifacts_by_suffix(&project_root.join(".yode").join("startup"), ".txt", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("remote"), ".json", usize::MAX).len()
                + recent_artifacts_by_suffix(&project_root.join(".yode").join("remote"), ".md", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("reviews"), ".md", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("transcripts"), ".md", usize::MAX).len(),
            recent_bundle_workspace_indexes(cwd, usize::MAX).len(),
        ),
        "Latest:".to_string(),
        latest_workflow_execution_artifact(project_root)
            .map(|path| format!("workflow -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "workflow -> none".to_string()),
        latest_coordinator_artifact(project_root)
            .map(|path| format!("coordinate -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "coordinate -> none".to_string()),
        latest_runtime_orchestration_artifact(project_root)
            .map(|path| format!("orchestration -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "orchestration -> none".to_string()),
        latest_bundle_workspace_index(cwd)
            .map(|path| format!("bundle -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "bundle -> none".to_string()),
    ]
}

fn artifact_history_family_lines(
    family: &str,
    project_root: &std::path::Path,
    cwd: &std::path::Path,
    runtime: Option<&yode_core::engine::EngineRuntimeState>,
) -> Result<Vec<String>, String> {
    let paths: Vec<PathBuf> = match family {
        "status" => recent_artifacts_by_suffix(&project_root.join(".yode").join("status"), ".md", 12),
        "remote" => recent_artifacts_by_suffix(&project_root.join(".yode").join("remote"), ".json", 8)
            .into_iter()
            .chain(recent_artifacts_by_suffix(&project_root.join(".yode").join("remote"), ".md", 4))
            .collect(),
        "startup" => recent_artifacts_by_suffix(&project_root.join(".yode").join("startup"), ".json", 8)
            .into_iter()
            .chain(recent_artifacts_by_suffix(&project_root.join(".yode").join("startup"), ".txt", 4))
            .collect(),
        "reviews" => recent_artifacts_by_suffix(&project_root.join(".yode").join("reviews"), ".md", 12),
        "transcripts" => {
            recent_artifacts_by_suffix(&project_root.join(".yode").join("transcripts"), ".md", 12)
        }
        "bundles" => recent_bundle_workspace_indexes(cwd, 8),
        "workflow" => {
            recent_artifacts_by_suffix(&project_root.join(".yode").join("status"), "workflow-execution.md", 8)
        }
        "coordinate" => recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "coordinate-summary.md",
            8,
        )
        .into_iter()
        .chain(recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "coordinate-dry-run.md",
            4,
        ))
        .collect(),
        "runtime" => {
            let mut paths = recent_artifacts_by_suffix(
                &project_root.join(".yode").join("status"),
                "runtime-timeline.md",
                4,
            );
            paths.extend(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("status"),
                "runtime-tasks.md",
                4,
            ));
            if let Some(state) = runtime {
                for candidate in [
                    state.last_tool_turn_artifact_path.as_deref(),
                    state.last_recovery_artifact_path.as_deref(),
                    state.last_permission_artifact_path.as_deref(),
                    state.last_compaction_session_memory_path.as_deref(),
                ] {
                    if let Some(path) = candidate {
                        let path = PathBuf::from(path);
                        if path.exists() {
                            paths.push(path);
                        }
                    }
                }
            }
            paths
        }
        other => return Err(format!("Unknown artifact history family '{}'.", other)),
    };
    if paths.is_empty() {
        Ok(vec!["Overview:".to_string(), "none".to_string()])
    } else {
        let mut lines = vec!["Overview:".to_string()];
        lines.extend(artifact_history_lines(paths));
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::{artifact_history_family_lines, artifact_inventory_lines, artifact_summary_lines};

    #[test]
    fn inventory_and_summary_lines_surface_aliases_and_counts() {
        let dir = std::env::temp_dir().join(format!("yode-inspect-artifacts-{}", uuid::Uuid::new_v4()));
        let status = dir.join(".yode").join("status");
        let remote = dir.join(".yode").join("remote");
        let startup = dir.join(".yode").join("startup");
        let reviews = dir.join(".yode").join("reviews");
        let transcripts = dir.join(".yode").join("transcripts");
        let bundle = dir.join("diagnostics-sample");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&status).unwrap();
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&startup).unwrap();
        std::fs::create_dir_all(&reviews).unwrap();
        std::fs::create_dir_all(&transcripts).unwrap();
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(status.join("aaa-runtime-timeline.md"), "x").unwrap();
        std::fs::write(remote.join("bbb-remote-execution-state.json"), "x").unwrap();
        std::fs::write(startup.join("ccc-provider-inventory.json"), "x").unwrap();
        std::fs::write(reviews.join("ddd-review.md"), "x").unwrap();
        std::fs::write(transcripts.join("eee-transcript.md"), "x").unwrap();
        std::fs::write(bundle.join("workspace-index.md"), "x").unwrap();

        let inventory = artifact_inventory_lines(&dir, &dir);
        assert!(inventory.iter().any(|line| line.contains("latest-runtime-timeline")));
        assert!(inventory.iter().any(|line| line.contains("[fresh]")));

        let summary = artifact_summary_lines(&dir, &dir);
        assert!(summary.iter().any(|line| line.contains("status=")));
        assert!(summary.iter().any(|line| line.contains("bundle ->")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_family_errors_for_unknown_values() {
        let dir = std::env::temp_dir().join(format!("yode-inspect-history-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let err = artifact_history_family_lines("unknown", &dir, &dir, None).unwrap_err();
        assert!(err.contains("Unknown artifact history family"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
