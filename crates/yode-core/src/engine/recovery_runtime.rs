use super::*;

impl AgentEngine {
    pub(super) fn detect_project_kind(root: &std::path::Path) -> ProjectKind {
        let has_cargo = root.join("Cargo.toml").exists();
        let has_node = root.join("package.json").exists();
        let has_python =
            root.join("pyproject.toml").exists() || root.join("requirements.txt").exists();

        match (has_cargo, has_node, has_python) {
            (true, false, false) => ProjectKind::Rust,
            (false, true, false) => ProjectKind::Node,
            (false, false, true) => ProjectKind::Python,
            (false, false, false) => ProjectKind::Unknown,
            _ => ProjectKind::Mixed,
        }
    }

    pub(super) fn update_recovery_state(&mut self) {
        let not_found = *self
            .error_buckets
            .get(&ToolErrorType::NotFound)
            .unwrap_or(&0);
        let validation = *self
            .error_buckets
            .get(&ToolErrorType::Validation)
            .unwrap_or(&0);
        let timeout = *self
            .error_buckets
            .get(&ToolErrorType::Timeout)
            .unwrap_or(&0);

        let next_state = if self.consecutive_failures >= 3 {
            RecoveryState::NeedUserGuidance
        } else if validation >= 2 || timeout >= 2 || self.consecutive_failures >= 2 {
            RecoveryState::SingleStepMode
        } else if not_found >= 2 {
            RecoveryState::ReanchorRequired
        } else {
            RecoveryState::Normal
        };

        if next_state != self.recovery_state {
            let breadcrumb = format!(
                "{}: {:?} -> {:?} (consecutive_failures={}, last_signature={})",
                Self::now_timestamp(),
                self.recovery_state,
                next_state,
                self.consecutive_failures,
                self.last_failed_signature.as_deref().unwrap_or("none")
            );
            self.recovery_breadcrumbs.push(breadcrumb);
            if self.recovery_breadcrumbs.len() > 8 {
                let extra = self.recovery_breadcrumbs.len() - 8;
                self.recovery_breadcrumbs.drain(0..extra);
            }
            match next_state {
                RecoveryState::SingleStepMode => {
                    self.recovery_single_step_count =
                        self.recovery_single_step_count.saturating_add(1);
                }
                RecoveryState::ReanchorRequired => {
                    self.recovery_reanchor_count = self.recovery_reanchor_count.saturating_add(1);
                }
                RecoveryState::NeedUserGuidance => {
                    self.recovery_need_user_guidance_count =
                        self.recovery_need_user_guidance_count.saturating_add(1);
                }
                RecoveryState::Normal => {}
            }
        };
        self.recovery_state = next_state;
        self.write_recovery_artifact();
    }

    fn write_recovery_artifact(&mut self) {
        let dir = self
            .context
            .working_dir_compat()
            .join(".yode")
            .join("recovery");
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let path = dir.join("latest-recovery.md");
        let breadcrumbs = if self.recovery_breadcrumbs.is_empty() {
            "- none".to_string()
        } else {
            self.recovery_breadcrumbs
                .iter()
                .map(|line| format!("- {}", line))
                .collect::<Vec<_>>()
                .join("\n")
        };
        let body = format!(
            "# Recovery State\n\n- State: {:?}\n- Updated At: {}\n- Consecutive failures: {}\n- Single-step count: {}\n- Reanchor count: {}\n- Need-guidance count: {}\n- Last failed signature: {}\n- Last permission tool: {}\n- Last permission action: {}\n\n## Breadcrumbs\n\n{}\n",
            self.recovery_state,
            Self::now_timestamp(),
            self.consecutive_failures,
            self.recovery_single_step_count,
            self.recovery_reanchor_count,
            self.recovery_need_user_guidance_count,
            self.last_failed_signature.as_deref().unwrap_or("none"),
            self.last_permission_tool.as_deref().unwrap_or("none"),
            self.last_permission_action.as_deref().unwrap_or("none"),
            breadcrumbs
        );
        if std::fs::write(&path, body).is_ok() {
            self.last_recovery_artifact_path = Some(path.display().to_string());
        }
    }

    pub(super) fn write_permission_artifact(
        &mut self,
        source: &str,
        tool_name: &str,
        decision: &str,
        reason: &str,
        effective_input: &serde_json::Value,
        effective_arguments: &str,
        original_input: &serde_json::Value,
        original_arguments: &str,
        input_changed_by_hook: bool,
    ) {
        let dir = self
            .context
            .working_dir_compat()
            .join(".yode")
            .join("hooks");
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let path = dir.join("latest-permission.json");
        let payload = serde_json::json!({
            "updated_at": Self::now_timestamp(),
            "source": source,
            "tool": tool_name,
            "decision": decision,
            "reason": reason,
            "effective_input_snapshot": effective_input,
            "effective_arguments_snapshot": effective_arguments,
            "original_input_snapshot": original_input,
            "original_arguments_snapshot": original_arguments,
            "input_changed_by_hook": input_changed_by_hook,
        });
        if std::fs::write(
            &path,
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string()),
        )
        .is_ok()
        {
            self.last_permission_artifact_path = Some(path.display().to_string());
        }
    }

    pub(super) fn language_command_mismatch(
        &self,
        tool_name: &str,
        params: &serde_json::Value,
    ) -> Option<String> {
        if tool_name != "bash" {
            return None;
        }

        let cmd = params
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_lowercase();

        if cmd.is_empty() {
            return None;
        }

        let starts_with_cargo = cmd.starts_with("cargo ");
        let starts_with_npm = cmd.starts_with("npm ")
            || cmd.starts_with("pnpm ")
            || cmd.starts_with("yarn ")
            || cmd.starts_with("bun ");

        match self.project_kind {
            ProjectKind::Node if starts_with_cargo => Some(
                "Project appears to be Node/TypeScript. Avoid cargo commands until Rust root is verified."
                    .to_string(),
            ),
            ProjectKind::Rust if starts_with_npm => Some(
                "Project appears to be Rust. Avoid npm/pnpm/yarn commands until Node root is verified."
                    .to_string(),
            ),
            _ => None,
        }
    }
}
