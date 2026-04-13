use std::path::Path;

use crate::commands::context::CommandContext;
use crate::commands::{CommandOutput, CommandResult};

use super::definitions::{
    compact_json_preview, is_safe_workflow_step, latest_workflow_name, load_workflow_definition,
    workflow_requires_write_mode, workflow_template, workflow_template_names,
};
use super::workspace::{
    nested_workflow_guard_narrative, workflow_checkpoint_workspace, workflow_remote_bridge_hint,
};

pub(super) fn execute_workflows_command(
    args: &str,
    ctx: &mut CommandContext<'_>,
    dir: &Path,
) -> CommandResult {
    let parts = args.split_whitespace().collect::<Vec<_>>();
    if let ["run", name] = parts.as_slice() {
        let name = resolve_workflow_name(dir, name)?;
        ctx.input.set_text(&format!(
            "Use `workflow_run` with name=\"{}\" and summarize the result.",
            name
        ));
        return Ok(CommandOutput::Message(format!(
            "Loaded a workflow_run prompt for '{}'.",
            name
        )));
    }
    if let ["run-write", name] = parts.as_slice() {
        let name = resolve_workflow_name(dir, name)?;
        ctx.input.set_text(&format!(
            "Use `workflow_run_with_writes` with name=\"{}\". Explain why this workflow needs mutating tools, then summarize every file or git-side effect clearly.",
            name
        ));
        return Ok(CommandOutput::Message(format!(
            "Loaded a write-enabled workflow prompt for '{}'.",
            name
        )));
    }
    if let ["show", name] = parts.as_slice() {
        return Ok(CommandOutput::Message(render_show_workflow(
            dir,
            resolve_workflow_name(dir, name)?,
        )?));
    }
    if let ["preview", name] = parts.as_slice() {
        return Ok(CommandOutput::Message(render_preview_workflow(
            dir,
            resolve_workflow_name(dir, name)?,
        )?));
    }
    if let ["init", template] = parts.as_slice() {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("Failed to create {}: {}", dir.display(), err))?;
        let (file_name, content) = workflow_template(template).ok_or_else(|| {
            format!(
                "Unknown workflow template: {}. Available: {}",
                template,
                workflow_template_names().join(", ")
            )
        })?;
        let path = dir.join(file_name);
        std::fs::write(&path, content)
            .map_err(|err| format!("Failed to write {}: {}", path.display(), err))?;
        return Ok(CommandOutput::Message(format!(
            "Initialized workflow template '{}' at {}.",
            template,
            path.display()
        )));
    }

    Ok(CommandOutput::Message(render_workflow_list(dir)))
}

fn resolve_workflow_name(dir: &Path, name: &str) -> Result<String, String> {
    if name == "latest" {
        latest_workflow_name(dir).ok_or_else(|| "No workflow scripts found.".to_string())
    } else {
        Ok(name.to_string())
    }
}

fn render_show_workflow(dir: &Path, name: String) -> Result<String, String> {
    let (path, json, steps) = load_workflow_definition(dir, &name)?;
    let write_capable = workflow_requires_write_mode(&steps);
    let write_steps = steps
        .iter()
        .enumerate()
        .filter_map(|(index, step)| {
            let tool_name = step.get("tool_name").and_then(|value| value.as_str())?;
            (!is_safe_workflow_step(tool_name)).then(|| format!("{}. {}", index + 1, tool_name))
        })
        .collect::<Vec<_>>();
    let mut rendered_steps = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let tool_name = step
            .get("tool_name")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        rendered_steps.push(format!(
            "{}. {} [{}] {}",
            index + 1,
            tool_name,
            if is_safe_workflow_step(tool_name) {
                "safe"
            } else {
                "write"
            },
            if step
                .get("continue_on_error")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                "(continue_on_error)"
            } else {
                ""
            }
        ));
    }
    let mut output = workflow_checkpoint_workspace(
        &format!(
            "Workflow {}",
            json.get("name")
                .and_then(|value| value.as_str())
                .unwrap_or(name.as_str())
        ),
        &path,
        json.get("description")
            .and_then(|value| value.as_str())
            .unwrap_or("none"),
        if write_capable {
            "write-capable (use /workflows run-write)"
        } else {
            "safe read-only (use /workflows run)"
        },
        rendered_steps,
    );
    if !write_steps.is_empty() {
        output.push_str("\n\nWrite-capable steps:\n");
        for step in write_steps {
            output.push_str(&format!("  - {}\n", step));
        }
    }
    output.push_str(&format!(
        "\n\n{}\n{}",
        nested_workflow_guard_narrative(),
        workflow_remote_bridge_hint()
    ));
    Ok(output)
}

fn render_preview_workflow(dir: &Path, name: String) -> Result<String, String> {
    let (path, json, steps) = load_workflow_definition(dir, &name)?;
    let mut rendered_steps = Vec::new();
    for (index, step) in steps.iter().enumerate() {
        let tool_name = step
            .get("tool_name")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let params_preview = step
            .get("params")
            .map(compact_json_preview)
            .unwrap_or_else(|| "{}".to_string());
        rendered_steps.push(format!(
            "{}. {} [{}]{} / params: {}",
            index + 1,
            tool_name,
            if is_safe_workflow_step(tool_name) {
                "safe"
            } else {
                "write"
            },
            if step
                .get("continue_on_error")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                " continue_on_error"
            } else {
                ""
            },
            params_preview
        ));
    }
    Ok(format!(
        "{}\n\n{}\n{}",
        workflow_checkpoint_workspace(
            "Workflow plan preview",
            &path,
            json.get("description")
                .and_then(|value| value.as_str())
                .unwrap_or("none"),
            if workflow_requires_write_mode(&steps) {
                "write-capable"
            } else {
                "safe read-only"
            },
            rendered_steps,
        ),
        nested_workflow_guard_narrative(),
        workflow_remote_bridge_hint()
    ))
}

fn render_workflow_list(dir: &Path) -> String {
    let entries = std::fs::read_dir(dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect::<Vec<_>>();

    if entries.is_empty() {
        return format!("No workflow scripts found in {}.", dir.display());
    }

    let mut output = format!("Workflow scripts in {}:\n", dir.display());
    for path in entries {
        let label = std::fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
            .map(|json| {
                let mode = json
                    .get("steps")
                    .and_then(|value| value.as_array())
                    .map(|steps| {
                        if workflow_requires_write_mode(steps) {
                            "write"
                        } else {
                            "safe"
                        }
                    })
                    .unwrap_or("unknown");
                let name = json
                    .get("name")
                    .and_then(|value| value.as_str())
                    .unwrap_or_else(|| {
                        path.file_stem()
                            .and_then(|value| value.to_str())
                            .unwrap_or("workflow")
                    });
                let description = json
                    .get("description")
                    .and_then(|value| value.as_str())
                    .unwrap_or("no description");
                format!("{} [{}] — {}", name, mode, description)
            })
            .unwrap_or_else(|| path.display().to_string());
        output.push_str(&format!("  - {} ({})\n", label, path.display()));
    }
    output.push_str(
        "\nUse `/workflows run <name>` for safe workflows or `/workflows run-write <name>` for confirmed write-capable workflows.",
    );
    output
}
