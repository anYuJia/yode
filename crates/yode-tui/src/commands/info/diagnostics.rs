use super::diagnostics_render::{
    diagnostics_inspector_actions, render_diagnostics_overview_with_width,
};
use crate::commands::artifact_nav::attach_inspector_actions;
use crate::commands::context::CommandContext;
use crate::commands::inspector_bridge::document_from_command_output;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct DiagnosticsCommand {
    meta: CommandMeta,
}

impl DiagnosticsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "diagnostics",
                description: "Show a unified diagnostics overview",
                aliases: &["diag"],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for DiagnosticsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| (engine.runtime_state(), engine.runtime_tasks_snapshot()));
        let Some((state, tasks)) = runtime else {
            return Ok(CommandOutput::Message(
                "Diagnostics unavailable: engine busy.".to_string(),
            ));
        };

        let project_root = std::path::Path::new(&ctx.session.working_dir);
        let body = render_diagnostics_overview_with_width(
            project_root,
            &state,
            &tasks,
            diagnostics_terminal_width(),
        );
        let mut document = document_from_command_output(
            "Diagnostics inspector",
            body.lines().map(str::to_string).collect(),
        );
        attach_inspector_actions(
            &mut document,
            diagnostics_inspector_actions(project_root, &state, &tasks),
        );
        Ok(CommandOutput::OpenInspector(document))
    }
}

fn diagnostics_terminal_width() -> usize {
    crossterm::terminal::size()
        .ok()
        .map(|(width, _)| width as usize)
        .unwrap_or(96)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use yode_core::permission::{PermissionManager, PermissionMode as CorePermissionMode};
    use yode_llm::provider::LlmProvider;

    use super::*;
    use crate::ui::inspector::{InspectorActionKind, InspectorActionTarget};

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
    fn diagnostics_command_opens_inspector_with_default_status_action() {
        let dir =
            std::env::temp_dir().join(format!("yode-diagnostics-command-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let output = run_diagnostics_command(&dir);
        let CommandOutput::OpenInspector(doc) = output else {
            panic!("expected diagnostics to open an inspector");
        };
        assert_eq!(doc.state.title, "Diagnostics inspector");
        let action = doc.panels[0]
            .actions
            .iter()
            .find(|action| action.command == "/status")
            .expect("expected status fallback command");
        assert_eq!(action.label, "Inspect status");
        assert!(action.typed.as_ref().is_some_and(|typed| {
            typed.kind == InspectorActionKind::OpenInspectorTarget
                && typed.target == InspectorActionTarget::InspectorTarget("status".to_string())
        }));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn diagnostics_command_types_artifact_actions_and_keeps_command_fallback() {
        let dir =
            std::env::temp_dir().join(format!("yode-diagnostics-command-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        let plugin_dir = dir.join(".yode").join("plugins").join("broken");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(plugin_dir.join("plugin.toml"), "name =").unwrap();

        let output = run_diagnostics_command(&dir);
        let CommandOutput::OpenInspector(doc) = output else {
            panic!("expected diagnostics to open an inspector");
        };
        let actions = &doc.panels[0].actions;
        assert!(actions.iter().any(|action| {
            action.label == "Inspect plugins"
                && action.command == "/plugin list"
                && action.typed.as_ref().is_some_and(|typed| {
                    typed.kind == InspectorActionKind::OpenInspectorTarget
                        && typed.target
                            == InspectorActionTarget::InspectorTarget("plugin list".to_string())
                })
        }));
        let artifact_action = actions
            .iter()
            .find(|action| action.command == "/inspect artifact .yode/plugins/broken/plugin.toml")
            .expect("expected plugin diagnostic artifact action");
        assert_eq!(artifact_action.label, "Open artifact");
        assert!(artifact_action.typed.as_ref().is_some_and(|typed| {
            typed.kind == InspectorActionKind::OpenArtifact
                && typed.target
                    == InspectorActionTarget::Artifact(
                        ".yode/plugins/broken/plugin.toml".to_string(),
                    )
        }));

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn run_diagnostics_command(dir: &std::path::Path) -> CommandOutput {
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider);
        let tools = Arc::new(yode_tools::registry::ToolRegistry::new());
        let context = yode_core::context::AgentContext::new(
            dir.to_path_buf(),
            "mock".to_string(),
            "mock-model".to_string(),
        );
        let engine = Arc::new(tokio::sync::Mutex::new(
            yode_core::engine::AgentEngine::new(
                provider,
                Arc::clone(&tools),
                PermissionManager::new(CorePermissionMode::Default),
                context,
            ),
        ));
        let provider_registry = Arc::new(yode_llm::registry::ProviderRegistry::new());
        let mut provider_name = "mock".to_string();
        let mut provider_models = vec!["mock-model".to_string()];
        let all_provider_models = HashMap::<String, Vec<String>>::new();
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
        let mut ctx = CommandContext {
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
        DiagnosticsCommand::new().execute("", &mut ctx).unwrap()
    }
}
