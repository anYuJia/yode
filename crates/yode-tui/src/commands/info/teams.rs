use std::path::{Path, PathBuf};

use crate::commands::artifact_nav::latest_agent_team_state_artifact;
use crate::commands::context::CommandContext;
use crate::commands::workspace_nav::{
    workspace_breadcrumb, workspace_jump_inventory, workspace_selection_summary,
};
use crate::commands::workspace_text::{workspace_artifact_lines, workspace_bullets, WorkspaceText};
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use yode_agent::AgentTeamSnapshot;
use yode_tools::builtin::team_runtime::{
    agent_team_artifact_paths, load_agent_team_snapshot, render_agent_team_monitor_from_snapshot,
};

pub struct TeamsCommand {
    meta: CommandMeta,
}

impl TeamsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "teams",
                description: "List, inspect, and monitor multi-agent team runtime state",
                aliases: &[],
                args: vec![
                    ArgDef {
                        name: "action".into(),
                        required: false,
                        hint: "<list|latest|messages|monitor|team-id>".into(),
                        completions: ArgCompletionSource::Static(vec![
                            "list".to_string(),
                            "latest".to_string(),
                            "messages".to_string(),
                            "monitor".to_string(),
                        ]),
                    },
                    ArgDef {
                        name: "team-id".into(),
                        required: false,
                        hint: "<team-id|latest>".into(),
                        completions: ArgCompletionSource::Static(vec!["latest".to_string()]),
                    },
                ],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for TeamsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let parts = args.split_whitespace().collect::<Vec<_>>();
        let working_dir = PathBuf::from(&ctx.session.working_dir);
        let Ok(engine) = ctx.engine.try_lock() else {
            return Err("Engine is busy, try again.".into());
        };

        match parts.as_slice() {
            [] | ["list"] => Ok(CommandOutput::Message(render_team_list(&engine, &working_dir))),
            ["latest"] => render_team_detail_from_selector(&engine, &working_dir, "latest"),
            ["monitor"] => render_team_monitor_from_selector(&engine, &working_dir, "latest"),
            ["messages"] => render_team_messages_from_selector(&engine, &working_dir, "latest"),
            ["monitor", selector] => render_team_monitor_from_selector(&engine, &working_dir, selector),
            ["messages", selector] => {
                render_team_messages_from_selector(&engine, &working_dir, selector)
            }
            [selector] => render_team_detail_from_selector(&engine, &working_dir, selector),
            _ => Err(
                "Usage: /teams | /teams list | /teams latest | /teams monitor [team-id|latest] | /teams messages [team-id|latest] | /teams <team-id>".into(),
            ),
        }
    }
}

fn render_team_list(engine: &yode_core::engine::AgentEngine, working_dir: &Path) -> String {
    let team_ids = available_team_ids(engine, working_dir);
    if team_ids.is_empty() {
        return "No agent teams recorded.".to_string();
    }

    let mut lines = vec![format!("Agent teams ({})", team_ids.len())];
    for team_id in team_ids {
        if let Some(snapshot) = resolve_team_snapshot(engine, working_dir, &team_id) {
            if let Some(state) = snapshot.state {
                lines.push(format!(
                    "{} [{}] {} / {} active / {} completed / {} failed",
                    state.team_id,
                    state.mode,
                    state.goal,
                    state.active_count,
                    state.completed_count,
                    state.failed_count
                ));
            } else {
                lines.push(format!("{} [state missing]", team_id));
            }
        }
    }
    lines.join("\n")
}

