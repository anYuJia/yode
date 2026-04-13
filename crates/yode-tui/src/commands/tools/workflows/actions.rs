use std::path::Path;

use crate::commands::artifact_nav::{
    artifact_freshness_badge, latest_workflow_execution_artifact, open_artifact_inspector,
    recent_artifacts_by_suffix, stale_artifact_actions,
    write_runtime_orchestration_timeline_artifact,
};
use crate::commands::context::CommandContext;
use crate::commands::{CommandOutput, CommandResult};
use crate::commands::workspace_nav::workspace_jump_inventory;

use super::definitions::{
    compact_json_preview, is_safe_workflow_step, latest_workflow_name, load_workflow_definition,
    workflow_requires_write_mode, workflow_template, workflow_template_names,
};
use super::workspace::{
    nested_workflow_guard_narrative, workflow_checkpoint_workspace, workflow_jump_targets,
    workflow_remote_bridge_follow_up, workflow_remote_bridge_hint,
    write_workflow_execution_artifact,
};

pub(super) fn execute_workflows_command(
    args: &str,
    ctx: &mut CommandContext<'_>,
    dir: &Path,
) -> CommandResult {
    let parts = args.split_whitespace().collect::<Vec<_>>();
    let project_root = std::path::PathBuf::from(&ctx.session.working_dir);

    if let ["latest"] = parts.as_slice() {
        let path = latest_workflow_execution_artifact(&project_root)
            .ok_or_else(|| "No workflow execution artifacts found.".to_string())?;
        let footer = workflow_artifact_footer(&path);
        let doc = open_artifact_inspector(
            "Workflow execution inspector",
            &path,
            Some(footer),
            vec![("kind".into(), "workflow".into())],
        )
        .ok_or_else(|| format!("Failed to open workflow artifact {}.", path.display()))?;
        return Ok(CommandOutput::OpenInspector(doc));
    }
    if let ["timeline"] = parts.as_slice() {
        let path = write_runtime_orchestration_timeline_artifact(
            &project_root,
            &ctx.session.session_id,
        )
        .ok_or_else(|| "Failed to write runtime orchestration timeline.".to_string())?;
        let doc = open_artifact_inspector(
            "Runtime orchestration timeline",
            std::path::Path::new(&path),
            Some("/inspect artifact latest-orchestration | /inspect workflows latest".to_string()),
            vec![("kind".into(), "orchestration".into())],
        )
        .ok_or_else(|| format!("Failed to open timeline artifact {}.", path))?;
        return Ok(CommandOutput::OpenInspector(doc));
    }
    if let ["history"] = parts.as_slice() {
        let artifacts = recent_artifacts_by_suffix(
            &project_root.join(".yode").join("status"),
            "workflow-execution.md",
            8,
        );
        if artifacts.is_empty() {
            return Ok(CommandOutput::Message(
                "No workflow execution artifacts found.".to_string(),
            ));
        }
        let mut output = String::from("Workflow execution history:\n");
        for path in artifacts {
            output.push_str(&format!(
                "  - [{}] {}\n",
                artifact_freshness_badge(&path),
                path.display()
            ));
        }
        output.push_str("\nUse `/workflows latest` or `/inspect artifact latest-workflow`.");
        return Ok(CommandOutput::Message(output));
    }
    if let ["run", name] = parts.as_slice() {
        let name = resolve_workflow_name(dir, name)?;
        let (path, json, steps) = load_workflow_definition(dir, &name)?;
        let prompt = format!(
            "Use `workflow_run` with name=\"{}\" and summarize the result.",
            name
        );
        ctx.input.set_text(&prompt);
        let artifact = write_workflow_execution_artifact(
            &project_root,
            &ctx.session.session_id,
            &name,
            &path,
            json.get("description")
                .and_then(|value| value.as_str())
                .unwrap_or("none"),
            "safe read-only",
            &prompt,
            &workflow_step_summaries(&steps),
        );
        let timeline =
            write_runtime_orchestration_timeline_artifact(&project_root, &ctx.session.session_id);
        return Ok(CommandOutput::Message(format!(
            "Loaded a workflow_run prompt for '{}'.\nArtifact: {}\nTimeline: {}",
            name,
            artifact.unwrap_or_else(|| "none".to_string()),
            timeline.unwrap_or_else(|| "none".to_string())
        )));
    }
    if let ["run-write", name] = parts.as_slice() {
        let name = resolve_workflow_name(dir, name)?;
        let (path, json, steps) = load_workflow_definition(dir, &name)?;
        let prompt = format!(
            "Use `workflow_run_with_writes` with name=\"{}\". Explain why this workflow needs mutating tools, then summarize every file or git-side effect clearly.",
            name
        );
        ctx.input.set_text(&prompt);
        let artifact = write_workflow_execution_artifact(
            &project_root,
            &ctx.session.session_id,
            &name,
            &path,
            json.get("description")
                .and_then(|value| value.as_str())
                .unwrap_or("none"),
            "write-capable",
            &prompt,
            &workflow_step_summaries(&steps),
        );
        let timeline =
            write_runtime_orchestration_timeline_artifact(&project_root, &ctx.session.session_id);
        return Ok(CommandOutput::Message(format!(
            "Loaded a write-enabled workflow prompt for '{}'.\nArtifact: {}\nTimeline: {}",
            name,
            artifact.unwrap_or_else(|| "none".to_string()),
            timeline.unwrap_or_else(|| "none".to_string())
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
        "\n\n{}\n{}\n{}\n{}",
        nested_workflow_guard_narrative(),
        workflow_remote_bridge_hint(),
        workflow_remote_bridge_follow_up(&workflow_project_root(dir)).join("\n"),
        workspace_jump_inventory(workflow_jump_targets(&name))
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
        "{}\n\n{}\n{}\n{}\n{}",
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
        workflow_remote_bridge_hint(),
        workflow_remote_bridge_follow_up(&workflow_project_root(dir)).join("\n"),
        workspace_jump_inventory(workflow_jump_targets(&name))
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
        "\nUse `/workflows run <name>` for safe workflows, `/workflows run-write <name>` for confirmed write-capable workflows, or `/workflows latest` to inspect the latest execution artifact.",
    );
    output
}

fn workflow_step_summaries(steps: &[serde_json::Value]) -> Vec<String> {
    steps.iter()
        .enumerate()
        .map(|(index, step)| {
            let tool_name = step
                .get("tool_name")
                .and_then(|value| value.as_str())
                .unwrap_or("unknown");
            format!(
                "{}. {} [{}]",
                index + 1,
                tool_name,
                if is_safe_workflow_step(tool_name) {
                    "safe"
                } else {
                    "write"
                }
            )
        })
        .collect()
}

fn workflow_project_root(dir: &Path) -> std::path::PathBuf {
    dir.parent()
        .and_then(|path| path.parent())
        .unwrap_or(dir)
        .to_path_buf()
}

fn workflow_artifact_footer(path: &Path) -> String {
    let mut lines = vec![
        "/inspect workflows timeline".to_string(),
        "/workflows preview latest".to_string(),
        "/workflows run latest".to_string(),
        "/workflows run-write latest".to_string(),
    ];
    if let Some(stale) = stale_artifact_actions(path, &lines) {
        lines.push(stale);
    }
    lines.join("\n")
}
