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
    require_confirmation: Vec<String>,
}

impl PermissionManager {
    pub fn new(require_confirmation: Vec<String>) -> Self {
        Self {
            require_confirmation,
        }
    }

    /// Check if a tool needs confirmation.
    pub fn check(&self, tool_name: &str) -> PermissionAction {
        if self.require_confirmation.contains(&tool_name.to_string()) {
            PermissionAction::Confirm
        } else {
            PermissionAction::Allow
        }
    }

    /// Create a permissive manager that allows everything without confirmation.
    pub fn permissive() -> Self {
        Self {
            require_confirmation: vec![],
        }
    }

    /// Create a strict manager that requires confirmation for everything.
    pub fn strict() -> Self {
        // By default, everything requires confirmation
        Self {
            require_confirmation: vec![
                "bash".to_string(),
                "write_file".to_string(),
                "edit_file".to_string(),
            ],
        }
    }
}
