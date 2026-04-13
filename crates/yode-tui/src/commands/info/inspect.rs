use std::path::PathBuf;

use crate::commands::artifact_nav::{
    latest_artifact_by_suffix, latest_bundle_workspace_index, latest_coordinator_artifact,
    latest_runtime_orchestration_artifact, latest_workflow_execution_artifact,
    open_artifact_inspector, recent_artifacts_by_suffix, resolve_artifact_basename,
    stale_artifact_actions,
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
    let remote_dir = project_root.join(".yode").join("remote");
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let (path, title, kind, refresh) = match args {
        "list" => {
            let lines = artifact_inventory_lines(&project_root, &cwd);
            return Ok(CommandOutput::OpenInspector(document_from_command_output(
                "Artifact inventory",
                lines,
            )));
        }
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
        "artifact latest-workflow".to_string(),
        "artifact latest-coordinate".to_string(),
        "artifact latest-orchestration".to_string(),
        "artifact latest-remote-capability".to_string(),
        "artifact latest-remote-execution".to_string(),
        "artifact bundle".to_string(),
    ];
    let status_dir = project_root.join(".yode").join("status");
    let remote_dir = project_root.join(".yode").join("remote");
    for path in recent_artifacts_by_suffix(&status_dir, ".md", 6)
        .into_iter()
        .chain(recent_artifacts_by_suffix(&remote_dir, ".json", 4))
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
        "latest-workflow".to_string(),
        "latest-coordinate".to_string(),
        "latest-orchestration".to_string(),
        "latest-remote-capability".to_string(),
        "latest-remote-execution".to_string(),
        "bundle".to_string(),
        "Recent status artifacts:".to_string(),
    ];
    for path in recent_artifacts_by_suffix(&project_root.join(".yode").join("status"), ".md", 8) {
        lines.push(path.display().to_string());
    }
    lines.push("Recent remote artifacts:".to_string());
    for path in recent_artifacts_by_suffix(&project_root.join(".yode").join("remote"), ".json", 8) {
        lines.push(path.display().to_string());
    }
    lines.push("Recent bundles:".to_string());
    if let Some(path) = latest_bundle_workspace_index(cwd) {
        lines.push(path.display().to_string());
    } else {
        lines.push("none".to_string());
    }
    lines
}
