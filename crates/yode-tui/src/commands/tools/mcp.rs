use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use std::collections::BTreeMap;

pub struct McpCommand {
    meta: CommandMeta,
}

impl McpCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "mcp",
                description: "Summarize MCP-backed tools grouped by server",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Tools,
                hidden: false,
            },
        }
    }
}

impl Command for McpCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let mut by_server = BTreeMap::<String, Vec<String>>::new();
        for tool in ctx.tools.definitions() {
            if let Some((server, original_name)) = parse_mcp_tool_name(&tool.name) {
                by_server
                    .entry(server.to_string())
                    .or_default()
                    .push(original_name.to_string());
            }
        }

        if by_server.is_empty() {
            return Ok(CommandOutput::Message(
                "No MCP server tools are currently registered.".to_string(),
            ));
        }

        let mut lines = vec![format!("MCP servers ({}):", by_server.len())];
        for (server, mut tools) in by_server {
            tools.sort();
            let preview = tools.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
            let more = tools.len().saturating_sub(6);
            lines.push(format!(
                "  - {} [{} tool(s)] {}{}",
                server,
                tools.len(),
                preview,
                if more > 0 {
                    format!(" (+{} more)", more)
                } else {
                    String::new()
                }
            ));
        }
        Ok(CommandOutput::Messages(lines))
    }
}

pub fn parse_mcp_tool_name(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    let (server, tool) = rest.split_once('_')?;
    Some((server, tool))
}

#[cfg(test)]
mod tests {
    use super::parse_mcp_tool_name;

    #[test]
    fn parse_mcp_tool_name_extracts_server_and_tool() {
        assert_eq!(
            parse_mcp_tool_name("mcp__github_list_prs"),
            Some(("github", "list_prs"))
        );
        assert_eq!(parse_mcp_tool_name("read_file"), None);
    }
}
