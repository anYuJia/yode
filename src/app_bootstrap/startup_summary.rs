use crate::app_bootstrap::tooling::ToolingSetupMetrics;

pub(crate) fn tooling_phase_summary(tooling: &ToolingSetupMetrics) -> String {
    format!(
        "tooling[builtin={}ms mcp_connect={}ms mcp_register={}ms skills={}ms total={}ms]",
        tooling.builtin_register_ms,
        tooling.mcp_connect_ms,
        tooling.mcp_register_ms,
        tooling.skill_discovery_ms,
        tooling.total_ms
    )
}
