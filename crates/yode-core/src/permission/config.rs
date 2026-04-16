use serde::{Deserialize, Serialize};

use super::{PermissionRule, RuleBehavior, RuleSource};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionConfig {
    #[serde(default)]
    pub default_mode: Option<String>,
    #[serde(default)]
    pub always_allow: Vec<PermissionRuleConfig>,
    #[serde(default)]
    pub always_ask: Vec<PermissionRuleConfig>,
    #[serde(default)]
    pub always_deny: Vec<PermissionRuleConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRuleConfig {
    #[serde(default)]
    pub tool: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
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
                category: r.category.clone(),
                pattern: r.pattern.clone(),
                description: r.description.clone(),
            });
        }
        for r in &self.always_ask {
            rules.push(PermissionRule {
                source,
                behavior: RuleBehavior::Ask,
                tool_name: r.tool.clone(),
                category: r.category.clone(),
                pattern: r.pattern.clone(),
                description: r.description.clone(),
            });
        }
        for r in &self.always_deny {
            rules.push(PermissionRule {
                source,
                behavior: RuleBehavior::Deny,
                tool_name: r.tool.clone(),
                category: r.category.clone(),
                pattern: r.pattern.clone(),
                description: r.description.clone(),
            });
        }
        rules
    }
}
