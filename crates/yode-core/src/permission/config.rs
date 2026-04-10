use serde::{Deserialize, Serialize};

use super::{PermissionRule, RuleBehavior, RuleSource};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionConfig {
    #[serde(default)]
    pub default_mode: Option<String>,
    #[serde(default)]
    pub always_allow: Vec<PermissionRuleConfig>,
    #[serde(default)]
    pub always_deny: Vec<PermissionRuleConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleConfig {
    pub tool: String,
    #[serde(default)]
    pub pattern: Option<String>,
}

impl PermissionConfig {
    /// Convert to permission rules with the given source.
    pub fn to_rules(&self, source: RuleSource) -> Vec<PermissionRule> {
        let mut rules = Vec::new();
        for r in &self.always_allow {
            rules.push(PermissionRule {
                source,
                behavior: RuleBehavior::Allow,
                tool_name: r.tool.clone(),
                pattern: r.pattern.clone(),
            });
        }
        for r in &self.always_deny {
            rules.push(PermissionRule {
                source,
                behavior: RuleBehavior::Deny,
                tool_name: r.tool.clone(),
                pattern: r.pattern.clone(),
            });
        }
        rules
    }
}
