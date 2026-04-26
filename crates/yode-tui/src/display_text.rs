pub(crate) fn compact_path_tail(path: &str) -> String {
    let parts: Vec<&str> = path.rsplitn(3, '/').collect();
    if parts.len() >= 3 {
        format!(".../{}/{}", parts[1], parts[0])
    } else {
        path.to_string()
    }
}

pub(crate) fn human_tool_display_name(tool_name: &str) -> String {
    match tool_name {
        "bash" => "Bash".to_string(),
        "powershell" => "PowerShell".to_string(),
        "lsp" => "LSP".to_string(),
        "read_file" => "Read".to_string(),
        "write_file" => "Write".to_string(),
        "edit_file" => "Edit".to_string(),
        "project_map" => "Project Map".to_string(),
        "web_search" => "Web Search".to_string(),
        "web_fetch" => "Web Fetch".to_string(),
        "discover_skills" => "Discover Skills".to_string(),
        other => other
            .split('_')
            .filter(|segment| !segment.is_empty())
            .map(|segment| {
                let mut chars = segment.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().to_string() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

#[cfg(test)]
mod tests {
    use super::{compact_path_tail, human_tool_display_name};

    #[test]
    fn compact_path_tail_keeps_last_two_segments() {
        assert_eq!(
            compact_path_tail("/tmp/src/main.rs"),
            ".../src/main.rs"
        );
        assert_eq!(compact_path_tail("main.rs"), "main.rs");
    }

    #[test]
    fn human_tool_display_name_uses_consistent_capitalization() {
        assert_eq!(human_tool_display_name("read_file"), "Read");
        assert_eq!(human_tool_display_name("web_search"), "Web Search");
        assert_eq!(human_tool_display_name("custom_tool"), "Custom Tool");
    }
}
