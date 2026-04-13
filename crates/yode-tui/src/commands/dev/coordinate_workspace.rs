pub(crate) fn coordinator_dry_run_prompt(goal: &str) -> String {
    format!(
        "Use `coordinate_agents` for goal=\"{}\" with dry_run=true first. Show workstreams, dependencies, and the suggested execution order before starting real work.",
        goal
    )
}

#[cfg(test)]
mod tests {
    use super::coordinator_dry_run_prompt;

    #[test]
    fn coordinator_prompt_uses_dry_run() {
        assert!(coordinator_dry_run_prompt("demo").contains("dry_run=true"));
    }
}
