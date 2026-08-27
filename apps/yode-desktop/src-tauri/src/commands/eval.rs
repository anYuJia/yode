use std::path::{Path, PathBuf};

use serde::Serialize;
use yode_evals::{aggregate_outcomes, EvalAggregateReport, EvalOutcome, EvalTask};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalWorkspaceSnapshot {
    pub tasks: Vec<EvalTask>,
    pub outcomes: Vec<EvalOutcome>,
    pub report: EvalAggregateReport,
}

#[tauri::command]
pub async fn eval_task_save(project_root: String, task: EvalTask) -> Result<String, String> {
    task.validate().map_err(|error| error.to_string())?;
    let dir = eval_dir(&project_root)?.join("tasks");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|error| error.to_string())?;
    let path = dir.join(format!("{}.json", safe_id(&task.id)));
    write_json(&path, &task).await?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub async fn eval_tasks_list(project_root: String) -> Result<Vec<EvalTask>, String> {
    read_json_dir::<EvalTask>(&eval_dir(&project_root)?.join("tasks")).await
}

#[tauri::command]
pub async fn eval_outcome_record(
    project_root: String,
    outcome: EvalOutcome,
) -> Result<String, String> {
    if outcome.task_id.trim().is_empty() {
        return Err("eval outcome task_id cannot be empty".to_string());
    }
    let dir = eval_dir(&project_root)?.join("outcomes");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|error| error.to_string())?;
    let timestamp = outcome.finished_at.format("%Y%m%dT%H%M%S%3fZ");
    let path = dir.join(format!("{}-{}.json", safe_id(&outcome.task_id), timestamp));
    write_json(&path, &outcome).await?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub async fn eval_outcomes_list(project_root: String) -> Result<Vec<EvalOutcome>, String> {
    read_json_dir::<EvalOutcome>(&eval_dir(&project_root)?.join("outcomes")).await
}

#[tauri::command]
pub async fn eval_workspace_snapshot(
    project_root: String,
) -> Result<EvalWorkspaceSnapshot, String> {
    let tasks = eval_tasks_list(project_root.clone()).await?;
    let outcomes = eval_outcomes_list(project_root).await?;
    let report = aggregate_outcomes(&outcomes);
    Ok(EvalWorkspaceSnapshot {
        tasks,
        outcomes,
        report,
    })
}

fn eval_dir(project_root: &str) -> Result<PathBuf, String> {
    let trimmed = project_root.trim();
    if trimmed.is_empty() {
        return Err("project_root cannot be empty".to_string());
    }
    Ok(Path::new(trimmed).join(".yode").join("evals"))
}

fn safe_id(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if cleaned.is_empty() {
        "eval".to_string()
    } else {
        cleaned
    }
}

async fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let temp_path = path.with_extension("json.tmp");
    tokio::fs::write(&temp_path, bytes)
        .await
        .map_err(|error| error.to_string())?;
    tokio::fs::rename(&temp_path, path)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn read_json_dir<T>(dir: &Path) -> Result<Vec<T>, String>
where
    T: serde::de::DeserializeOwned,
{
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut reader = tokio::fs::read_dir(dir)
        .await
        .map_err(|error| error.to_string())?;
    let mut paths = Vec::new();
    while let Some(entry) = reader
        .next_entry()
        .await
        .map_err(|error| error.to_string())?
    {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();

    let mut values = Vec::with_capacity(paths.len());
    for path in paths {
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|error| format!("failed to read {}: {}", path.display(), error))?;
        let value = serde_json::from_slice::<T>(&bytes)
            .map_err(|error| format!("failed to parse {}: {}", path.display(), error))?;
        values.push(value);
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use yode_evals::{AcceptanceCriterion, EvalTaskCategory};

    #[test]
    fn safe_id_removes_path_separators() {
        assert_eq!(safe_id("bug/../one"), "bug----one");
        assert_eq!(safe_id("***"), "eval");
    }

    #[tokio::test]
    async fn task_round_trip_uses_workspace_eval_dir() {
        let dir = tempfile::tempdir().unwrap();
        let task = EvalTask {
            id: "bug-1".to_string(),
            title: "Bug one".to_string(),
            category: EvalTaskCategory::BugFix,
            prompt: "Fix the bug".to_string(),
            repository: None,
            base_revision: None,
            timeout_secs: Some(60),
            allowed_tools: Vec::new(),
            acceptance: vec![AcceptanceCriterion {
                id: "tests".to_string(),
                description: "Tests pass".to_string(),
                required: true,
                evidence_hint: Some("cargo test".to_string()),
            }],
            tags: Vec::new(),
        };

        eval_task_save(dir.path().display().to_string(), task.clone())
            .await
            .unwrap();
        let tasks = eval_tasks_list(dir.path().display().to_string())
            .await
            .unwrap();
        assert_eq!(tasks, vec![task]);
    }
}
