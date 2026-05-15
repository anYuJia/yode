use std::path::{Path, PathBuf};

use crate::commands::artifact_nav::{
    artifact_display_line, artifact_history_lines, attach_inspector_actions,
    latest_action_history_artifact, latest_action_metrics_artifact, latest_agent_team_artifact,
    latest_agent_team_bundle_artifact, latest_agent_team_messages_artifact,
    latest_agent_team_monitor_artifact, latest_agent_team_state_artifact,
    latest_artifact_by_suffix, latest_branch_artifact, latest_branch_merge_artifact,
    latest_branch_merge_state_artifact, latest_branch_state_artifact,
    latest_bundle_workspace_index, latest_checkpoint_artifact, latest_checkpoint_state_artifact,
    latest_coordinator_artifact, latest_coordinator_state_artifact, latest_hook_deferred_artifact,
    latest_hook_deferred_state_artifact, latest_mcp_resource_artifact,
    latest_mcp_resource_index_artifact, latest_media_compact_events_artifact,
    latest_permission_governance_artifact, latest_post_compact_restore_artifact,
    latest_post_compact_restore_diff_artifact, latest_post_compact_restore_state_artifact,
    latest_prompt_cache_artifact, latest_prompt_cache_break_artifact,
    latest_prompt_cache_diff_artifact, latest_prompt_cache_events_artifact,
    latest_prompt_cache_state_artifact, latest_remote_command_queue_artifact,
    latest_remote_control_artifact, latest_remote_control_state_artifact,
    latest_remote_live_session_artifact, latest_remote_live_session_state_artifact,
    latest_remote_queue_execution_artifact, latest_remote_session_transcript_sync_artifact,
    latest_remote_task_handoff_artifact, latest_remote_transport_artifact,
    latest_remote_transport_events_artifact, latest_remote_transport_state_artifact,
    latest_rewind_anchor_artifact, latest_rewind_anchor_state_artifact,
    latest_runtime_orchestration_artifact, latest_subagent_result_artifact,
    latest_workflow_execution_artifact, latest_workflow_state_artifact, open_artifact_inspector,
    recent_artifacts_by_suffix, recent_bundle_workspace_indexes,
    recent_mcp_resource_index_artifacts, resolve_artifact_basename, stale_artifact_actions,
};
use crate::commands::context::CommandContext;
use crate::commands::inspector_bridge::document_from_command_output;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use crate::mcp_resource_artifacts::{mcp_resource_manifest_badges, mcp_resource_manifest_summary};

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
                    hint: "[tasks|memory|reviews|status|diagnostics|doctor|hooks|permissions|workflows|coordinate|checkpoint|remote-control|artifact]".to_string(),
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
            value if value.starts_with("checkpoint") => (
                "checkpoint",
                value.strip_prefix("checkpoint").unwrap_or("").trim(),
                "Checkpoint inspector".to_string(),
            ),
            value if value.starts_with("remote-control") => (
                "remote-control",
                value.strip_prefix("remote-control").unwrap_or("").trim(),
                "Remote control inspector".to_string(),
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
            CommandOutput::Messages(lines) => Ok(CommandOutput::OpenInspector(
                document_from_command_output(&title, lines),
            )),
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
    let runtime = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| engine.runtime_state());

    if args == "list" {
        let lines = artifact_inventory_lines(&project_root, &project_root);
        return Ok(CommandOutput::OpenInspector(document_from_command_output(
            "Artifact inventory",
            lines,
        )));
    }
    if args == "summary" {
        let lines = artifact_summary_lines(&project_root, &project_root, runtime.as_ref());
        return Ok(CommandOutput::OpenInspector(document_from_command_output(
            "Artifact summary",
            lines,
        )));
    }
    if args == "history" || args.starts_with("history ") {
        let family = args.strip_prefix("history").unwrap_or("").trim();
        let family = if family.is_empty() { "status" } else { family };
        let lines =
            artifact_history_family_lines(family, &project_root, &project_root, runtime.as_ref())?;
        return Ok(CommandOutput::OpenInspector(document_from_command_output(
            &format!("Artifact history [{}]", family),
            lines,
        )));
    }

    let (path, title, kind, refresh): (PathBuf, String, String, Vec<String>) = match args {
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
            vec![
                "/coordinate latest".to_string(),
                "/coordinate timeline".to_string(),
            ],
        ),
        "latest-checkpoint" => (
            latest_checkpoint_artifact(&project_root)
                .ok_or_else(|| "No checkpoint artifact found.".to_string())?,
            "Session checkpoint inspector".to_string(),
            "checkpoint".to_string(),
            vec![
                "/checkpoint latest".to_string(),
                "/checkpoint list".to_string(),
            ],
        ),
        "latest-checkpoint-state" => (
            latest_checkpoint_state_artifact(&project_root)
                .ok_or_else(|| "No checkpoint state artifact found.".to_string())?,
            "Session checkpoint state".to_string(),
            "checkpoint".to_string(),
            vec![
                "/checkpoint latest".to_string(),
                "/checkpoint restore-dry-run latest".to_string(),
            ],
        ),
        "latest-branch" => (
            latest_branch_artifact(&project_root)
                .ok_or_else(|| "No branch artifact found.".to_string())?,
            "Session branch inspector".to_string(),
            "branch".to_string(),
            vec![
                "/checkpoint branch latest".to_string(),
                "/checkpoint branch list".to_string(),
            ],
        ),
        "latest-branch-state" => (
            latest_branch_state_artifact(&project_root)
                .ok_or_else(|| "No branch state artifact found.".to_string())?,
            "Session branch state".to_string(),
            "branch".to_string(),
            vec!["/checkpoint branch latest".to_string()],
        ),
        "latest-rewind-anchor" => (
            latest_rewind_anchor_artifact(&project_root)
                .ok_or_else(|| "No rewind anchor artifact found.".to_string())?,
            "Rewind anchor inspector".to_string(),
            "rewind".to_string(),
            vec![
                "/checkpoint rewind latest".to_string(),
                "/checkpoint rewind-anchor latest".to_string(),
            ],
        ),
        "latest-rewind-anchor-state" => (
            latest_rewind_anchor_state_artifact(&project_root)
                .ok_or_else(|| "No rewind anchor state artifact found.".to_string())?,
            "Rewind anchor state".to_string(),
            "rewind".to_string(),
            vec!["/checkpoint rewind latest".to_string()],
        ),
        "latest-branch-merge" => (
            latest_branch_merge_artifact(&project_root)
                .ok_or_else(|| "No branch merge preview artifact found.".to_string())?,
            "Branch merge preview".to_string(),
            "branch_merge".to_string(),
            vec!["/checkpoint branch merge-dry-run latest".to_string()],
        ),
        "latest-branch-merge-state" => (
            latest_branch_merge_state_artifact(&project_root)
                .ok_or_else(|| "No branch merge state artifact found.".to_string())?,
            "Branch merge state".to_string(),
            "branch_merge".to_string(),
            vec!["/checkpoint branch merge-dry-run latest".to_string()],
        ),
        "latest-remote-control" => (
            latest_remote_control_artifact(&project_root)
                .ok_or_else(|| "No remote control artifact found.".to_string())?,
            "Remote control inspector".to_string(),
            "remote_control".to_string(),
            vec!["/remote-control latest".to_string()],
        ),
        "latest-remote-control-state" => (
            latest_remote_control_state_artifact(&project_root)
                .ok_or_else(|| "No remote control state artifact found.".to_string())?,
            "Remote control state".to_string(),
            "remote_control".to_string(),
            vec![
                "/remote-control latest".to_string(),
                "/remote-control doctor".to_string(),
            ],
        ),
        "latest-remote-queue" => (
            latest_remote_command_queue_artifact(&project_root)
                .ok_or_else(|| "No remote command queue artifact found.".to_string())?,
            "Remote command queue".to_string(),
            "remote_control".to_string(),
            vec!["/remote-control queue".to_string()],
        ),
        "latest-remote-task-handoff" => (
            latest_remote_task_handoff_artifact(&project_root)
                .ok_or_else(|| "No remote task handoff artifact found.".to_string())?,
            "Remote task handoff".to_string(),
            "remote_task".to_string(),
            vec!["/remote-control handoff latest".to_string()],
        ),
        "latest-remote-queue-execution" => (
            latest_remote_queue_execution_artifact(&project_root)
                .ok_or_else(|| "No remote queue execution artifact found.".to_string())?,
            "Remote queue execution".to_string(),
            "remote_queue".to_string(),
            vec![
                "/remote-control dispatch latest".to_string(),
                "/remote-control complete latest remote completion confirmed".to_string(),
            ],
        ),
        "latest-remote-transport" => (
            latest_remote_transport_artifact(&project_root)
                .ok_or_else(|| "No remote transport artifact found.".to_string())?,
            "Remote transport".to_string(),
            "remote_transport".to_string(),
            vec!["/remote-control doctor".to_string()],
        ),
        "latest-remote-transport-state" => (
            latest_remote_transport_state_artifact(&project_root)
                .ok_or_else(|| "No remote transport state artifact found.".to_string())?,
            "Remote transport state".to_string(),
            "remote_transport".to_string(),
            vec!["/remote-control doctor".to_string()],
        ),
        "latest-remote-transport-events" => (
            latest_remote_transport_events_artifact(&project_root)
                .ok_or_else(|| "No remote transport event artifact found.".to_string())?,
            "Remote transport events".to_string(),
            "remote_transport".to_string(),
            vec![
                "/remote-control transport".to_string(),
                "/remote-control run latest".to_string(),
            ],
        ),
        "latest-remote-live-session" => (
            latest_remote_live_session_artifact(&project_root)
                .ok_or_else(|| "No remote live session artifact found.".to_string())?,
            "Remote live session".to_string(),
            "remote_live_session".to_string(),
            vec!["/remote-control session".to_string()],
        ),
        "latest-remote-live-session-state" => (
            latest_remote_live_session_state_artifact(&project_root)
                .ok_or_else(|| "No remote live session state artifact found.".to_string())?,
            "Remote live session state".to_string(),
            "remote_live_session".to_string(),
            vec!["/remote-control session sync".to_string()],
        ),
        "latest-remote-session-transcript-sync" => (
            latest_remote_session_transcript_sync_artifact(&project_root)
                .ok_or_else(|| "No remote session transcript sync artifact found.".to_string())?,
            "Remote session transcript sync".to_string(),
            "remote_live_session".to_string(),
            vec!["/remote-control session".to_string()],
        ),
        "latest-agent-team" => (
            latest_agent_team_artifact(&project_root)
                .ok_or_else(|| "No agent team artifact found.".to_string())?,
            "Agent team".to_string(),
            "agent_team".to_string(),
            vec!["/inspect artifact history teams".to_string()],
        ),
        "latest-agent-team-state" => (
            latest_agent_team_state_artifact(&project_root)
                .ok_or_else(|| "No agent team state artifact found.".to_string())?,
            "Agent team state".to_string(),
            "agent_team".to_string(),
            vec!["/inspect artifact latest-agent-team".to_string()],
        ),
        "latest-agent-team-messages" => (
            latest_agent_team_messages_artifact(&project_root)
                .ok_or_else(|| "No agent team messages artifact found.".to_string())?,
            "Agent team messages".to_string(),
            "agent_team".to_string(),
            vec!["/inspect artifact latest-agent-team".to_string()],
        ),
        "latest-agent-team-monitor" => (
            latest_agent_team_monitor_artifact(&project_root)
                .ok_or_else(|| "No agent team monitor artifact found.".to_string())?,
            "Agent team monitor".to_string(),
            "agent_team".to_string(),
            vec!["/inspect artifact latest-agent-team-state".to_string()],
        ),
        "latest-agent-team-bundle" => (
            latest_agent_team_bundle_artifact(&project_root)
                .ok_or_else(|| "No agent team bundle artifact found.".to_string())?,
            "Agent team bundle".to_string(),
            "agent_team".to_string(),
            vec!["/inspect artifact latest-agent-team-monitor".to_string()],
        ),
        "latest-subagent-result" => (
            latest_subagent_result_artifact(&project_root)
                .ok_or_else(|| "No subagent result artifact found.".to_string())?,
            "Subagent result".to_string(),
            "agent_team".to_string(),
            vec!["/inspect artifact history teams".to_string()],
        ),
        "latest-workflow-state" => (
            latest_workflow_state_artifact(&project_root)
                .ok_or_else(|| "No workflow state artifact found.".to_string())?,
            "Workflow runtime state".to_string(),
            "workflow".to_string(),
            vec!["/inspect artifact latest-workflow".to_string()],
        ),
        "latest-coordinate-state" => (
            latest_coordinator_state_artifact(&project_root)
                .ok_or_else(|| "No coordinator state artifact found.".to_string())?,
            "Coordinator runtime state".to_string(),
            "coordinate".to_string(),
            vec!["/inspect artifact latest-coordinate".to_string()],
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
            latest_bundle_workspace_index(&project_root)
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
            vec![
                "/doctor bundle".to_string(),
                "/inspect artifact history runtime".to_string(),
            ],
        ),
        "latest-runtime-tasks" => (
            latest_artifact_by_suffix(&status_dir, "runtime-tasks.md")
                .ok_or_else(|| "No runtime task artifact found.".to_string())?,
            "Runtime task inventory artifact".to_string(),
            "runtime".to_string(),
            vec!["/tasks latest".to_string()],
        ),
        "latest-action-history" => (
            latest_action_history_artifact(&project_root)
                .ok_or_else(|| "No inspector action history artifact found.".to_string())?,
            "Inspector action history".to_string(),
            "actions".to_string(),
            vec!["/inspect artifact history status".to_string()],
        ),
        "latest-action-metrics" => (
            latest_action_metrics_artifact(&project_root)
                .ok_or_else(|| "No inspector action metrics artifact found.".to_string())?,
            "Inspector action metrics".to_string(),
            "actions".to_string(),
            vec!["/inspect artifact history actions".to_string()],
        ),
        "latest-prompt-cache" => (
            latest_prompt_cache_artifact(&project_root)
                .ok_or_else(|| "No prompt cache artifact found.".to_string())?,
            "Prompt cache artifact".to_string(),
            "prompt-cache".to_string(),
            vec!["/status".to_string(), "/doctor".to_string()],
        ),
        "latest-prompt-cache-state" => (
            latest_prompt_cache_state_artifact(&project_root)
                .ok_or_else(|| "No prompt cache state artifact found.".to_string())?,
            "Prompt cache state".to_string(),
            "prompt-cache".to_string(),
            vec!["/status".to_string(), "/doctor".to_string()],
        ),
        "latest-prompt-cache-events" => (
            latest_prompt_cache_events_artifact(&project_root)
                .ok_or_else(|| "No prompt cache events artifact found.".to_string())?,
            "Prompt cache events".to_string(),
            "prompt-cache".to_string(),
            vec!["/status".to_string(), "/doctor".to_string()],
        ),
        "latest-media-compact-events" => (
            latest_media_compact_events_artifact(&project_root)
                .ok_or_else(|| "No media compact events artifact found.".to_string())?,
            "Media compact events".to_string(),
            "compact".to_string(),
            vec![
                "/context".to_string(),
                "/status".to_string(),
                "/inspect artifact history compact".to_string(),
                "/compact help".to_string(),
            ],
        ),
        "latest-prompt-cache-break" => (
            latest_prompt_cache_break_artifact(&project_root)
                .ok_or_else(|| "No prompt cache break artifact found.".to_string())?,
            "Prompt cache break".to_string(),
            "prompt-cache".to_string(),
            vec!["/status".to_string(), "/doctor".to_string()],
        ),
        "latest-prompt-cache-diff" => (
            latest_prompt_cache_diff_artifact(&project_root)
                .ok_or_else(|| "No prompt cache diff artifact found.".to_string())?,
            "Prompt cache diff".to_string(),
            "prompt-cache".to_string(),
            vec!["/status".to_string(), "/doctor".to_string()],
        ),
        "latest-post-compact-restore" => (
            latest_post_compact_restore_artifact(&project_root)
                .ok_or_else(|| "No post-compact restore artifact found.".to_string())?,
            "Post-compact restore".to_string(),
            "compact".to_string(),
            vec![
                "/context".to_string(),
                "/compact help".to_string(),
                "/inspect artifact history compact".to_string(),
                "/status".to_string(),
            ],
        ),
        "latest-post-compact-restore-state" => (
            latest_post_compact_restore_state_artifact(&project_root)
                .ok_or_else(|| "No post-compact restore state artifact found.".to_string())?,
            "Post-compact restore state".to_string(),
            "compact".to_string(),
            vec![
                "/context".to_string(),
                "/compact help".to_string(),
                "/inspect artifact history compact".to_string(),
                "/status".to_string(),
            ],
        ),
        "latest-post-compact-restore-diff" => (
            latest_post_compact_restore_diff_artifact(&project_root)
                .ok_or_else(|| "No post-compact restore diff artifact found.".to_string())?,
            "Post-compact restore diff".to_string(),
            "compact".to_string(),
            vec![
                "/context".to_string(),
                "/compact help".to_string(),
                "/inspect artifact history compact".to_string(),
                "/status".to_string(),
            ],
        ),
        "latest-mcp-resource" => (
            latest_mcp_resource_artifact(&project_root)
                .ok_or_else(|| "No MCP resource artifact found.".to_string())?,
            "MCP resource artifact".to_string(),
            "mcp_resource".to_string(),
            vec![
                "/mcp".to_string(),
                "/inspect artifact history mcp-resources".to_string(),
            ],
        ),
        "latest-mcp-resource-index" => (
            latest_mcp_resource_index_artifact(&project_root)
                .ok_or_else(|| "No MCP resource index artifact found.".to_string())?,
            "MCP resource index".to_string(),
            "mcp_resource_index".to_string(),
            vec![
                "/inspect artifact latest-mcp-resource".to_string(),
                "/inspect artifact history mcp-resources".to_string(),
                "/mcp resources cleanup".to_string(),
            ],
        ),
        "latest-hook-failures" => (
            latest_artifact_by_suffix(&status_dir, "hook-failures.md")
                .ok_or_else(|| "No hook failure artifact found.".to_string())?,
            "Hook failure artifact".to_string(),
            "hook".to_string(),
            vec!["/hooks".to_string()],
        ),
        "latest-hook-deferred" => (
            latest_hook_deferred_artifact(&project_root)
                .ok_or_else(|| "No hook deferred artifact found.".to_string())?,
            "Hook deferred artifact".to_string(),
            "hook".to_string(),
            vec!["/inspect artifact history hooks".to_string()],
        ),
        "latest-hook-deferred-state" => (
            latest_hook_deferred_state_artifact(&project_root)
                .ok_or_else(|| "No hook deferred state artifact found.".to_string())?,
            "Hook deferred state artifact".to_string(),
            "hook".to_string(),
            vec!["/inspect artifact latest-hook-deferred".to_string()],
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
        "latest-permission-policy" => (
            latest_artifact_by_suffix(&startup_dir, "permission-policy.json")
                .ok_or_else(|| "No permission policy artifact found.".to_string())?,
            "Permission policy artifact".to_string(),
            "permission".to_string(),
            vec!["/permissions governance".to_string()],
        ),
        "latest-settings-scopes" => (
            latest_artifact_by_suffix(&startup_dir, "settings-scopes.json")
                .ok_or_else(|| "No settings scopes artifact found.".to_string())?,
            "Settings scopes artifact".to_string(),
            "startup".to_string(),
            vec!["/mcp".to_string()],
        ),
        "latest-managed-mcp-inventory" => (
            latest_artifact_by_suffix(&startup_dir, "managed-mcp-inventory.json")
                .ok_or_else(|| "No managed MCP inventory artifact found.".to_string())?,
            "Managed MCP inventory artifact".to_string(),
            "startup".to_string(),
            vec!["/mcp".to_string()],
        ),
        "latest-tool-search-activation" => (
            latest_artifact_by_suffix(&startup_dir, "tool-search-activation.json")
                .ok_or_else(|| "No tool search activation artifact found.".to_string())?,
            "Tool search activation artifact".to_string(),
            "startup".to_string(),
            vec!["/tools diag".to_string()],
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
                .and_then(|state| {
                    state
                        .last_tool_turn_artifact_path
                        .as_ref()
                        .map(PathBuf::from)
                })
                .filter(|path| path.exists())
                .ok_or_else(|| "No tool artifact found.".to_string())?,
            "Tool artifact".to_string(),
            "runtime".to_string(),
            vec!["/tools".to_string(), "/brief".to_string()],
        ),
        "latest-recovery" => (
            runtime
                .as_ref()
                .and_then(|state| {
                    state
                        .last_recovery_artifact_path
                        .as_ref()
                        .map(PathBuf::from)
                })
                .filter(|path| path.exists())
                .ok_or_else(|| "No recovery artifact found.".to_string())?,
            "Recovery artifact".to_string(),
            "recovery".to_string(),
            vec!["/hooks".to_string(), "/brief".to_string()],
        ),
        "latest-permission" => (
            runtime
                .as_ref()
                .and_then(|state| {
                    state
                        .last_permission_artifact_path
                        .as_ref()
                        .map(PathBuf::from)
                })
                .filter(|path| path.exists())
                .ok_or_else(|| "No permission artifact found.".to_string())?,
            "Permission artifact".to_string(),
            "permission".to_string(),
            vec!["/permissions".to_string()],
        ),
        "latest-permission-governance" => (
            latest_permission_governance_artifact(&project_root)
                .ok_or_else(|| "No permission governance artifact found.".to_string())?,
            "Permission governance artifact".to_string(),
            "permission".to_string(),
            vec!["/permissions governance".to_string()],
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
    let mut extra_badges = vec![("kind".into(), kind.clone())];
    if kind == "mcp_resource" {
        if let Some(summary) = mcp_resource_manifest_summary(&path, true, " · ") {
            footer_lines.push(format!("resource {}", summary));
        }
        footer_lines.push("cleanup /mcp resources cleanup [keep=N|all]".to_string());
        extra_badges.extend(mcp_resource_manifest_badges(&path));
    } else if kind == "mcp_resource_index" {
        footer_lines.push("cleanup /mcp resources cleanup [keep=N|all]".to_string());
        extra_badges.push(("scope".to_string(), "mcp-resources".to_string()));
    }
    let doc = open_artifact_inspector(
        &title,
        &path,
        (!footer_lines.is_empty()).then(|| footer_lines.join("\n")),
        extra_badges,
    )
    .ok_or_else(|| format!("Failed to open artifact {}.", path.display()))?;
    let mut doc = doc;
    let actions = artifact_inspector_actions(&kind, &refresh);
    if !actions.is_empty() {
        attach_inspector_actions(&mut doc, actions);
    }
    Ok(CommandOutput::OpenInspector(doc))
}

fn artifact_inspector_actions(kind: &str, refresh: &[String]) -> Vec<(String, String)> {
    let mut actions = refresh
        .iter()
        .map(|command| (command.clone(), command.clone()))
        .collect::<Vec<_>>();
    if kind == "mcp_resource" || kind == "mcp_resource_index" {
        actions.push((
            "cleanup old MCP resource artifacts".to_string(),
            "/mcp resources cleanup".to_string(),
        ));
    } else if kind == "compact" {
        for (label, command) in [
            ("show context pressure", "/context"),
            ("show compact command help", "/compact help"),
            (
                "show compact artifact history",
                "/inspect artifact history compact",
            ),
        ] {
            if !actions.iter().any(|(_, existing)| existing == command) {
                actions.push((label.to_string(), command.to_string()));
            }
        }
    }
    actions
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
        "checkpoint".to_string(),
        "artifact list".to_string(),
        "artifact summary".to_string(),
        "artifact history".to_string(),
        "artifact history checkpoints".to_string(),
        "artifact history branches".to_string(),
        "artifact history rewind".to_string(),
        "artifact history status".to_string(),
        "artifact history state".to_string(),
        "artifact history remote".to_string(),
        "artifact history hooks".to_string(),
        "artifact history startup".to_string(),
        "artifact history reviews".to_string(),
        "artifact history transcripts".to_string(),
        "artifact history bundles".to_string(),
        "artifact history workflow".to_string(),
        "artifact history coordinate".to_string(),
        "artifact history runtime".to_string(),
        "artifact history actions".to_string(),
        "artifact history teams".to_string(),
        "artifact history mcp-resources".to_string(),
        "artifact latest-workflow".to_string(),
        "artifact latest-checkpoint".to_string(),
        "artifact latest-checkpoint-state".to_string(),
        "artifact latest-branch".to_string(),
        "artifact latest-branch-state".to_string(),
        "artifact latest-branch-merge".to_string(),
        "artifact latest-branch-merge-state".to_string(),
        "artifact latest-rewind-anchor".to_string(),
        "artifact latest-rewind-anchor-state".to_string(),
        "artifact latest-remote-control".to_string(),
        "artifact latest-remote-control-state".to_string(),
        "artifact latest-remote-queue".to_string(),
        "artifact latest-remote-queue-execution".to_string(),
        "artifact latest-remote-transport".to_string(),
        "artifact latest-remote-transport-state".to_string(),
        "artifact latest-remote-transport-events".to_string(),
        "artifact latest-remote-live-session".to_string(),
        "artifact latest-remote-live-session-state".to_string(),
        "artifact latest-remote-session-transcript-sync".to_string(),
        "artifact latest-agent-team".to_string(),
        "artifact latest-agent-team-state".to_string(),
        "artifact latest-agent-team-messages".to_string(),
        "artifact latest-agent-team-monitor".to_string(),
        "artifact latest-agent-team-bundle".to_string(),
        "artifact latest-subagent-result".to_string(),
        "artifact latest-remote-task-handoff".to_string(),
        "artifact latest-workflow-state".to_string(),
        "artifact latest-coordinate".to_string(),
        "artifact latest-coordinate-state".to_string(),
        "artifact latest-orchestration".to_string(),
        "artifact latest-runtime-timeline".to_string(),
        "artifact latest-runtime-tasks".to_string(),
        "artifact latest-prompt-cache".to_string(),
        "artifact latest-prompt-cache-state".to_string(),
        "artifact latest-prompt-cache-events".to_string(),
        "artifact latest-prompt-cache-break".to_string(),
        "artifact latest-prompt-cache-diff".to_string(),
        "artifact latest-post-compact-restore".to_string(),
        "artifact latest-post-compact-restore-state".to_string(),
        "artifact latest-post-compact-restore-diff".to_string(),
        "artifact latest-mcp-resource".to_string(),
        "artifact latest-mcp-resource-index".to_string(),
        "artifact latest-action-history".to_string(),
        "artifact latest-action-metrics".to_string(),
        "artifact latest-hook-failures".to_string(),
        "artifact latest-hook-deferred".to_string(),
        "artifact latest-hook-deferred-state".to_string(),
        "artifact latest-startup-profile".to_string(),
        "artifact latest-startup-manifest".to_string(),
        "artifact latest-provider-inventory".to_string(),
        "artifact latest-mcp-failures".to_string(),
        "artifact latest-permission-policy".to_string(),
        "artifact latest-settings-scopes".to_string(),
        "artifact latest-managed-mcp-inventory".to_string(),
        "artifact latest-tool-search-activation".to_string(),
        "artifact latest-review".to_string(),
        "artifact latest-transcript".to_string(),
        "artifact latest-session-memory".to_string(),
        "artifact latest-tool".to_string(),
        "artifact latest-recovery".to_string(),
        "artifact latest-permission".to_string(),
        "artifact latest-permission-governance".to_string(),
        "artifact latest-remote-capability".to_string(),
        "artifact latest-remote-execution".to_string(),
        "artifact bundle".to_string(),
    ];
    let status_dir = project_root.join(".yode").join("status");
    let mcp_resource_dir = status_dir.join("mcp-resources");
    let remote_dir = project_root.join(".yode").join("remote");
    let startup_dir = project_root.join(".yode").join("startup");
    let hooks_dir = project_root.join(".yode").join("hooks");
    let teams_dir = project_root.join(".yode").join("teams");
    let agent_results_dir = project_root.join(".yode").join("agent-results");
    let checkpoint_dir = project_root.join(".yode").join("checkpoints");
    let review_dir = project_root.join(".yode").join("reviews");
    let transcript_dir = project_root.join(".yode").join("transcripts");
    for path in recent_artifacts_by_suffix(&status_dir, ".md", 6)
        .into_iter()
        .chain(recent_artifacts_by_suffix(&remote_dir, ".json", 4))
        .chain(recent_artifacts_by_suffix(&status_dir, ".json", 6))
        .chain(recent_artifacts_by_suffix(&mcp_resource_dir, ".md", 4))
        .chain(recent_artifacts_by_suffix(&mcp_resource_dir, ".b64", 2))
        .chain(recent_artifacts_by_suffix(&checkpoint_dir, ".md", 4))
        .chain(recent_artifacts_by_suffix(&checkpoint_dir, ".json", 4))
        .chain(recent_artifacts_by_suffix(&startup_dir, ".json", 4))
        .chain(recent_artifacts_by_suffix(&startup_dir, ".txt", 2))
        .chain(recent_artifacts_by_suffix(&hooks_dir, ".md", 3))
        .chain(recent_artifacts_by_suffix(&hooks_dir, ".json", 3))
        .chain(recent_artifacts_by_suffix(&teams_dir, ".md", 4))
        .chain(recent_artifacts_by_suffix(&teams_dir, ".json", 3))
        .chain(recent_artifacts_by_suffix(&review_dir, ".md", 3))
        .chain(recent_artifacts_by_suffix(&agent_results_dir, ".md", 3))
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
        "latest-agent-team | latest-agent-team-state | latest-agent-team-messages | latest-agent-team-monitor | latest-agent-team-bundle | latest-subagent-result".to_string(),
        "latest-workflow-state | latest-coordinate-state".to_string(),
        "latest-checkpoint | latest-checkpoint-state".to_string(),
        "latest-branch | latest-branch-state | latest-branch-merge | latest-branch-merge-state | latest-rewind-anchor | latest-rewind-anchor-state".to_string(),
        "latest-remote-control | latest-remote-control-state | latest-remote-queue | latest-remote-queue-execution | latest-remote-transport | latest-remote-transport-state | latest-remote-transport-events | latest-remote-live-session | latest-remote-live-session-state | latest-remote-session-transcript-sync".to_string(),
        "latest-remote-task-handoff".to_string(),
        "latest-runtime-timeline | latest-runtime-tasks | latest-prompt-cache | latest-prompt-cache-state | latest-prompt-cache-events | latest-media-compact-events | latest-prompt-cache-break | latest-prompt-cache-diff | latest-post-compact-restore | latest-post-compact-restore-state | latest-post-compact-restore-diff | latest-mcp-resource | latest-mcp-resource-index | latest-hook-failures | latest-hook-deferred | latest-hook-deferred-state".to_string(),
        "latest-permission | latest-permission-governance | latest-permission-policy".to_string(),
        "latest-action-history | latest-action-metrics".to_string(),
        "history families: runtime | prompt-cache | compact | mcp-resources | hooks | remote | startup | reviews | transcripts | bundles".to_string(),
        "latest-startup-profile | latest-startup-manifest | latest-provider-inventory | latest-mcp-failures | latest-permission-policy | latest-settings-scopes | latest-managed-mcp-inventory | latest-tool-search-activation".to_string(),
        "latest-review | latest-transcript | latest-session-memory | latest-tool | latest-recovery | latest-permission | latest-permission-governance".to_string(),
        "latest-remote-capability | latest-remote-execution | bundle".to_string(),
        "Recent status artifacts:".to_string(),
    ];
    lines.extend(artifact_history_lines(recent_artifacts_by_suffix(
        &project_root.join(".yode").join("status"),
        ".md",
        8,
    )));
    lines.extend(artifact_history_lines(recent_artifacts_by_suffix(
        &project_root.join(".yode").join("status"),
        ".json",
        8,
    )));
    lines.push("Recent checkpoint artifacts:".to_string());
    lines.extend(artifact_history_lines(
        recent_artifacts_by_suffix(&project_root.join(".yode").join("checkpoints"), ".md", 6)
            .into_iter()
            .chain(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("checkpoints"),
                ".json",
                6,
            ))
            .collect::<Vec<_>>(),
    ));
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
    lines.push("Recent hook artifacts:".to_string());
    lines.extend(artifact_history_lines(
        recent_artifacts_by_suffix(&project_root.join(".yode").join("hooks"), ".md", 6)
            .into_iter()
            .chain(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("hooks"),
                ".json",
                6,
            ))
            .collect::<Vec<_>>(),
    ));
    let mcp_resource_dir = project_root
        .join(".yode")
        .join("status")
        .join("mcp-resources");
    lines.push("Recent MCP resource artifacts:".to_string());
    lines.extend(mcp_resource_artifact_history_lines(
        recent_artifacts_by_suffix(&mcp_resource_dir, ".md", 6)
            .into_iter()
            .chain(recent_artifacts_by_suffix(&mcp_resource_dir, ".b64", 4))
            .collect::<Vec<_>>(),
    ));
    lines.push("Recent review artifacts:".to_string());
    lines.extend(artifact_history_lines(recent_artifacts_by_suffix(
        &project_root.join(".yode").join("reviews"),
        ".md",
        4,
    )));
    lines.push("Recent team artifacts:".to_string());
    lines.extend(artifact_history_lines(
        recent_artifacts_by_suffix(&project_root.join(".yode").join("teams"), ".md", 6)
            .into_iter()
            .chain(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("teams"),
                ".json",
                6,
            ))
            .chain(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("agent-results"),
                ".md",
                4,
            ))
            .collect::<Vec<_>>(),
    ));
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

