use std::path::{Path, PathBuf};

use serde_json::{json, Value};

#[derive(Debug, Clone, Default)]
pub struct OrchestrationArtifactSet {
    pub summary_path: Option<PathBuf>,
    pub state_path: Option<PathBuf>,
    pub timeline_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
pub struct WorkflowRuntimeArtifactRequest<'a> {
    pub working_dir: &'a Path,
    pub workflow_path: &'a Path,
    pub workflow_name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub mode: &'a str,
    pub dry_run: bool,
    pub variables: &'a serde_json::Map<String, Value>,
    pub steps: &'a [Value],
    pub write_steps: &'a [Value],
}

#[derive(Debug, Clone, Copy)]
pub struct CoordinatorRuntimeArtifactRequest<'a> {
    pub working_dir: &'a Path,
    pub goal: &'a str,
    pub dry_run: bool,
    pub max_parallel: &'a str,
    pub phase_count: usize,
    pub workstream_count: usize,
    pub timeline: &'a str,
    pub plan: &'a [Value],
    pub results: &'a [Value],
}

pub fn persist_workflow_runtime_artifacts(
    request: WorkflowRuntimeArtifactRequest<'_>,
) -> anyhow::Result<OrchestrationArtifactSet> {
    let dir = ensure_status_dir(request.working_dir)?;
    let stamp = timestamp_stamp();
    let workflow_name = request.workflow_name.unwrap_or("workflow");
    let description = request.description.unwrap_or("none");
    let slug = slugify(workflow_name);
    let summary_path = dir.join(format!("{}-{}-workflow-execution.md", stamp, slug));
    let state_path = dir.join(format!("{}-{}-workflow-runtime-state.json", stamp, slug));
    let timeline_path = dir.join(format!(
        "{}-{}-runtime-orchestration-timeline.md",
        stamp, slug
    ));

    let state = json!({
        "kind": "workflow",
        "name": workflow_name,
        "description": description,
        "workflow_path": request.workflow_path.display().to_string(),
        "mode": request.mode,
        "dry_run": request.dry_run,
        "step_count": request.steps.len(),
        "write_steps": request.write_steps,
        "variables": request.variables,
        "steps": request.steps,
        "summary_artifact": summary_path.display().to_string(),
        "timeline_artifact": timeline_path.display().to_string(),
        "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    });
    std::fs::write(&state_path, serde_json::to_string_pretty(&state)?)?;

    let summary = render_workflow_summary(&request, &state_path, &timeline_path);
    std::fs::write(&summary_path, summary)?;

    let timeline = render_orchestration_timeline(
        "workflow",
        workflow_name,
        if request.dry_run {
            "dry-run planned"
        } else if request.steps.iter().any(step_is_error) {
            "execution finished with errors"
        } else {
            "execution finished"
        },
        &[("summary", &summary_path), ("state", &state_path)],
        request.steps,
    );
    std::fs::write(&timeline_path, timeline)?;

    Ok(OrchestrationArtifactSet {
        summary_path: Some(summary_path),
        state_path: Some(state_path),
        timeline_path: Some(timeline_path),
    })
}

pub async fn persist_workflow_runtime_artifacts_async(
    request: WorkflowRuntimeArtifactRequest<'_>,
) -> anyhow::Result<OrchestrationArtifactSet> {
    let dir = ensure_status_dir_async(request.working_dir).await?;
    let stamp = timestamp_stamp();
    let workflow_name = request.workflow_name.unwrap_or("workflow");
    let description = request.description.unwrap_or("none");
    let slug = slugify(workflow_name);
    let summary_path = dir.join(format!("{}-{}-workflow-execution.md", stamp, slug));
    let state_path = dir.join(format!("{}-{}-workflow-runtime-state.json", stamp, slug));
    let timeline_path = dir.join(format!(
        "{}-{}-runtime-orchestration-timeline.md",
        stamp, slug
    ));

    let state = json!({
        "kind": "workflow",
        "name": workflow_name,
        "description": description,
        "workflow_path": request.workflow_path.display().to_string(),
        "mode": request.mode,
        "dry_run": request.dry_run,
        "step_count": request.steps.len(),
        "write_steps": request.write_steps,
        "variables": request.variables,
        "steps": request.steps,
        "summary_artifact": summary_path.display().to_string(),
        "timeline_artifact": timeline_path.display().to_string(),
        "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    });
    tokio::fs::write(&state_path, serde_json::to_string_pretty(&state)?).await?;

    let summary = render_workflow_summary(&request, &state_path, &timeline_path);
    tokio::fs::write(&summary_path, summary).await?;

    let timeline = render_orchestration_timeline(
        "workflow",
        workflow_name,
        if request.dry_run {
            "dry-run planned"
        } else if request.steps.iter().any(step_is_error) {
            "execution finished with errors"
        } else {
            "execution finished"
        },
        &[("summary", &summary_path), ("state", &state_path)],
        request.steps,
    );
    tokio::fs::write(&timeline_path, timeline).await?;

    Ok(OrchestrationArtifactSet {
        summary_path: Some(summary_path),
        state_path: Some(state_path),
        timeline_path: Some(timeline_path),
    })
}

pub fn persist_coordinator_runtime_artifacts(
    request: CoordinatorRuntimeArtifactRequest<'_>,
) -> anyhow::Result<OrchestrationArtifactSet> {
    let dir = ensure_status_dir(request.working_dir)?;
    let stamp = timestamp_stamp();
    let slug = slugify(request.goal);
    let summary_suffix = if request.dry_run {
        "coordinate-dry-run.md"
    } else {
        "coordinate-summary.md"
    };
    let summary_path = dir.join(format!("{}-{}-{}", stamp, slug, summary_suffix));
    let state_path = dir.join(format!("{}-{}-coordinate-runtime-state.json", stamp, slug));
    let timeline_path = dir.join(format!(
        "{}-{}-runtime-orchestration-timeline.md",
        stamp, slug
    ));

    let state = json!({
        "kind": "coordinator",
        "goal": request.goal,
        "dry_run": request.dry_run,
        "phase_count": request.phase_count,
        "workstream_count": request.workstream_count,
        "max_parallel": request.max_parallel,
        "timeline": request.timeline,
        "plan": request.plan,
        "results": request.results,
        "summary_artifact": summary_path.display().to_string(),
        "timeline_artifact": timeline_path.display().to_string(),
        "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    });
    std::fs::write(&state_path, serde_json::to_string_pretty(&state)?)?;

    let summary = render_coordinator_summary(&request, &state_path, &timeline_path);
    std::fs::write(&summary_path, summary)?;

    let timeline_body = render_orchestration_timeline(
        "coordinator",
        request.goal,
        if request.dry_run {
            "dry-run planned"
        } else if request.results.iter().any(step_is_error) {
            "execution finished with errors"
        } else {
            "execution finished"
        },
        &[("summary", &summary_path), ("state", &state_path)],
        if request.dry_run {
            request.plan
        } else {
            request.results
        },
    );
    std::fs::write(&timeline_path, timeline_body)?;

    Ok(OrchestrationArtifactSet {
        summary_path: Some(summary_path),
        state_path: Some(state_path),
        timeline_path: Some(timeline_path),
    })
}

pub async fn persist_coordinator_runtime_artifacts_async(
    request: CoordinatorRuntimeArtifactRequest<'_>,
) -> anyhow::Result<OrchestrationArtifactSet> {
    let dir = ensure_status_dir_async(request.working_dir).await?;
    let stamp = timestamp_stamp();
    let slug = slugify(request.goal);
    let summary_suffix = if request.dry_run {
        "coordinate-dry-run.md"
    } else {
        "coordinate-summary.md"
    };
    let summary_path = dir.join(format!("{}-{}-{}", stamp, slug, summary_suffix));
    let state_path = dir.join(format!("{}-{}-coordinate-runtime-state.json", stamp, slug));
    let timeline_path = dir.join(format!(
        "{}-{}-runtime-orchestration-timeline.md",
        stamp, slug
    ));

    let state = json!({
        "kind": "coordinator",
        "goal": request.goal,
        "dry_run": request.dry_run,
        "phase_count": request.phase_count,
        "workstream_count": request.workstream_count,
        "max_parallel": request.max_parallel,
        "timeline": request.timeline,
        "plan": request.plan,
        "results": request.results,
        "summary_artifact": summary_path.display().to_string(),
        "timeline_artifact": timeline_path.display().to_string(),
        "timestamp": chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    });
    tokio::fs::write(&state_path, serde_json::to_string_pretty(&state)?).await?;

    let summary = render_coordinator_summary(&request, &state_path, &timeline_path);
    tokio::fs::write(&summary_path, summary).await?;

    let timeline_body = render_orchestration_timeline(
        "coordinator",
        request.goal,
        if request.dry_run {
            "dry-run planned"
        } else if request.results.iter().any(step_is_error) {
            "execution finished with errors"
        } else {
            "execution finished"
        },
        &[("summary", &summary_path), ("state", &state_path)],
        if request.dry_run {
            request.plan
        } else {
            request.results
        },
    );
    tokio::fs::write(&timeline_path, timeline_body).await?;

    Ok(OrchestrationArtifactSet {
        summary_path: Some(summary_path),
        state_path: Some(state_path),
        timeline_path: Some(timeline_path),
    })
}

fn ensure_status_dir(working_dir: &Path) -> anyhow::Result<PathBuf> {
    let dir = working_dir.join(".yode").join("status");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

async fn ensure_status_dir_async(working_dir: &Path) -> anyhow::Result<PathBuf> {
    let dir = working_dir.join(".yode").join("status");
    tokio::fs::create_dir_all(&dir).await?;
    Ok(dir)
}

fn timestamp_stamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

fn slugify(raw: &str) -> String {
    let slug = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "orchestration".to_string()
    } else {
        slug.to_string()
    }
}

fn render_workflow_summary(
    request: &WorkflowRuntimeArtifactRequest<'_>,
    state_path: &Path,
    timeline_path: &Path,
) -> String {
    let workflow_name = request.workflow_name.unwrap_or("workflow");
    let description = request.description.unwrap_or("none");
    let mut lines = vec![
        "# Workflow Execution".to_string(),
        String::new(),
        format!("- Name: {}", workflow_name),
        format!("- Path: {}", request.workflow_path.display()),
        format!("- Description: {}", description),
        format!("- Mode: {}", request.mode),
        format!("- Dry run: {}", request.dry_run),
        format!("- Step count: {}", request.steps.len()),
        format!("- State artifact: {}", state_path.display()),
        format!("- Timeline artifact: {}", timeline_path.display()),
        format!(
            "- Timestamp: {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ),
        String::new(),
    ];

    if request.variables.is_empty() {
        lines.push("Variables: none".to_string());
    } else {
        lines.push("Variables:".to_string());
        for (key, value) in request.variables {
            lines.push(format!("- {}={}", key, compact_json(value)));
        }
    }

    if request.write_steps.is_empty() {
        lines.push("Write checkpoints: none".to_string());
    } else {
        lines.push("Write checkpoints:".to_string());
        for checkpoint in request.write_steps {
            lines.push(format!("- {}", step_outline(checkpoint)));
        }
    }

    lines.push(String::new());
    lines.push("Steps:".to_string());
    for step in request.steps {
        lines.push(format!("- {}", step_outline(step)));
    }
    lines.join("\n")
}

fn render_coordinator_summary(
    request: &CoordinatorRuntimeArtifactRequest<'_>,
    state_path: &Path,
    timeline_path: &Path,
) -> String {
    let mut lines = vec![
        if request.dry_run {
            "# Coordinator Dry Run".to_string()
        } else {
            "# Coordinator Summary".to_string()
        },
        String::new(),
        format!("- Goal: {}", request.goal),
        format!("- Dry run: {}", request.dry_run),
        format!("- Max parallel: {}", request.max_parallel),
        format!("- Phase count: {}", request.phase_count),
        format!("- Workstream count: {}", request.workstream_count),
        format!("- State artifact: {}", state_path.display()),
        format!("- Timeline artifact: {}", timeline_path.display()),
        format!(
            "- Timestamp: {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ),
        String::new(),
        "Timeline:".to_string(),
    ];
    for line in request
        .timeline
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        lines.push(format!("- {}", line.trim()));
    }

    lines.push(String::new());
    lines.push("Plan:".to_string());
    for item in request.plan {
        lines.push(format!("- {}", step_outline(item)));
    }

    if !request.results.is_empty() {
        lines.push(String::new());
        lines.push("Results:".to_string());
        for item in request.results {
            lines.push(format!("- {}", step_outline(item)));
        }
    }

    lines.join("\n")
}

fn render_orchestration_timeline(
    kind: &str,
    label: &str,
    outcome: &str,
    artifacts: &[(&str, &Path)],
    entries: &[Value],
) -> String {
    let mut lines = vec![
        "# Runtime Orchestration Timeline".to_string(),
        String::new(),
        format!("- Kind: {}", kind),
        format!("- Label: {}", label),
        format!("- Outcome: {}", outcome),
        format!(
            "- Timestamp: {}",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ),
        String::new(),
        "Artifacts:".to_string(),
    ];
    for (name, path) in artifacts {
        lines.push(format!("- {}: {}", name, path.display()));
    }

    lines.push(String::new());
    lines.push("Events:".to_string());
    for entry in entries {
        lines.push(format!("- {}", step_outline(entry)));
    }
    lines.join("\n")
}

fn step_outline(value: &Value) -> String {
    if let Some(index) = value.get("index").and_then(|value| value.as_u64()) {
        let tool = value
            .get("tool")
            .or_else(|| value.get("tool_name"))
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let status = if value
            .get("approval_checkpoint")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            "checkpoint".to_string()
        } else if value
            .get("is_error")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
        {
            "error".to_string()
        } else if value
            .get("write_capable")
            .and_then(|value| value.as_bool())
            .is_some()
        {
            if value
                .get("write_capable")
                .and_then(|value| value.as_bool())
                .unwrap_or(false)
            {
                "write".to_string()
            } else {
                "read".to_string()
            }
        } else {
            "ok".to_string()
        };
        let preview = value
            .get("content")
            .map(compact_json)
            .or_else(|| value.get("params").map(compact_json))
            .unwrap_or_else(|| "none".to_string());
        return format!(
            "step {} {} [{}] {}",
            index,
            tool,
            status,
            truncate_preview(&preview, 160)
        );
    }

    let phase = value.get("phase").and_then(|value| value.as_u64());
    let batch = value.get("batch").and_then(|value| value.as_u64());
    let id = value.get("id").and_then(|value| value.as_str());
    let description = value
        .get("description")
        .and_then(|value| value.as_str())
        .unwrap_or("none");
    let status = value
        .get("status")
        .and_then(|value| value.as_str())
        .unwrap_or("planned");
    let output = value
        .get("output")
        .map(compact_json)
        .unwrap_or_else(|| "none".to_string());

    if let Some(id) = id {
        return format!(
            "phase {} batch {} {} [{}] {} / {}",
            phase.unwrap_or(0),
            batch.unwrap_or(0),
            id,
            status,
            description,
            truncate_preview(&output, 160)
        );
    }

    compact_json(value)
}

fn step_is_error(value: &Value) -> bool {
    value
        .get("is_error")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
        || value
            .get("status")
            .and_then(|value| value.as_str())
            .is_some_and(|status| status == "error")
}

fn compact_json(value: &Value) -> String {
    match value {
        Value::String(text) => text.split_whitespace().collect::<Vec<_>>().join(" "),
        _ => serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string()),
    }
}

fn truncate_preview(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        format!("{}...", text.chars().take(max_chars).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        persist_coordinator_runtime_artifacts, persist_workflow_runtime_artifacts,
        CoordinatorRuntimeArtifactRequest, WorkflowRuntimeArtifactRequest,
    };
    use serde_json::json;

    fn artifact_display_name(path: &std::path::Path) -> String {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact")
            .to_string()
    }

    #[test]
    fn workflow_runtime_artifacts_write_expected_suffixes() {
        let dir = tempfile::tempdir().unwrap();
        let workflow = dir.path().join("demo.json");
        std::fs::write(&workflow, "{}").unwrap();
        let variables = serde_json::Map::new();
        let steps =
            vec![json!({"index":1,"tool":"read_file","write_capable":false,"params":{"path":"a"}})];
        let artifacts = persist_workflow_runtime_artifacts(WorkflowRuntimeArtifactRequest {
            working_dir: dir.path(),
            workflow_path: &workflow,
            workflow_name: Some("demo"),
            description: Some("demo workflow"),
            mode: "safe_read_only",
            dry_run: true,
            variables: &variables,
            steps: &steps,
            write_steps: &[],
        })
        .unwrap();
        assert!(
            artifact_display_name(artifacts.summary_path.as_ref().unwrap())
                .ends_with("workflow-execution.md")
        );
        assert!(
            artifact_display_name(artifacts.state_path.as_ref().unwrap())
                .ends_with("workflow-runtime-state.json")
        );
        assert!(
            artifact_display_name(artifacts.timeline_path.as_ref().unwrap())
                .ends_with("runtime-orchestration-timeline.md")
        );
    }

    #[test]
    fn coordinator_runtime_artifacts_write_summary_and_state() {
        let dir = tempfile::tempdir().unwrap();
        let plan = vec![
            json!({"phase":1,"batch":1,"id":"review","description":"review","status":"planned"}),
        ];
        let results = vec![
            json!({"phase":1,"batch":1,"id":"review","description":"review","status":"ok","output":"done"}),
        ];
        let artifacts = persist_coordinator_runtime_artifacts(CoordinatorRuntimeArtifactRequest {
            working_dir: dir.path(),
            goal: "ship feature",
            dry_run: false,
            max_parallel: "2",
            phase_count: 2,
            workstream_count: 3,
            timeline: "Phase 1\nPhase 2",
            plan: &plan,
            results: &results,
        })
        .unwrap();
        let summary = std::fs::read_to_string(artifacts.summary_path.unwrap()).unwrap();
        assert!(summary.contains("# Coordinator Summary"));
        let state = std::fs::read_to_string(artifacts.state_path.unwrap()).unwrap();
        assert!(state.contains("\"kind\": \"coordinator\""));
    }
}