fn render_team_detail_from_selector(
    engine: &yode_core::engine::AgentEngine,
    working_dir: &Path,
    selector: &str,
) -> CommandResult {
    let snapshot = resolve_team_snapshot_from_selector(engine, working_dir, selector)
        .ok_or_else(|| format!("Team '{}' not found.", selector))?;
    let state = snapshot
        .state
        .clone()
        .ok_or_else(|| "Team snapshot has no state.".to_string())?;
    let artifacts = agent_team_artifact_paths(working_dir, &state.team_id);

    let member_lines = state
        .members
        .iter()
        .map(|member| {
            format!(
                "{} [{}] inbox={} inheritance={}{}{}",
                member.member_id,
                member.status,
                member.pending_message_count,
                member.permission_inheritance,
                member
                    .runtime_task_id
                    .as_ref()
                    .map(|id| format!(" / task={}", id))
                    .unwrap_or_default(),
                member
                    .last_result_preview
                    .as_ref()
                    .map(|preview| format!(" / {}", preview))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    let message_lines = recent_message_lines(&snapshot);

    Ok(CommandOutput::Message(
        WorkspaceText::new("Team workspace")
            .subtitle(state.team_id.clone())
            .field(
                "Breadcrumb",
                workspace_breadcrumb("Teams", Some(&state.team_id)),
            )
            .field(
                "Selection",
                workspace_selection_summary(1, available_team_ids(engine, working_dir).len()),
            )
            .field("Goal", state.goal.clone())
            .field("Mode", state.mode.clone())
            .field(
                "Members",
                format!(
                    "{} total / {} active / {} completed / {} failed",
                    state.member_count,
                    state.active_count,
                    state.completed_count,
                    state.failed_count
                ),
            )
            .field("Updated", state.updated_at.clone())
            .section("Members", workspace_bullets(member_lines))
            .section("Recent messages", workspace_bullets(message_lines))
            .section(
                "Artifacts",
                workspace_artifact_lines([
                    (
                        "summary",
                        artifacts
                            .summary_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "none".to_string()),
                    ),
                    (
                        "state",
                        artifacts
                            .state_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "none".to_string()),
                    ),
                    (
                        "messages",
                        artifacts
                            .messages_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "none".to_string()),
                    ),
                    (
                        "monitor",
                        artifacts
                            .monitor_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "none".to_string()),
                    ),
                    (
                        "bundle",
                        artifacts
                            .bundle_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "none".to_string()),
                    ),
                ]),
            )
            .footer(workspace_jump_inventory(team_jump_targets(&state.team_id)))
            .render(),
    ))
}

fn render_team_monitor_from_selector(
    engine: &yode_core::engine::AgentEngine,
    working_dir: &Path,
    selector: &str,
) -> CommandResult {
    let snapshot = resolve_team_snapshot_from_selector(engine, working_dir, selector)
        .ok_or_else(|| format!("Team '{}' not found.", selector))?;
    let rendered = render_agent_team_monitor_from_snapshot(&snapshot, None, true)
        .map_err(|err| err.to_string())?;
    Ok(CommandOutput::Message(rendered))
}

fn render_team_messages_from_selector(
    engine: &yode_core::engine::AgentEngine,
    working_dir: &Path,
    selector: &str,
) -> CommandResult {
    let snapshot = resolve_team_snapshot_from_selector(engine, working_dir, selector)
        .ok_or_else(|| format!("Team '{}' not found.", selector))?;
    let state = snapshot
        .state
        .clone()
        .ok_or_else(|| "Team snapshot has no state.".to_string())?;
    let lines = recent_message_lines(&snapshot);
    Ok(CommandOutput::Message(
        WorkspaceText::new("Team messages")
            .subtitle(state.team_id)
            .section("Messages", workspace_bullets(lines))
            .footer(workspace_jump_inventory(team_jump_targets(selector)))
            .render(),
    ))
}

fn resolve_team_snapshot_from_selector(
    engine: &yode_core::engine::AgentEngine,
    working_dir: &Path,
    selector: &str,
) -> Option<AgentTeamSnapshot> {
    let team_id = if selector == "latest" {
        engine
            .runtime_latest_team_id()
            .or_else(|| latest_team_id_from_disk(working_dir))
    } else {
        Some(selector.to_string())
    }?;
    resolve_team_snapshot(engine, working_dir, &team_id)
}

fn resolve_team_snapshot(
    engine: &yode_core::engine::AgentEngine,
    working_dir: &Path,
    team_id: &str,
) -> Option<AgentTeamSnapshot> {
    engine.runtime_team_snapshot(team_id).or_else(|| {
        load_agent_team_snapshot(working_dir, team_id)
            .ok()
            .flatten()
    })
}

fn available_team_ids(engine: &yode_core::engine::AgentEngine, working_dir: &Path) -> Vec<String> {
    let mut ids = engine.runtime_team_ids();
    for id in team_ids_from_disk(working_dir) {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids.sort();
    ids
}

fn latest_team_id_from_disk(working_dir: &Path) -> Option<String> {
    latest_agent_team_state_artifact(working_dir).and_then(|path| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| stem.trim_end_matches("-agent-team-state").to_string())
    })
}

