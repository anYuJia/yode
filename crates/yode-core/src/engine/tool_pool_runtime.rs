use super::*;
use yode_tools::registry::{
    ToolOrigin, ToolPermissionState, ToolPoolEntry, ToolPoolPhase, ToolPoolSnapshot,
};

impl AgentEngine {
    pub(super) fn build_tool_pool_snapshot(&self) -> ToolPoolSnapshot {
        let inventory = self.tools.inventory();
        let runtime_plan_mode_enabled =
            self.plan_mode.try_lock().map(|mode| *mode).unwrap_or(false);
        let mut entries = self
            .tools
            .list()
            .into_iter()
            .map(|tool| {
                self.build_tool_pool_entry(
                    tool.name(),
                    ToolPoolPhase::Active,
                    tool.capabilities().read_only,
                    runtime_plan_mode_enabled,
                )
            })
            .collect::<Vec<_>>();
        entries.extend(self.tools.list_deferred().into_iter().map(|(name, tool)| {
            self.build_tool_pool_entry(
                &name,
                ToolPoolPhase::Deferred,
                tool.capabilities().read_only,
                runtime_plan_mode_enabled,
            )
        }));
        entries.sort_by(|left, right| {
            left.phase
                .cmp(&right.phase)
                .then(left.origin.cmp(&right.origin))
                .then(left.name.cmp(&right.name))
        });

        ToolPoolSnapshot {
            permission_mode: if runtime_plan_mode_enabled {
                format!("{}+runtime-plan", self.permissions.mode())
            } else {
                self.permissions.mode().to_string()
            },
            tool_search_enabled: inventory.tool_search_enabled,
            tool_search_reason: inventory.tool_search_reason,
            entries,
        }
    }

    fn build_tool_pool_entry(
        &self,
        name: &str,
        phase: ToolPoolPhase,
        read_only: bool,
        runtime_plan_mode_enabled: bool,
    ) -> ToolPoolEntry {
        let explanation = self.permissions.explain_with_content(name, None);
        let mut permission = match explanation.action {
            PermissionAction::Allow => ToolPermissionState::Allow,
            PermissionAction::Confirm => ToolPermissionState::Confirm,
            PermissionAction::Deny => ToolPermissionState::Deny,
        };
        let mut reason = explanation.reason;
        let matched_rule = explanation.matched_rule;

        if name == "tool_search" && !self.tools.inventory().tool_search_enabled {
            permission = ToolPermissionState::Deny;
            reason = "Tool search is disabled because the current tool inventory does not need deferred loading.".to_string();
        }
        if runtime_plan_mode_enabled && name != "exit_plan_mode" && !read_only {
            permission = ToolPermissionState::Deny;
            reason =
                "Runtime plan mode hides mutating tools based on tool annotations.".to_string();
        }

        ToolPoolEntry {
            name: name.to_string(),
            phase,
            origin: if name.starts_with("mcp__") {
                ToolOrigin::Mcp
            } else {
                ToolOrigin::Builtin
            },
            permission,
            visible_to_model: permission.visible_to_model(),
            reason,
            matched_rule,
        }
    }
}