fn artifact_summary_lines(
    project_root: &std::path::Path,
    cwd: &std::path::Path,
    runtime: Option<&yode_core::engine::EngineRuntimeState>,
) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(state) = runtime {
        let snapshot = crate::ui::status_summary::runtime_status_snapshot_from_parts(
            project_root,
            Some(state.clone()),
            0,
        );
        lines.push("Runtime:".to_string());
        lines.push(format!(
            "runtime -> {}",
            crate::ui::status_summary::session_runtime_summary_text(
                &snapshot,
                state.estimated_context_tokens,
            )
        ));
        lines.push(format!(
            "context -> {}",
            crate::ui::status_summary::context_window_summary_text(
                Some(state),
                state.estimated_context_tokens,
            )
        ));
        lines.push(format!(
            "tools -> {}",
            crate::ui::status_summary::tool_runtime_summary_text(state)
        ));
    }
    lines.extend([
        "Counts:".to_string(),
        format!(
            "status={} state={} checkpoints={} startup={} hooks={} remote={} mcp_resources={} teams={} reviews={} transcripts={} bundles={}",
            recent_artifacts_by_suffix(&project_root.join(".yode").join("status"), ".md", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("status"), ".json", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("checkpoints"), ".md", usize::MAX).len()
                + recent_artifacts_by_suffix(&project_root.join(".yode").join("checkpoints"), ".json", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("startup"), ".json", usize::MAX).len()
                + recent_artifacts_by_suffix(&project_root.join(".yode").join("startup"), ".txt", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("hooks"), ".md", usize::MAX).len()
                + recent_artifacts_by_suffix(&project_root.join(".yode").join("hooks"), ".json", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("remote"), ".json", usize::MAX).len()
                + recent_artifacts_by_suffix(&project_root.join(".yode").join("remote"), ".md", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("status").join("mcp-resources"), ".md", usize::MAX).len()
                + recent_artifacts_by_suffix(&project_root.join(".yode").join("status").join("mcp-resources"), ".b64", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("teams"), ".json", usize::MAX).len()
                + recent_artifacts_by_suffix(&project_root.join(".yode").join("teams"), ".md", usize::MAX).len()
                + recent_artifacts_by_suffix(&project_root.join(".yode").join("agent-results"), ".md", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("reviews"), ".md", usize::MAX).len(),
            recent_artifacts_by_suffix(&project_root.join(".yode").join("transcripts"), ".md", usize::MAX).len(),
            recent_bundle_workspace_indexes(cwd, usize::MAX).len(),
        ),
        "Latest:".to_string(),
        latest_workflow_execution_artifact(project_root)
            .map(|path| format!("workflow -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "workflow -> none".to_string()),
        latest_checkpoint_artifact(project_root)
            .map(|path| format!("checkpoint -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "checkpoint -> none".to_string()),
        latest_workflow_state_artifact(project_root)
            .map(|path| format!("workflow_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "workflow_state -> none".to_string()),
        latest_checkpoint_state_artifact(project_root)
            .map(|path| format!("checkpoint_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "checkpoint_state -> none".to_string()),
        latest_branch_artifact(project_root)
            .map(|path| format!("branch -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "branch -> none".to_string()),
        latest_branch_state_artifact(project_root)
            .map(|path| format!("branch_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "branch_state -> none".to_string()),
        latest_branch_merge_artifact(project_root)
            .map(|path| format!("branch_merge -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "branch_merge -> none".to_string()),
        latest_branch_merge_state_artifact(project_root)
            .map(|path| format!("branch_merge_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "branch_merge_state -> none".to_string()),
        latest_rewind_anchor_artifact(project_root)
            .map(|path| format!("rewind_anchor -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "rewind_anchor -> none".to_string()),
        latest_rewind_anchor_state_artifact(project_root)
            .map(|path| format!("rewind_anchor_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "rewind_anchor_state -> none".to_string()),
        latest_remote_control_artifact(project_root)
            .map(|path| format!("remote_control -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "remote_control -> none".to_string()),
        latest_remote_control_state_artifact(project_root)
            .map(|path| format!("remote_control_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "remote_control_state -> none".to_string()),
        latest_remote_command_queue_artifact(project_root)
            .map(|path| format!("remote_queue -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "remote_queue -> none".to_string()),
        latest_remote_task_handoff_artifact(project_root)
            .map(|path| format!("remote_handoff -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "remote_handoff -> none".to_string()),
        latest_remote_queue_execution_artifact(project_root)
            .map(|path: std::path::PathBuf| {
                format!("remote_queue_execution -> {}", artifact_display_line(&path))
            })
            .unwrap_or_else(|| "remote_queue_execution -> none".to_string()),
        latest_remote_transport_artifact(project_root)
            .map(|path| format!("remote_transport -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "remote_transport -> none".to_string()),
        latest_remote_transport_state_artifact(project_root)
            .map(|path| format!("remote_transport_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "remote_transport_state -> none".to_string()),
        latest_remote_transport_events_artifact(project_root)
            .map(|path| format!("remote_transport_events -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "remote_transport_events -> none".to_string()),
        latest_remote_live_session_artifact(project_root)
            .map(|path| format!("remote_live_session -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "remote_live_session -> none".to_string()),
        latest_remote_live_session_state_artifact(project_root)
            .map(|path| format!("remote_live_session_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "remote_live_session_state -> none".to_string()),
        latest_remote_session_transcript_sync_artifact(project_root)
            .map(|path| format!("remote_session_transcript_sync -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "remote_session_transcript_sync -> none".to_string()),
        latest_agent_team_artifact(project_root)
            .map(|path| format!("agent_team -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "agent_team -> none".to_string()),
        latest_agent_team_state_artifact(project_root)
            .map(|path| format!("agent_team_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "agent_team_state -> none".to_string()),
        latest_agent_team_messages_artifact(project_root)
            .map(|path| format!("agent_team_messages -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "agent_team_messages -> none".to_string()),
        latest_agent_team_monitor_artifact(project_root)
            .map(|path| format!("agent_team_monitor -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "agent_team_monitor -> none".to_string()),
        latest_agent_team_bundle_artifact(project_root)
            .map(|path| format!("agent_team_bundle -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "agent_team_bundle -> none".to_string()),
        latest_subagent_result_artifact(project_root)
            .map(|path| format!("subagent_result -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "subagent_result -> none".to_string()),
        latest_hook_deferred_artifact(project_root)
            .map(|path| format!("hook_deferred -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "hook_deferred -> none".to_string()),
        latest_hook_deferred_state_artifact(project_root)
            .map(|path| format!("hook_deferred_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "hook_deferred_state -> none".to_string()),
        latest_permission_governance_artifact(project_root)
            .map(|path| format!("permission_governance -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "permission_governance -> none".to_string()),
        latest_action_history_artifact(project_root)
            .map(|path| format!("action_history -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "action_history -> none".to_string()),
        latest_action_metrics_artifact(project_root)
            .map(|path| format!("action_metrics -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "action_metrics -> none".to_string()),
        latest_prompt_cache_artifact(project_root)
            .map(|path| format!("prompt_cache -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "prompt_cache -> none".to_string()),
        latest_prompt_cache_state_artifact(project_root)
            .map(|path| format!("prompt_cache_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "prompt_cache_state -> none".to_string()),
        latest_prompt_cache_events_artifact(project_root)
            .map(|path| format!("prompt_cache_events -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "prompt_cache_events -> none".to_string()),
        latest_prompt_cache_break_artifact(project_root)
            .map(|path| format!("prompt_cache_break -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "prompt_cache_break -> none".to_string()),
        latest_prompt_cache_diff_artifact(project_root)
            .map(|path| format!("prompt_cache_diff -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "prompt_cache_diff -> none".to_string()),
        latest_post_compact_restore_artifact(project_root)
            .map(|path| format!("post_compact_restore -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "post_compact_restore -> none".to_string()),
        latest_post_compact_restore_state_artifact(project_root)
            .map(|path| format!("post_compact_restore_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "post_compact_restore_state -> none".to_string()),
        latest_post_compact_restore_diff_artifact(project_root)
            .map(|path| format!("post_compact_restore_diff -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "post_compact_restore_diff -> none".to_string()),
        latest_mcp_resource_artifact(project_root)
            .map(|path| format!("mcp_resource -> {}", mcp_resource_artifact_display_line(&path)))
            .unwrap_or_else(|| "mcp_resource -> none".to_string()),
        latest_mcp_resource_index_artifact(cwd)
            .map(|path| format!("mcp_resource_index -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "mcp_resource_index -> none".to_string()),
        latest_coordinator_artifact(project_root)
            .map(|path| format!("coordinate -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "coordinate -> none".to_string()),
        latest_coordinator_state_artifact(project_root)
            .map(|path| format!("coordinate_state -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "coordinate_state -> none".to_string()),
        latest_runtime_orchestration_artifact(project_root)
            .map(|path| format!("orchestration -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "orchestration -> none".to_string()),
        latest_bundle_workspace_index(cwd)
            .map(|path| format!("bundle -> {}", artifact_display_line(&path)))
            .unwrap_or_else(|| "bundle -> none".to_string()),
    ]);
    lines
}

fn mcp_resource_artifact_history_lines(paths: impl IntoIterator<Item = PathBuf>) -> Vec<String> {
    paths
        .into_iter()
        .map(|path| mcp_resource_artifact_display_line(&path))
        .collect()
}

fn mcp_resource_artifact_display_line(path: &Path) -> String {
    let base = artifact_display_line(path);
    let Some(summary) = mcp_resource_manifest_summary(path, true, " · ") else {
        return base;
    };
    format!("{} · {}", base, summary)
}

fn artifact_history_family_lines(
    family: &str,
    project_root: &std::path::Path,
    cwd: &std::path::Path,
    runtime: Option<&yode_core::engine::EngineRuntimeState>,
) -> Result<Vec<String>, String> {
    let paths: Vec<PathBuf> = match family {
        "status" => {
            recent_artifacts_by_suffix(&project_root.join(".yode").join("status"), ".md", 12)
        }
        "state" => {
            recent_artifacts_by_suffix(&project_root.join(".yode").join("status"), ".json", 12)
        }
        "actions" => recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "inspector-action-history.md",
            12,
        )
        .into_iter()
        .chain(recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "inspector-action-metrics.json",
            12,
        ))
        .collect(),
        "teams" => recent_artifacts_by_suffix(&project_root.join(".yode").join("teams"), ".md", 12)
            .into_iter()
            .chain(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("teams"),
                ".json",
                12,
            ))
            .chain(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("agent-results"),
                ".md",
                12,
            ))
            .collect(),
        "hooks" => recent_artifacts_by_suffix(&project_root.join(".yode").join("hooks"), ".md", 12)
            .into_iter()
            .chain(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("hooks"),
                ".json",
                12,
            ))
            .chain(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("status"),
                "hook-failures.md",
                12,
            ))
            .collect(),
        "checkpoints" => {
            recent_artifacts_by_suffix(&project_root.join(".yode").join("checkpoints"), ".md", 12)
                .into_iter()
                .chain(recent_artifacts_by_suffix(
                    &project_root.join(".yode").join("checkpoints"),
                    ".json",
                    12,
                ))
                .collect()
        }
        "branches" => recent_artifacts_by_suffix(
            &project_root.join(".yode").join("checkpoints"),
            "branch.md",
            12,
        )
        .into_iter()
        .chain(recent_artifacts_by_suffix(
            &project_root.join(".yode").join("checkpoints"),
            "branch-state.json",
            12,
        ))
        .collect(),
        "rewind" => recent_artifacts_by_suffix(
            &project_root.join(".yode").join("checkpoints"),
            "rewind-anchor.md",
            12,
        )
        .into_iter()
        .chain(recent_artifacts_by_suffix(
            &project_root.join(".yode").join("checkpoints"),
            "rewind-anchor-state.json",
            12,
        ))
        .collect(),
        "remote" => {
            recent_artifacts_by_suffix(&project_root.join(".yode").join("remote"), ".json", 8)
                .into_iter()
                .chain(recent_artifacts_by_suffix(
                    &project_root.join(".yode").join("remote"),
                    ".md",
                    4,
                ))
                .collect()
        }
        "startup" => {
            recent_artifacts_by_suffix(&project_root.join(".yode").join("startup"), ".json", 8)
                .into_iter()
                .chain(recent_artifacts_by_suffix(
                    &project_root.join(".yode").join("startup"),
                    ".txt",
                    4,
                ))
                .collect()
        }
        "reviews" => {
            recent_artifacts_by_suffix(&project_root.join(".yode").join("reviews"), ".md", 12)
        }
        "transcripts" => {
            recent_artifacts_by_suffix(&project_root.join(".yode").join("transcripts"), ".md", 12)
        }
        "compact" => recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "post-compact-restore.md",
            12,
        )
        .into_iter()
        .chain(recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "post-compact-restore-state.json",
            12,
        ))
        .chain(recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "post-compact-restore-diff.md",
            12,
        ))
        .chain(recent_artifacts_by_suffix(
            &project_root.join(".yode").join("transcripts"),
            ".md",
            12,
        ))
        .collect(),
        "mcp-resources" => {
            let paths = recent_artifacts_by_suffix(
                &project_root
                    .join(".yode")
                    .join("status")
                    .join("mcp-resources"),
                ".md",
                12,
            )
            .into_iter()
            .chain(recent_artifacts_by_suffix(
                &project_root
                    .join(".yode")
                    .join("status")
                    .join("mcp-resources"),
                ".b64",
                12,
            ))
            .chain(recent_mcp_resource_index_artifacts(cwd, 8))
            .collect::<Vec<_>>();
            return Ok(mcp_resource_artifact_history_lines(paths));
        }
        "bundles" => recent_bundle_workspace_indexes(cwd, 8),
        "workflow" => recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "workflow-execution.md",
            8,
        ),
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
            paths.extend(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("status"),
                "prompt-cache.md",
                4,
            ));
            paths.extend(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("status"),
                "prompt-cache-events.md",
                4,
            ));
            paths.extend(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("status"),
                "media-compact-events.md",
                4,
            ));
            paths.extend(recent_artifacts_by_suffix(
                &project_root.join(".yode").join("status"),
                "post-compact-restore.md",
                4,
            ));
            if let Some(state) = runtime {
                for path in [
                    state.last_tool_turn_artifact_path.as_deref(),
                    state.last_recovery_artifact_path.as_deref(),
                    state.last_permission_artifact_path.as_deref(),
                    state.last_compaction_session_memory_path.as_deref(),
                ]
                .into_iter()
                .flatten()
                {
                    let path = PathBuf::from(path);
                    if path.exists() {
                        paths.push(path);
                    }
                }
            }
            paths
        }
        "prompt-cache" => recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "prompt-cache.md",
            8,
        )
        .into_iter()
        .chain(recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "prompt-cache-state.json",
            8,
        ))
        .chain(recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "prompt-cache-events.md",
            8,
        ))
        .chain(recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "media-compact-events.md",
            8,
        ))
        .chain(recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "prompt-cache-break.json",
            8,
        ))
        .chain(recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "prompt-cache-diff.md",
            8,
        ))
        .collect(),
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
    use super::{
        artifact_history_family_lines, artifact_inspector_actions, artifact_inventory_lines,
        artifact_summary_lines, mcp_resource_manifest_badges, mcp_resource_manifest_summary,
    };

    #[test]
    fn inventory_and_summary_lines_surface_aliases_and_counts() {
        let dir =
            std::env::temp_dir().join(format!("yode-inspect-artifacts-{}", uuid::Uuid::new_v4()));
        let status = dir.join(".yode").join("status");
        let remote = dir.join(".yode").join("remote");
        let startup = dir.join(".yode").join("startup");
        let mcp_resources = dir.join(".yode").join("status").join("mcp-resources");
        let reviews = dir.join(".yode").join("reviews");
        let transcripts = dir.join(".yode").join("transcripts");
        let bundle = dir.join("diagnostics-sample");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&status).unwrap();
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&startup).unwrap();
        std::fs::create_dir_all(&mcp_resources).unwrap();
        std::fs::create_dir_all(&reviews).unwrap();
        std::fs::create_dir_all(&transcripts).unwrap();
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(status.join("aaa-runtime-timeline.md"), "x").unwrap();
        std::fs::write(status.join("fff-prompt-cache.md"), "x").unwrap();
        std::fs::write(remote.join("bbb-remote-execution-state.json"), "x").unwrap();
        std::fs::write(startup.join("ccc-provider-inventory.json"), "x").unwrap();
        std::fs::write(mcp_resources.join("ggg-mcp-resource-sample.md"), "x").unwrap();
        std::fs::write(mcp_resources.join("ggg-mcp-resource-sample.b64"), "eA==").unwrap();
        std::fs::write(reviews.join("ddd-review.md"), "x").unwrap();
        std::fs::write(transcripts.join("eee-transcript.md"), "x").unwrap();
        std::fs::write(bundle.join("workspace-index.md"), "x").unwrap();
        std::fs::write(bundle.join("mcp-resources-index.md"), "x").unwrap();

        let inventory = artifact_inventory_lines(&dir, &dir);
        assert!(inventory
            .iter()
            .any(|line| line.contains("latest-runtime-timeline")));
        assert!(inventory
            .iter()
            .any(|line| line.contains("latest-mcp-resource")));
        assert!(inventory
            .iter()
            .any(|line| line.contains("latest-mcp-resource-index")));
        assert!(inventory.iter().any(|line| line.contains("[fresh]")));

        let summary = artifact_summary_lines(&dir, &dir, None);
        assert!(summary.iter().any(|line| line.contains("status=")));
        assert!(summary.iter().any(|line| line.contains("mcp_resources=2")));
        assert!(summary.iter().any(|line| line.contains("mcp_resource ->")));
        assert!(summary
            .iter()
            .any(|line| line.contains("mcp_resource_index ->")));
        assert!(summary.iter().any(|line| line.contains("bundle ->")));
        assert!(summary.iter().any(|line| line.contains("prompt_cache ->")));
        let bundle_index = summary
            .iter()
            .position(|line| line.starts_with("bundle ->"))
            .unwrap();
        let prompt_index = summary
            .iter()
            .position(|line| line.contains("prompt_cache ->"))
            .unwrap();
        assert!(prompt_index < bundle_index);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn history_family_errors_for_unknown_values() {
        let dir =
            std::env::temp_dir().join(format!("yode-inspect-history-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let err = artifact_history_family_lines("unknown", &dir, &dir, None).unwrap_err();
        assert!(err.contains("Unknown artifact history family"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prompt_cache_history_family_reads_prompt_cache_artifacts() {
        let dir = std::env::temp_dir().join(format!(
            "yode-inspect-prompt-cache-{}",
            uuid::Uuid::new_v4()
        ));
        let status = dir.join(".yode").join("status");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&status).unwrap();
        std::fs::write(status.join("a-prompt-cache.md"), "x").unwrap();
        std::fs::write(status.join("b-prompt-cache-events.md"), "x").unwrap();

        let lines = artifact_history_family_lines("prompt-cache", &dir, &dir, None).unwrap();
        assert!(lines.iter().any(|line| line.contains("prompt-cache.md")));
        assert!(lines
            .iter()
            .any(|line| line.contains("prompt-cache-events.md")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compact_history_family_reads_restore_and_transcript_artifacts() {
        let dir =
            std::env::temp_dir().join(format!("yode-inspect-compact-{}", uuid::Uuid::new_v4()));
        let status = dir.join(".yode").join("status");
        let transcripts = dir.join(".yode").join("transcripts");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&status).unwrap();
        std::fs::create_dir_all(&transcripts).unwrap();
        std::fs::write(status.join("a-post-compact-restore.md"), "x").unwrap();
        std::fs::write(transcripts.join("b-transcript.md"), "x").unwrap();

        let lines = artifact_history_family_lines("compact", &dir, &dir, None).unwrap();
        assert!(lines
            .iter()
            .any(|line| line.contains("post-compact-restore.md")));
        assert!(lines.iter().any(|line| line.contains("b-transcript.md")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_resources_history_family_reads_manifests_and_base64_artifacts() {
        let dir = std::env::temp_dir().join(format!(
            "yode-inspect-mcp-resources-{}",
            uuid::Uuid::new_v4()
        ));
        let resources = dir.join(".yode").join("status").join("mcp-resources");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::write(
            resources.join("a-mcp-resource-image.md"),
            "# MCP Resource Blob Artifact\n\n- Server: demo\n- URI: mcp://image\n- Blob count: 1\n- Retention: keep newest 120 artifact files\n\n## Blob 1\n\n- Decode warning: invalid base64\n",
        )
        .unwrap();
        std::fs::write(resources.join("a-mcp-resource-image.b64"), "eA==").unwrap();
        let bundle = dir.join(".yode").join("exports").join("diagnostics-mcp");
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(bundle.join("workspace-index.md"), "x").unwrap();
        std::fs::write(bundle.join("mcp-resources-index.md"), "index").unwrap();

        let lines = artifact_history_family_lines("mcp-resources", &dir, &dir, None).unwrap();
        assert!(lines
            .iter()
            .any(|line| line.contains("a-mcp-resource-image.md")));
        assert!(lines.iter().any(|line| line.contains("server=demo")));
        assert!(lines.iter().any(|line| line.contains("uri=mcp://image")));
        assert!(lines.iter().any(|line| line.contains("blobs=1")));
        assert!(lines.iter().any(|line| line.contains("decode_warnings=1")));
        assert!(lines
            .iter()
            .any(|line| line.contains("retention=keep newest 120 artifact files")));
        assert!(lines
            .iter()
            .any(|line| line.contains("a-mcp-resource-image.b64")));
        assert!(lines
            .iter()
            .any(|line| line.contains("mcp-resources-index.md")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_resource_manifest_helpers_extract_footer_and_badges() {
        let dir =
            std::env::temp_dir().join(format!("yode-inspect-mcp-badges-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("artifact.md");
        std::fs::write(
            &path,
            "# MCP Resource Blob Artifact\n\n- Server: demo\n- URI: mcp://image\n- Blob count: 2\n- Retention: keep newest 120 artifact files\n\n## Blob 1\n\n- Decode warning: invalid base64\n",
        )
        .unwrap();

        let summary = mcp_resource_manifest_summary(&path, true, " · ").unwrap();
        assert!(summary.contains("server=demo"));
        assert!(summary.contains("decode_warnings=1"));
        let badges = mcp_resource_manifest_badges(&path);
        assert!(badges.contains(&("server".to_string(), "demo".to_string())));
        assert!(badges.contains(&("blobs".to_string(), "2".to_string())));
        assert!(badges.contains(&("decode".to_string(), "warnings=1".to_string())));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_resource_inspector_actions_include_cleanup_command() {
        let actions = artifact_inspector_actions(
            "mcp_resource",
            &[
                "/mcp".to_string(),
                "/inspect artifact history mcp-resources".to_string(),
            ],
        );
        assert!(actions.contains(&("/mcp".to_string(), "/mcp".to_string())));
        assert!(actions.contains(&(
            "cleanup old MCP resource artifacts".to_string(),
            "/mcp resources cleanup".to_string()
        )));
        let index_actions = artifact_inspector_actions(
            "mcp_resource_index",
            &["/inspect artifact latest-mcp-resource".to_string()],
        );
        assert!(index_actions
            .iter()
            .any(|(_, command)| command == "/mcp resources cleanup"));

        let regular = artifact_inspector_actions("runtime", &["/status".to_string()]);
        assert!(!regular
            .iter()
            .any(|(_, command)| command.contains("resources cleanup")));
    }

    #[test]
    fn compact_inspector_actions_include_context_and_partial_compact_help() {
        let actions = artifact_inspector_actions("compact", &["/status".to_string()]);
        assert!(actions.contains(&("/status".to_string(), "/status".to_string())));
        assert!(actions.contains(&("show context pressure".to_string(), "/context".to_string())));
        assert!(actions.contains(&(
            "show compact command help".to_string(),
            "/compact help".to_string()
        )));
        assert!(actions.contains(&(
            "show compact artifact history".to_string(),
            "/inspect artifact history compact".to_string()
        )));
    }

    #[test]
    fn artifact_summary_surfaces_runtime_summaries_when_available() {
        let dir =
            std::env::temp_dir().join(format!("yode-inspect-runtime-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".yode").join("status")).unwrap();
        let state = yode_core::engine::EngineRuntimeState {
            query_source: "User".to_string(),
            autocompact_disabled: false,
            compaction_failures: 0,
            total_compactions: 1,
            auto_compactions: 1,
            manual_compactions: 0,
            last_compaction_breaker_reason: None,
            context_window_tokens: 128_000,
            compaction_threshold_tokens: 96_000,
            estimated_context_tokens: 64_000,
            message_count: 4,
            live_session_memory_initialized: true,
            live_session_memory_updating: false,
            live_session_memory_path: String::new(),
            session_tool_calls_total: 5,
            last_compaction_mode: None,
            last_compaction_at: None,
            last_compaction_summary_excerpt: None,
            last_compaction_session_memory_path: None,
            last_compaction_transcript_path: None,
            last_compact_boundary: None,
            last_session_memory_update_at: None,
            last_session_memory_update_path: None,
            last_session_memory_generated_summary: false,
            session_memory_update_count: 2,
            tracked_failed_tool_results: 0,
            hook_total_executions: 0,
            hook_timeout_count: 0,
            hook_execution_error_count: 0,
            hook_nonzero_exit_count: 0,
            hook_wake_notification_count: 0,
            stop_hook_continue_count: 0,
            last_stop_hook_continue_reason: None,
            last_hook_failure_event: None,
            last_hook_failure_command: None,
            last_hook_failure_reason: None,
            last_hook_failure_at: None,
            last_hook_timeout_command: None,
            last_compaction_prompt_tokens: None,
            last_post_compaction_estimated_tokens: None,
            last_post_compaction_threshold_tokens: None,
            last_post_compaction_will_retrigger: None,
            last_restore_budget: None,
            plan: Default::default(),
            async_task_restore_summary: None,
            context_collapse: Default::default(),
            avg_compaction_prompt_tokens: None,
            compaction_cause_histogram: std::collections::BTreeMap::new(),
            last_microcompact_media_removed: 0,
            last_microcompact_media_saved_chars: 0,
            microcompact_media_removed_total: 0,
            microcompact_media_saved_chars_total: 0,
            system_prompt_estimated_tokens: 0,
            system_prompt_segments: Vec::new(),
            prompt_cache: yode_core::engine::PromptCacheRuntimeState::default(),
            cost: Default::default(),
            last_turn_duration_ms: None,
            last_turn_stop_reason: None,
            last_turn_artifact_path: None,
            last_stream_watchdog_stage: None,
            stream_retry_reason_histogram: std::collections::BTreeMap::new(),
            recovery_state: "Normal".to_string(),
            recovery_single_step_count: 0,
            recovery_reanchor_count: 0,
            recovery_need_user_guidance_count: 0,
            last_failed_signature: None,
            recovery_breadcrumbs: Vec::new(),
            last_recovery_artifact_path: None,
            last_permission_tool: None,
            last_permission_action: None,
            last_permission_explanation: None,
            last_permission_artifact_path: None,
            recent_permission_denials: Vec::new(),
            tool_pool: yode_tools::registry::ToolPoolSnapshot::default(),
            current_turn_tool_calls: 1,
            current_turn_tool_output_bytes: 0,
            current_turn_tool_progress_events: 0,
            current_turn_parallel_batches: 0,
            current_turn_parallel_calls: 0,
            current_turn_max_parallel_batch_size: 0,
            current_turn_truncated_results: 0,
            current_turn_budget_notice_emitted: false,
            current_turn_budget_warning_emitted: false,
            tool_budget_notice_count: 0,
            tool_budget_warning_count: 0,
            last_tool_budget_warning: None,
            tool_progress_event_count: 2,
            last_tool_progress_message: None,
            last_tool_progress_tool: None,
            last_tool_progress_at: None,
            parallel_tool_batch_count: 0,
            parallel_tool_call_count: 0,
            max_parallel_batch_size: 0,
            tool_truncation_count: 0,
            last_tool_truncation_reason: None,
            latest_repeated_tool_failure: None,
            read_file_history: Vec::new(),
            command_tool_duplication_hints: Vec::new(),
            last_tool_turn_completed_at: None,
            last_tool_turn_artifact_path: None,
            tool_error_type_counts: std::collections::BTreeMap::new(),
            tool_trace_scope: "last".to_string(),
            tool_traces: Vec::new(),
        };
        let summary = artifact_summary_lines(&dir, &dir, Some(&state));
        assert!(summary.iter().any(|line| line.contains("runtime ->")));
        assert!(summary.iter().any(|line| line.contains("context ->")));
        assert!(summary.iter().any(|line| line.contains("tools ->")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
