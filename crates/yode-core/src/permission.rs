use std::collections::HashSet;

/// Permission system for tool execution.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionAction {
    /// Tool is allowed to run without confirmation
    Allow,
    /// Tool requires user confirmation
    Confirm,
    /// Tool is denied
    Deny,
}

/// Manages permissions for tools.
#[derive(Debug, Clone)]
pub struct PermissionManager {
    /// Tools that require confirmation
    require_confirmation: HashSet<String>,
}

impl PermissionManager {
    pub fn new(require_confirmation: Vec<String>) -> Self {
        Self {
            require_confirmation: require_confirmation.into_iter().collect(),
        }
    }

    /// Check if a tool needs confirmation.
    pub fn check(&self, tool_name: &str) -> PermissionAction {
        if self.require_confirmation.contains(tool_name) {
            PermissionAction::Confirm
        } else {
            PermissionAction::Allow
        }
    }

    /// Create a permissive manager that allows everything without confirmation.
    pub fn permissive() -> Self {
        Self {
            require_confirmation: HashSet::new(),
        }
    }

    /// Create a strict manager that requires confirmation for everything.
    pub fn strict() -> Self {
        Self {
            require_confirmation: [
                "bash".to_string(),
                "write_file".to_string(),
                "edit_file".to_string(),
            ].into_iter().collect(),
        }
    }

    pub fn allow(&mut self, tool_name: &str) { self.require_confirmation.remove(tool_name); }
    pub fn deny(&mut self, tool_name: &str) { self.require_confirmation.insert(tool_name.to_string()); }
    pub fn reset(&mut self, defaults: Vec<String>) { self.require_confirmation = defaults.into_iter().collect(); }
    pub fn confirmable_tools(&self) -> Vec<&str> {
        let mut tools: Vec<&str> = self.require_confirmation.iter().map(|s| s.as_str()).collect();
        tools.sort();
        tools
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permissive_allows_all() {
        let pm = PermissionManager::permissive();
        assert_eq!(pm.check("bash"), PermissionAction::Allow);
        assert_eq!(pm.check("read_file"), PermissionAction::Allow);
    }

    #[test]
    fn test_strict_confirms_dangerous() {
        let pm = PermissionManager::strict();
        assert_eq!(pm.check("bash"), PermissionAction::Confirm);
        assert_eq!(pm.check("edit_file"), PermissionAction::Confirm);
        assert_eq!(pm.check("write_file"), PermissionAction::Confirm);
        // Read-only tools should still be allowed
        assert_eq!(pm.check("read_file"), PermissionAction::Allow);
        assert_eq!(pm.check("glob"), PermissionAction::Allow);
    }

    #[test]
    fn test_custom_confirmation_list() {
        let pm = PermissionManager::new(vec!["my_tool".into()]);
        assert_eq!(pm.check("my_tool"), PermissionAction::Confirm);
        assert_eq!(pm.check("other"), PermissionAction::Allow);
    }

    #[test]
    fn test_allow_removes_confirmation() {
        let mut pm = PermissionManager::strict();
        assert_eq!(pm.check("bash"), PermissionAction::Confirm);
        pm.allow("bash");
        assert_eq!(pm.check("bash"), PermissionAction::Allow);
    }

    #[test]
    fn test_deny_adds_confirmation() {
        let mut pm = PermissionManager::permissive();
        assert_eq!(pm.check("read_file"), PermissionAction::Allow);
        pm.deny("read_file");
        assert_eq!(pm.check("read_file"), PermissionAction::Confirm);
    }

    #[test]
    fn test_reset_restores_defaults() {
        let mut pm = PermissionManager::permissive();
        pm.deny("bash");
        pm.deny("edit_file");
        pm.reset(vec!["write_file".into()]);
        assert_eq!(pm.check("bash"), PermissionAction::Allow);
        assert_eq!(pm.check("edit_file"), PermissionAction::Allow);
        assert_eq!(pm.check("write_file"), PermissionAction::Confirm);
    }

    #[test]
    fn test_confirmable_tools_sorted() {
        let pm = PermissionManager::new(vec!["zebra".into(), "alpha".into(), "middle".into()]);
        assert_eq!(pm.confirmable_tools(), vec!["alpha", "middle", "zebra"]);
    }
}
