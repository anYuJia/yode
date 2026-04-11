use std::time::Instant;

use tracing::info;

use super::tooling::ToolingSetupMetrics;

#[derive(Debug, Clone)]
pub(crate) struct StartupPhaseTiming {
    pub(crate) label: &'static str,
    pub(crate) duration_ms: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct StartupProfiler {
    started_at: Instant,
    last_checkpoint: Instant,
    phases: Vec<StartupPhaseTiming>,
}

impl StartupProfiler {
    pub(crate) fn new() -> Self {
        let now = Instant::now();
        Self {
            started_at: now,
            last_checkpoint: now,
            phases: Vec::new(),
        }
    }

    pub(crate) fn checkpoint(&mut self, label: &'static str) {
        let now = Instant::now();
        self.phases.push(StartupPhaseTiming {
            label,
            duration_ms: now.duration_since(self.last_checkpoint).as_millis() as u64,
        });
        self.last_checkpoint = now;
    }

    fn total_ms(&self) -> u64 {
        self.started_at.elapsed().as_millis() as u64
    }

    pub(crate) fn summary(&self, mode: &'static str, tooling: &ToolingSetupMetrics) -> String {
        let phases = self
            .phases
            .iter()
            .map(|phase| format!("{}={}ms", phase.label, phase.duration_ms))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "mode={} total={}ms tooling[builtin={}ms mcp_connect={}ms mcp_register={}ms skills={}ms total={}ms counts[builtin={} configured_mcp={} connected_mcp={} mcp_tools={} skills={} final_tools={}]] phases[{}]",
            mode,
            self.total_ms(),
            tooling.builtin_register_ms,
            tooling.mcp_connect_ms,
            tooling.mcp_register_ms,
            tooling.skill_discovery_ms,
            tooling.total_ms,
            tooling.builtin_tool_count,
            tooling.configured_mcp_server_count,
            tooling.connected_mcp_server_count,
            tooling.mcp_tool_count,
            tooling.discovered_skill_count,
            tooling.final_tool_count,
            phases
        )
    }

    pub(crate) fn log_summary(&self, mode: &'static str, tooling: &ToolingSetupMetrics) {
        info!(
            startup_mode = mode,
            total_ms = self.total_ms(),
            builtin_register_ms = tooling.builtin_register_ms,
            mcp_connect_ms = tooling.mcp_connect_ms,
            mcp_register_ms = tooling.mcp_register_ms,
            skill_discovery_ms = tooling.skill_discovery_ms,
            tooling_total_ms = tooling.total_ms,
            builtin_tool_count = tooling.builtin_tool_count,
            configured_mcp_server_count = tooling.configured_mcp_server_count,
            connected_mcp_server_count = tooling.connected_mcp_server_count,
            mcp_tool_count = tooling.mcp_tool_count,
            discovered_skill_count = tooling.discovered_skill_count,
            final_tool_count = tooling.final_tool_count,
            summary = %self.summary(mode, tooling),
            "Startup profile"
        );
    }
}
