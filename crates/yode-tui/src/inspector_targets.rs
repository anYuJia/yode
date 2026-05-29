pub(crate) fn read_only_inspector_target_from_command(command: &str) -> Option<String> {
    let command = command.trim();
    if let Some(target) = command.strip_prefix("/inspect ").map(str::trim) {
        if target.is_empty() || target.starts_with("artifact") {
            return None;
        }
        let target = target.trim_start_matches('/');
        if target.is_empty() {
            return None;
        }
        return read_only_inspector_target_from_command(&format!("/{target}"));
    }

    let target = command.strip_prefix('/')?;
    read_only_inspector_target(target)
}

pub(crate) fn read_only_inspector_target(target: &str) -> Option<String> {
    let parts = target.split_whitespace().collect::<Vec<_>>();
    match parts.as_slice() {
        ["status"] => Some("status".to_string()),
        ["diagnostics"] => Some("diagnostics".to_string()),
        ["context"] => Some("context".to_string()),
        ["brief"] => Some("brief".to_string()),
        ["files"] => Some("files".to_string()),
        ["cost"] => Some("cost".to_string()),
        ["config"] => Some("config".to_string()),
        ["version"] => Some("version".to_string()),
        ["keys"] | ["keybindings"] => Some("keys".to_string()),
        ["time"] => Some("time".to_string()),
        ["help"] => Some("help".to_string()),
        ["history"] => Some("history".to_string()),
        ["history", "pick"] => Some("history pick".to_string()),
        ["history", count] if count.chars().all(|ch| ch.is_ascii_digit()) => {
            Some(format!("history {}", count))
        }
        ["history", "search", query @ ..]
            if !query.is_empty() && query.iter().all(|token| is_safe_target_token(token)) =>
        {
            Some(format!("history search {}", query.join(" ")))
        }
        ["update", "status"] => Some("update status".to_string()),
        ["tools"] => Some("tools".to_string()),
        ["tools", sub] if matches!(*sub, "diag" | "diagnostics" | "list" | "verbose") => {
            Some(format!("tools {}", sub))
        }
        ["plugin", "list"] => Some("plugin list".to_string()),
        ["plugin", "inspect", name] if is_safe_target_token(name) => {
            Some(format!("plugin inspect {}", name))
        }
        ["skills", "list"] => Some("skills list".to_string()),
        ["skills", "show", name] if is_safe_target_token(name) => {
            Some(format!("skills show {}", name))
        }
        ["doctor"] => Some("doctor".to_string()),
        ["doctor", sub]
            if matches!(
                *sub,
                "remote" | "remote-control" | "remote-review" | "remote-artifacts" | "restore"
            ) =>
        {
            Some(format!("doctor {}", sub))
        }
        ["permissions", sub] if matches!(*sub, "mode" | "sources" | "scopes" | "denials") => {
            Some(format!("permissions {}", sub))
        }
        ["permissions", "mode", "guide"] => Some("permissions mode guide".to_string()),
        ["permissions", "denials", tool] if is_safe_target_token(tool) => {
            Some(format!("permissions denials {}", tool))
        }
        ["memory"] => Some("memory".to_string()),
        ["memory", sub] if matches!(*sub, "live" | "session" | "latest" | "list" | "pick") => {
            Some(format!("memory {}", sub))
        }
        ["memory", "list", filter] if is_safe_target_token(filter) => {
            Some(format!("memory list {}", filter))
        }
        ["memory", "latest", "compare", target] if is_safe_target_token(target) => {
            Some(format!("memory latest compare {}", target))
        }
        ["memory", "compare", left, right]
            if is_safe_target_token(left) && is_safe_target_token(right) =>
        {
            Some(format!("memory compare {} {}", left, right))
        }
        ["tasks"] => Some("tasks".to_string()),
        ["tasks", sub]
            if matches!(
                *sub,
                "monitor"
                    | "summary"
                    | "latest"
                    | "list"
                    | "notifications"
                    | "running"
                    | "failed"
                    | "completed"
                    | "cancelled"
                    | "pending"
                    | "bash"
                    | "agent"
            ) =>
        {
            Some(format!("tasks {}", sub))
        }
        ["tasks", "latest", filter] | ["tasks", "list", filter] if is_safe_target_token(filter) => {
            Some(format!("tasks {} {}", parts[1], filter))
        }
        ["tasks", "read", target] if is_safe_target_token(target) => {
            Some(format!("tasks read {}", target))
        }
        ["teams"] => Some("teams".to_string()),
        ["teams", sub] if matches!(*sub, "list" | "latest" | "monitor" | "messages") => {
            Some(format!("teams {}", sub))
        }
        ["teams", "monitor", selector] | ["teams", "messages", selector]
            if is_safe_target_token(selector) =>
        {
            Some(format!("teams {} {}", parts[1], selector))
        }
        ["teams", selector] if is_safe_target_token(selector) => {
            Some(format!("teams {}", selector))
        }
        ["reviews"] => Some("reviews".to_string()),
        ["reviews", sub] if matches!(*sub, "latest" | "list" | "summary") => {
            Some(format!("reviews {}", sub))
        }
        ["reviews", "latest", kind] | ["reviews", "list", kind] | ["reviews", "summary", kind]
            if is_safe_target_token(kind) =>
        {
            Some(format!("reviews {} {}", parts[1], kind))
        }
        ["workflows"] => Some("workflows".to_string()),
        ["workflows", sub] if matches!(*sub, "latest" | "history") => {
            Some(format!("workflows {}", sub))
        }
        ["workflows", sub, name]
            if matches!(*sub, "show" | "preview") && is_safe_target_token(name) =>
        {
            Some(format!("workflows {} {}", sub, name))
        }
        ["coordinate", sub] if matches!(*sub, "latest" | "summary" | "history") => {
            Some(format!("coordinate {}", sub))
        }
        ["remote-control", sub]
            if matches!(
                *sub,
                "latest" | "queue" | "tasks" | "replay" | "retry-summary"
            ) =>
        {
            Some(format!("remote-control {}", sub))
        }
        ["remote-control", "replay", "latest"] => Some("remote-control replay latest".to_string()),
        ["checkpoint", "list"] => Some("checkpoint list".to_string()),
        ["checkpoint", target] if is_safe_checkpoint_target(target) => {
            Some(format!("checkpoint {}", target))
        }
        ["checkpoint", "diff", left, right]
            if is_safe_checkpoint_target(left) && is_safe_checkpoint_target(right) =>
        {
            Some(format!("checkpoint diff {} {}", left, right))
        }
        ["checkpoint", "branch", "list"] => Some("checkpoint branch list".to_string()),
        ["checkpoint", "branch", target] if is_safe_checkpoint_target(target) => {
            Some(format!("checkpoint branch {}", target))
        }
        ["checkpoint", "branch", "diff", left, right]
            if is_safe_checkpoint_target(left) && is_safe_checkpoint_target(right) =>
        {
            Some(format!("checkpoint branch diff {} {}", left, right))
        }
        ["checkpoint", "rollback", "list"] => Some("checkpoint rollback list".to_string()),
        ["checkpoint", "rollback", target] if is_safe_checkpoint_target(target) => {
            Some(format!("checkpoint rollback {}", target))
        }
        ["checkpoint", "rollback-dry-run", target] if is_safe_checkpoint_target(target) => {
            Some(format!("checkpoint rollback-dry-run {}", target))
        }
        ["checkpoint", "rewind-anchor"] => Some("checkpoint rewind-anchor".to_string()),
        ["checkpoint", "rewind-anchor", "list"] => {
            Some("checkpoint rewind-anchor list".to_string())
        }
        ["checkpoint", "rewind-anchor", target] if is_safe_checkpoint_target(target) => {
            Some(format!("checkpoint rewind-anchor {}", target))
        }
        _ => None,
    }
}