fn team_ids_from_disk(working_dir: &Path) -> Vec<String> {
    let dir = working_dir.join(".yode").join("teams");
    let mut entries = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with("agent-team-state.json"))
        })
        .filter_map(|path| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.trim_end_matches("-agent-team-state").to_string())
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

fn recent_message_lines(snapshot: &AgentTeamSnapshot) -> Vec<String> {
    if snapshot.messages.is_empty() {
        return vec!["none".to_string()];
    }
    snapshot
        .messages
        .iter()
        .rev()
        .take(8)
        .map(|message| {
            format!(
                "{} [{}:{}] {}",
                message.at, message.target, message.kind, message.message
            )
        })
        .collect()
}

fn team_jump_targets(team_id: &str) -> Vec<String> {
    vec![
        "/teams list".to_string(),
        format!("/teams monitor {}", team_id),
        format!("/teams messages {}", team_id),
        "/inspect artifact latest-agent-team".to_string(),
        "/inspect artifact latest-agent-team-monitor".to_string(),
        "/inspect artifact latest-subagent-result".to_string(),
        "/tasks monitor".to_string(),
    ]
}

#[cfg(test)]
mod tests {
    use super::{recent_message_lines, team_ids_from_disk, TeamsCommand};
    use crate::commands::{Command, CommandOutput};
    use yode_agent::{AgentTeamMemberState, AgentTeamMessage, AgentTeamSnapshot};
    use yode_core::permission::{PermissionManager, PermissionMode as CorePermissionMode};
    use yode_llm::provider::LlmProvider;
    use yode_tools::builtin::team_runtime::{
        append_agent_team_message, persist_agent_team_runtime,
    };

    struct MockProvider;

