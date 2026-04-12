use yode_tools::registry::ToolInventory;

use super::helpers::ReviewSummary;

#[derive(Debug, Clone, Default)]
pub(super) struct StatusArtifactLinks {
    pub review_artifact: Option<String>,
    pub startup_profile_artifact: Option<String>,
    pub startup_manifest_artifact: Option<String>,
    pub provider_inventory_artifact: Option<String>,
    pub resume_warmup_artifact: Option<String>,
    pub mcp_failure_artifact: Option<String>,
    pub tool_artifact: Option<String>,
    pub recovery_artifact: Option<String>,
    pub permission_artifact: Option<String>,
    pub transcript_artifact: Option<String>,
    pub runtime_task_artifact: Option<String>,
}

pub(super) fn busy_runtime_sections(always_allow: &str, inventory: &ToolInventory) -> String {
    format!(
        "\n\nCompact:\n  Runtime state:   engine busy\n\nMemory:\n  Runtime state:   engine busy\n\nTools:\n  Inventory:       {} total / {} active / {} deferred\n  MCP tools:       {} active / {} deferred\n  Search mode:     {}\n  Search reason:   {}\n  Activations:     {} (last: {})\n  Duplicate regs:  {} ({})\n  Always-allow:    {}",
        inventory.total_count,
        inventory.active_count,
        inventory.deferred_count,
        inventory.mcp_active_count,
        inventory.mcp_deferred_count,
        inventory.tool_search_enabled,
        inventory.tool_search_reason.as_deref().unwrap_or("none"),
        inventory.activation_count,
        inventory.last_activated_tool.as_deref().unwrap_or("none"),
        inventory.duplicate_registration_count,
        if inventory.duplicate_tool_names.is_empty() {
            "none".to_string()
        } else {
            inventory.duplicate_tool_names.join(" | ")
        },
        always_allow,
    )
}

pub(super) fn reviews_section(latest_review: Option<&ReviewSummary>) -> String {
    format!(
        "\n\nReviews:\n  Latest review:   {}\n  Review status:   {}\n  Review preview:  {}",
        latest_review
            .map(|summary| summary.path.display().to_string())
            .unwrap_or_else(|| "none".to_string()),
        latest_review.map(|summary| summary.status).unwrap_or("none"),
        latest_review
            .map(|summary| summary.preview.as_str())
            .unwrap_or("none"),
    )
}

pub(super) fn artifact_links_section(links: &StatusArtifactLinks) -> String {
    format!(
        "\n\nArtifacts:\n  Review:          {}\n  Startup profile: {}\n  Startup manifest: {}\n  Provider inv:    {}\n  Resume warmup:   {}\n  MCP failures:    {}\n  Tool:            {}\n  Recovery:        {}\n  Permission:      {}\n  Transcript:      {}\n  Runtime tasks:   {}",
        links.review_artifact.as_deref().unwrap_or("none"),
        links.startup_profile_artifact.as_deref().unwrap_or("none"),
        links.startup_manifest_artifact.as_deref().unwrap_or("none"),
        links.provider_inventory_artifact.as_deref().unwrap_or("none"),
        links.resume_warmup_artifact.as_deref().unwrap_or("none"),
        links.mcp_failure_artifact.as_deref().unwrap_or("none"),
        links.tool_artifact.as_deref().unwrap_or("none"),
        links.recovery_artifact.as_deref().unwrap_or("none"),
        links.permission_artifact.as_deref().unwrap_or("none"),
        links.transcript_artifact.as_deref().unwrap_or("none"),
        links.runtime_task_artifact.as_deref().unwrap_or("none"),
    )
}
