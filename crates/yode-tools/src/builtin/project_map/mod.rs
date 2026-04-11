mod analysis;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use crate::tool::{Tool, ToolCapabilities, ToolContext, ToolResult};

use self::analysis::{
    analyze_dependencies, build_module_tree, detect_project_type, find_config_files,
    find_entry_points, scan_project_stats,
};

pub struct ProjectMapTool;

#[async_trait]
impl Tool for ProjectMapTool {
    fn name(&self) -> &str {
        "project_map"
    }

    fn user_facing_name(&self) -> &str {
        "Project Map"
    }

    fn activity_description(&self, _params: &Value) -> String {
        "Analyzing project structure".to_string()
    }

    fn description(&self) -> &str {
        "Generate a project structure model including type detection, module map, entry points, \
         config files, and dependency analysis. Use this FIRST when analyzing an unfamiliar codebase \
         to build a mental model before diving into specifics."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "depth": {
                    "type": "integer",
                    "description": "Directory scan depth (default: 2)",
                    "default": 2
                },
                "include_deps": {
                    "type": "boolean",
                    "description": "Whether to analyze module-level dependencies (default: true)",
                    "default": true
                }
            }
        })
    }

    fn capabilities(&self) -> ToolCapabilities {
        ToolCapabilities {
            read_only: true,
            requires_confirmation: false,
            supports_auto_execution: true,
        }
    }

    async fn execute(&self, params: Value, ctx: &ToolContext) -> Result<ToolResult> {
        let working_dir = ctx
            .working_dir
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Working directory not set"))?;

        let depth = params.get("depth").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
        let include_deps = params
            .get("include_deps")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let mut output = String::new();

        let project_type = detect_project_type(working_dir);
        output.push_str("## Project Overview\n");
        output.push_str(&format!("- Type: {}\n", project_type.display_name()));

        let stats = scan_project_stats(working_dir);
        output.push_str(&format!(
            "- Scale: {} files, ~{}K lines\n",
            stats.file_count,
            stats.total_lines / 1000
        ));
        for (lang, count) in &stats.lines_by_language {
            output.push_str(&format!("  - {}: {} lines\n", lang, count));
        }

        let entries = find_entry_points(working_dir, &project_type);
        if !entries.is_empty() {
            output.push_str("\n## Entry Points\n");
            for entry in &entries {
                let rel = entry.strip_prefix(working_dir).unwrap_or(entry).display();
                output.push_str(&format!("- {}\n", rel));
            }
        }

        let tree = build_module_tree(working_dir, depth);
        if !tree.is_empty() {
            output.push_str("\n## Module Map\n");
            output.push_str(&tree);
        }

        if include_deps {
            let deps = analyze_dependencies(working_dir, &project_type);
            if !deps.is_empty() {
                output.push_str("\n## Dependencies\n");
                for (module, dep_list) in &deps {
                    output.push_str(&format!(
                        "- {} → depends on: {}\n",
                        module,
                        dep_list.join(", ")
                    ));
                }
            }
        }

        let configs = find_config_files(working_dir);
        if !configs.is_empty() {
            output.push_str("\n## Config Files\n");
            for cfg in &configs {
                let rel = cfg.strip_prefix(working_dir).unwrap_or(cfg).display();
                output.push_str(&format!("- {}\n", rel));
            }
        }

        let metadata = json!({
            "project_type": project_type.display_name(),
            "file_count": stats.file_count,
            "total_lines": stats.total_lines,
        });

        Ok(ToolResult::success_with_metadata(output, metadata))
    }
}