    #[async_trait::async_trait]
    impl LlmProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }

        async fn chat(
            &self,
            _request: yode_llm::types::ChatRequest,
        ) -> anyhow::Result<yode_llm::types::ChatResponse> {
            unimplemented!("not used by command tests")
        }

        async fn chat_stream(
            &self,
            _request: yode_llm::types::ChatRequest,
            _tx: tokio::sync::mpsc::Sender<yode_llm::types::StreamEvent>,
        ) -> anyhow::Result<()> {
            unimplemented!("not used by command tests")
        }

        async fn list_models(&self) -> anyhow::Result<Vec<yode_llm::ModelInfo>> {
            Ok(vec![])
        }
    }

    #[test]
    fn recent_message_lines_fall_back_to_none() {
        assert_eq!(
            recent_message_lines(&AgentTeamSnapshot::default()),
            vec!["none"]
        );
    }

    #[test]
    fn recent_message_lines_show_latest_first() {
        let snapshot = AgentTeamSnapshot {
            state: None,
            messages: vec![
                AgentTeamMessage {
                    at: "2026-01-01 00:00:00".to_string(),
                    target: "a".to_string(),
                    kind: "message".to_string(),
                    message: "first".to_string(),
                },
                AgentTeamMessage {
                    at: "2026-01-01 00:00:01".to_string(),
                    target: "b".to_string(),
                    kind: "handoff".to_string(),
                    message: "second".to_string(),
                },
            ],
        };
        let lines = recent_message_lines(&snapshot);
        assert!(lines[0].contains("second"));
    }

    #[test]
    fn disk_team_ids_pick_up_state_files() {
        let dir = std::env::temp_dir().join(format!("yode-teams-{}", uuid::Uuid::new_v4()));
        let teams = dir.join(".yode").join("teams");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&teams).unwrap();
        std::fs::write(teams.join("team-a-agent-team-state.json"), "{}").unwrap();
        std::fs::write(teams.join("team-b-agent-team-state.json"), "{}").unwrap();
        let ids = team_ids_from_disk(&dir);
        assert_eq!(ids, vec!["team-a".to_string(), "team-b".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn teams_command_executes_list_latest_monitor_and_messages() {
        let dir = std::env::temp_dir().join(format!("yode-teams-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        persist_agent_team_runtime(
            &dir,
            "ship feature",
            Some("team-demo"),
            "manual",
            vec![AgentTeamMemberState {
                member_id: "review".to_string(),
                description: "review".to_string(),
                subagent_type: None,
                model: None,
                run_in_background: true,
                allowed_tools: vec![],
                permission_inheritance: "parent_tool_pool".to_string(),
                status: "running".to_string(),
                runtime_task_id: Some("task-1".to_string()),
                last_result_preview: None,
                result_artifact_path: None,
                last_updated_at: Some("2026-01-01 00:00:00".to_string()),
                pending_message_count: 0,
                last_message_at: None,
            }],
        )
        .unwrap();
        append_agent_team_message(
            &dir,
            "team-demo",
            "review",
            "handoff",
            "focus on tests",
        )
        .unwrap();

        assert!(run_teams_command(&dir, "list").contains("Agent teams (1)"));
        assert!(run_teams_command(&dir, "latest").contains("Team workspace"));
        assert!(run_teams_command(&dir, "monitor latest").contains("Agent Team Monitor"));
        assert!(run_teams_command(&dir, "messages latest").contains("focus on tests"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn run_teams_command(dir: &std::path::Path, args: &str) -> String {
        let provider: std::sync::Arc<dyn LlmProvider> = std::sync::Arc::new(MockProvider);
        let tools = std::sync::Arc::new(yode_tools::registry::ToolRegistry::new());
        let context = yode_core::context::AgentContext::new(
            dir.to_path_buf(),
            "mock".to_string(),
            "mock-model".to_string(),
        );
        let engine = std::sync::Arc::new(tokio::sync::Mutex::new(
            yode_core::engine::AgentEngine::new(
                provider,
                std::sync::Arc::clone(&tools),
                PermissionManager::new(CorePermissionMode::Default),
                context,
            ),
        ));
        let provider_registry = std::sync::Arc::new(yode_llm::registry::ProviderRegistry::new());
        let mut provider_name = "mock".to_string();
        let mut provider_models = vec!["mock-model".to_string()];
        let all_provider_models = std::collections::HashMap::new();
        let mut chat_entries = Vec::new();
        let mut printed_count = 0usize;
        let mut streaming_buf = String::new();
        let mut streaming_markdown_stable_len = 0usize;
        let mut streaming_markdown_cached_buf_len = 0usize;
        let mut streaming_markdown_cached_width = 0usize;
        let mut streaming_markdown_preview_source = String::new();
        let mut streaming_markdown_preview = Vec::new();
        let mut streaming_markdown_remainder = None;
        let mut session = crate::app::SessionState {
            model: "mock-model".to_string(),
            session_id: "session-1".to_string(),
            working_dir: dir.display().to_string(),
            startup_profile: None,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            previous_prompt_tokens: 0,
            tool_call_count: 0,
            permission_mode: crate::app::PermissionMode::Normal,
            always_allow_tools: Vec::new(),
            input_estimated: false,
            turn_input_tokens: 0,
            turn_output_tokens: 0,
            resume_cache_warmup: None,
        };
        let mut input = crate::app::input::InputState::new();
        let terminal_caps = crate::terminal_caps::TerminalCaps::detect();
        let input_history = Vec::<String>::new();
        let mut should_quit = false;
        let cmd_registry = crate::commands::registry::CommandRegistry::new();
        let (engine_event_tx, _engine_event_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut ctx = crate::commands::context::CommandContext {
            engine,
            provider_registry: &provider_registry,
            provider_name: &mut provider_name,
            provider_models: &mut provider_models,
            all_provider_models: &all_provider_models,
            chat_entries: &mut chat_entries,
            printed_count: &mut printed_count,
            streaming_buf: &mut streaming_buf,
            streaming_markdown_stable_len: &mut streaming_markdown_stable_len,
            streaming_markdown_cached_buf_len: &mut streaming_markdown_cached_buf_len,
            streaming_markdown_cached_width: &mut streaming_markdown_cached_width,
            streaming_markdown_preview_source: &mut streaming_markdown_preview_source,
            streaming_markdown_preview: &mut streaming_markdown_preview,
            streaming_markdown_remainder: &mut streaming_markdown_remainder,
            tools: &tools,
            session: &mut session,
            input: &mut input,
            terminal_caps: &terminal_caps,
            input_history: &input_history,
            should_quit: &mut should_quit,
            session_start: std::time::Instant::now(),
            turn_started_at: None,
            cmd_registry: &cmd_registry,
            engine_event_tx: &engine_event_tx,
        };
        match TeamsCommand::new().execute(args, &mut ctx).unwrap() {
            CommandOutput::Message(message) => message,
            _ => panic!("expected message output"),
        }
    }
}