pub(crate) fn is_read_only_inspector_family(target: &str) -> Option<&'static str> {
    let family = target.split_whitespace().next()?;
    match family {
        "history" => Some("history"),
        "update" => Some("update"),
        "remote-control" => Some("remote-control"),
        "doctor" => Some("doctor"),
        "permissions" => Some("permissions"),
        "checkpoint" => Some("checkpoint"),
        "workflows" => Some("workflows"),
        "coordinate" => Some("coordinate"),
        "tasks" => Some("tasks"),
        "teams" => Some("teams"),
        "memory" => Some("memory"),
        "reviews" => Some("reviews"),
        "tools" => Some("tools"),
        "plugin" => Some("plugin"),
        "skills" => Some("skills"),
        "hooks" => Some("hooks"),
        "mcp" => Some("mcp"),
        _ => None,
    }
}

fn is_safe_target_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn is_safe_checkpoint_target(value: &str) -> bool {
    is_safe_target_token(value)
        && !matches!(
            value,
            "save" | "restore" | "restore-dry-run" | "rewind" | "merge" | "merge-dry-run"
        )
}

#[cfg(test)]
mod tests {
    use super::read_only_inspector_target_from_command;

    #[test]
    fn inspect_proxy_uses_same_read_only_boundary() {
        assert_eq!(
            read_only_inspector_target_from_command("/inspect checkpoint latest").as_deref(),
            Some("checkpoint latest")
        );
        assert_eq!(
            read_only_inspector_target_from_command("/inspect /keybindings").as_deref(),
            Some("keys")
        );
        assert_eq!(
            read_only_inspector_target_from_command("/tasks read latest").as_deref(),
            Some("tasks read latest")
        );
        for command in [
            "/inspect artifact latest-runtime-tasks",
            "/inspect workflows run latest",
            "/inspect permissions governance",
            "/inspect doctor bundle",
            "/inspect checkpoint restore latest",
            "/inspect remote-control doctor",
            "/inspect hooks",
        ] {
            assert_eq!(read_only_inspector_target_from_command(command), None);
        }
    }
}
