use super::*;
use yode_tools::registry::{
    ToolOrigin, ToolPermissionState, ToolPoolEntry, ToolPoolPhase, ToolPoolSnapshot,
};

impl AgentEngine {
    pub(super) fn build_tool_pool_snapshot(&self) -> ToolPoolSnapshot {
        let inventory = self.tools.inventory();
        let mut entries = self
            .tools
            .list()
            .into_iter()
            .map(|tool| self.build_tool_pool_entry(tool.name(), ToolPoolPhase::Active))
            .collect::<Vec<_>>();
        entries.extend(
            self.tools
                .list_deferred()
                .into_iter()
                .map(|(name, _)| self.build_tool_pool_entry(&name, ToolPoolPhase::Deferred)),
        );
        entries.sort_by(|left, right| {
            left.phase
                .cmp(&right.phase)
                .then(left.origin.cmp(&right.origin))
                .then(left.name.cmp(&right.name))
        });

        ToolPoolSnapshot {
            permission_mode: self.permissions.mode().to_string(),
            tool_search_enabled: inventory.tool_search_enabled,
            tool_search_reason: inventory.tool_search_reason,
            entries,
        }
    }

    fn build_tool_pool_entry(&self, name: &str, phase: ToolPoolPhase) -> ToolPoolEntry {
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
