use super::blocks::RestoreBlockKind;
use crate::engine::types::{RestoreBudgetEntryRuntimeState, RestoreBudgetRuntimeState};

pub(super) const POST_COMPACT_RESTORE_TOTAL_BUDGET_TOKENS: u32 = 5_000;

pub(super) struct RestoreBudget {
    total_tokens: u32,
    used_tokens: u32,
    entries: Vec<RestoreBudgetEntryRuntimeState>,
}

impl RestoreBudget {
    pub(super) fn new(total_tokens: u32) -> Self {
        Self {
            total_tokens,
            used_tokens: 0,
            entries: Vec::new(),
        }
    }

    pub(super) fn apply(&mut self, kind: RestoreBlockKind, content: String) -> String {
        let block_cap = restore_block_cap_tokens(kind);
        let remaining = self.total_tokens.saturating_sub(self.used_tokens);
        let cap = block_cap.min(remaining);
        let original_tokens = estimate_restore_text_tokens(&content);
        let (content, truncated, reason) = if original_tokens > cap {
            let reason = if remaining == 0 {
                "shared restore budget exhausted"
            } else if block_cap <= remaining {
                "per-block restore budget cap"
            } else {
                "shared restore budget remaining cap"
            };
            (
                truncate_restore_block(kind, &content, cap),
                true,
                Some(reason.to_string()),
            )
        } else {
            (content, false, None)
        };
        let used_tokens = estimate_restore_text_tokens(&content).min(cap);
        self.used_tokens = self.used_tokens.saturating_add(used_tokens);
        self.entries.push(RestoreBudgetEntryRuntimeState {
            kind: kind.label().to_string(),
            used_tokens,
            cap_tokens: cap,
            truncated,
            reason,
        });
        content
    }

    pub(super) fn into_runtime(self) -> RestoreBudgetRuntimeState {
        RestoreBudgetRuntimeState {
            total_tokens: self.total_tokens,
            used_tokens: self.used_tokens,
            entries: self.entries,
        }
    }
}

pub(super) fn estimate_restore_text_tokens(text: &str) -> u32 {
    text.chars().count().div_ceil(4).max(1) as u32
}

pub(super) fn restore_block_cap_tokens(kind: RestoreBlockKind) -> u32 {
    match kind {
        RestoreBlockKind::Runtime => 600,
        RestoreBlockKind::Files => 1_400,
        RestoreBlockKind::Plan => 400,
        RestoreBlockKind::Tasks => 700,
        RestoreBlockKind::Tools => 500,
        RestoreBlockKind::PromptCache => 500,
        RestoreBlockKind::Skills => 500,
        RestoreBlockKind::Mcp => 300,
        RestoreBlockKind::Artifacts => 300,
    }
}

pub(super) fn truncate_restore_block(
    kind: RestoreBlockKind,
    content: &str,
    cap_tokens: u32,
) -> String {
    let recovery = restore_recovery_instruction(kind);
    let marker = format!(
        "\n- Restore budget: truncated by {} token cap. {}",
        cap_tokens, recovery
    );
    if cap_tokens == 0 {
        return format!(
            "[Post-compact restore: {}]{}",
            kind.label(),
            marker.trim_start()
        );
    }
    let marker_chars = marker.chars().count();
    let max_chars = (cap_tokens as usize)
        .saturating_mul(4)
        .saturating_sub(marker_chars)
        .max(80);
    let mut truncated = content.chars().take(max_chars).collect::<String>();
    truncated.push_str(&marker);
    truncated
}

pub(super) fn restore_recovery_instruction(kind: RestoreBlockKind) -> &'static str {
    match kind {
        RestoreBlockKind::Files => {
            "Re-read the named files with focused read_file calls for exact content."
        }
        RestoreBlockKind::Skills => {
            "Run /skills active or rediscover the referenced SKILL.md files for full guidance."
        }
        RestoreBlockKind::Artifacts => {
            "Open the listed .yode/status or transcript artifacts for the full details."
        }
        RestoreBlockKind::Plan => "Run /plan status or inspect the plan file for full state.",
        RestoreBlockKind::Tasks => "Run /tasks latest or task_output with the task id for current output.",
        RestoreBlockKind::Mcp => "Run /mcp status or list resources again for full MCP state.",
        RestoreBlockKind::PromptCache => {
            "Use /context for current cache diagnostics; the next request will rederive cache state."
        }
        RestoreBlockKind::Tools => "Run /tools status for the full active tool inventory.",
        RestoreBlockKind::Runtime => "Use /status and /context for the full runtime state.",
    }
}

pub(super) fn apply_restore_budget(
    blocks: Vec<(RestoreBlockKind, String)>,
) -> (Vec<(RestoreBlockKind, String)>, RestoreBudgetRuntimeState) {
    let mut budget = RestoreBudget::new(POST_COMPACT_RESTORE_TOTAL_BUDGET_TOKENS);
    let blocks = blocks
        .into_iter()
        .map(|(kind, content)| (kind, budget.apply(kind, content)))
        .collect::<Vec<_>>();
    (blocks, budget.into_runtime())
}

pub(super) fn render_restore_budget_table(budget: &RestoreBudgetRuntimeState) -> String {
    let mut lines = vec![
        "## Restore Budget".to_string(),
        String::new(),
        format!(
            "- Total: {}/{} tokens",
            budget.used_tokens, budget.total_tokens
        ),
        String::new(),
        "| Block | Used | Cap | Truncated | Reason |".to_string(),
        "| --- | ---: | ---: | --- | --- |".to_string(),
    ];
    for entry in &budget.entries {
        lines.push(format!(
            "| {} | {} | {} | {} | {} |",
            entry.kind,
            entry.used_tokens,
            entry.cap_tokens,
            if entry.truncated { "yes" } else { "no" },
            entry.reason.as_deref().unwrap_or("none")
        ));
    }
    lines.join("\n")
}
