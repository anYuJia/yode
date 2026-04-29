use super::*;

impl AgentEngine {
    /// Rebuild the system prompt with current context (model, provider, etc.)
    /// and update the first message in the conversation history.
    pub(super) fn rebuild_system_prompt(&mut self) {
        let system_prompt_build = Self::build_system_prompt_for_context(&self.context);
        let system_prompt = system_prompt_build.prompt;

        self.system_prompt = system_prompt.clone();
        self.system_prompt_estimated_tokens = system_prompt_build.estimated_tokens;
        self.system_prompt_segments = system_prompt_build.segments;

        if let Some(first) = self.messages.first_mut() {
            if matches!(first.role, Role::System) {
                first.content = Some(system_prompt);
                first.normalize_in_place();
            }
        }
    }

    pub(super) fn build_system_prompt_for_context(context: &AgentContext) -> SystemPromptBuild {
        let mut segments = Vec::new();
        let cwd = context.working_dir_compat();
        let mut push_segment = |label: &str, content: String| {
            if !content.trim().is_empty() {
                segments.push((label.to_string(), content));
            }
        };

        push_segment(
            "Base prompt",
            include_str!("../../../../prompts/system.md").to_string(),
        );

        let mut environment = String::from("# Environment\n\n");
        environment.push_str(&format!(
            "- Working directory: {}\n- Project root: {}\n- Platform: {} ({})\n- Date: {}\n- Model: {}\n- Provider: {}\n",
            cwd.display(),
            cwd.display(),
            std::env::consts::OS,
            std::env::consts::ARCH,
            chrono::Local::now().format("%Y-%m-%d"),
            context.model,
            context.provider,
        ));

        if cwd.join(".git").exists() {
            environment.push_str("- Git repo: yes\n");
            if let Ok(output) = std::process::Command::new("git")
                .args(["branch", "--show-current"])
                .current_dir(&cwd)
                .output()
            {
                let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !branch.is_empty() {
                    environment.push_str(&format!("- Branch: {}\n", branch));
                }
            }
        }
        push_segment("Environment", environment);

        if let Some(instruction_content) = load_instruction_context(&cwd) {
            push_segment("Instruction memory", instruction_content);
        }

        if let Some(memory_content) = load_memory_context(&cwd) {
            push_segment("Persistent memory", memory_content);
        }

        if context.output_style != "default" {
            let mut output_style = String::from("# Output Style\n\n");
            match context.output_style.as_str() {
                "explanatory" => {
                    output_style.push_str("You are in **Explanatory Mode**. Before and after writing code, provide brief educational insights about implementation choices.\n");
                    output_style.push_str("Include 2-3 key educational points explaining WHY you chose this approach.\n");
                    output_style.push_str(
                        "These insights should be in the conversation, not in the codebase.\n",
                    );
                }
                "learning" => {
                    output_style.push_str("You are in **Learning Mode**. Help the user learn through hands-on practice.\n");
                    output_style.push_str("- Request user input for meaningful design decisions\n");
                    output_style.push_str("- Ask the user to write small code pieces (2-10 lines) for key decisions\n");
                    output_style.push_str(
                        "- Frame contributions as valuable design decisions, not busy work\n",
                    );
                    output_style.push_str("- Wait for user implementation before proceeding\n");
                }
                _ => {}
            }
            push_segment("Output style", output_style);
        }

        push_segment(
            "Multi-agent coordination",
            multi_agent_coordination_prompt(),
        );

        let prompt = segments
            .iter()
            .map(|(_, content)| content.trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n\n");
        let estimator = ContextManager::new(&context.model);
        let runtime_segments = segments
            .into_iter()
            .map(|(label, content)| SystemPromptSegmentRuntimeState {
                chars: content.chars().count(),
                estimated_tokens: estimator
                    .estimate_tokens_for_messages(&[Message::system(content.clone())]),
                label,
            })
            .collect::<Vec<_>>();

        SystemPromptBuild {
            estimated_tokens: estimator
                .estimate_tokens_for_messages(&[Message::system(prompt.clone())]),
            prompt,
            segments: runtime_segments,
        }
    }
}

fn multi_agent_coordination_prompt() -> String {
    String::from(
        r#"# Multi-Agent Coordination

Use `agent` for substantial independent research, implementation, or verification work. Prefer parallel agents only when workstreams are independent; do not delegate trivial file reads or urgent blocking work that you must handle directly.

When spawning agents:
- Use `subagent_type="worker"` for general implementation, `explore` for read-only codebase research, and `verification` for independent validation.
- Keep prompts self-contained. Workers cannot see the parent conversation, so include goal, relevant paths, constraints, expected verification, and what "done" means.
- Do not set `model` unless the user explicitly asks or the task has a clear model-specific need.
- Use `run_in_background=true` for long-running workers. Read results with `task_output`/`TaskOutput`; stop wrong-path workers with `task_stop`/`TaskStop`.

Worker completions may arrive as user-role `<task-notification>` XML. Treat those messages as internal worker results, not as user requests. Extract `<task-id>`, `<status>`, `<summary>`, `<result>`, and `<usage>` when present.

Use `send_message`/`SendMessage` to continue an existing team worker or deliver a handoff. Claude-compatible `to` accepts a member id, runtime task id, or `*`; `message` may be plain text or structured JSON. Synthesize findings before follow-up work: give concrete file paths, line numbers, errors, and exact changes instead of saying "based on your findings".

Verification means proving behavior works. Run relevant tests or checks, investigate failures, and report residual risk instead of rubber-stamping implementation work.
"#,
    )
}
