use std::collections::{hash_map::DefaultHasher, BTreeMap};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use uuid::Uuid;

use crate::tool::{ToolContext, ToolResult};

pub(crate) const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 120;
pub(crate) const MAX_COMMAND_TIMEOUT_SECS: u64 = 600;

pub(crate) fn command_timeout_secs(params: &Value) -> u64 {
    match params
        .get("timeout_ms")
        .and_then(|value| value.as_u64())
        .or_else(|| params.get("timeout").and_then(|value| value.as_u64()))
    {
        Some(timeout_ms) if timeout_ms >= 1000 => (timeout_ms / 1000).min(MAX_COMMAND_TIMEOUT_SECS),
        Some(timeout_ms) => timeout_ms.min(MAX_COMMAND_TIMEOUT_SECS),
        None => DEFAULT_COMMAND_TIMEOUT_SECS,
    }
}

pub(crate) fn timeout_ms_description() -> String {
    format!(
        "Optional timeout in milliseconds (max {}ms). Default: {}ms.",
        MAX_COMMAND_TIMEOUT_SECS * 1000,
        DEFAULT_COMMAND_TIMEOUT_SECS * 1000
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FileFingerprint {
    Missing,
    File { len: u64, hash: u64 },
}

#[derive(Debug, Clone)]
pub(crate) struct GitChangeSnapshot {
    root: PathBuf,
    files: BTreeMap<String, FileFingerprint>,
}

impl GitChangeSnapshot {
    pub(crate) async fn capture(working_dir: &Path) -> Option<Self> {
        let root = git_root(working_dir).await?;
        let paths = git_status_paths(&root).await?;
        let mut files = BTreeMap::new();
        for path in paths {
            let fingerprint = file_fingerprint(&root.join(&path)).await;
            files.insert(path, fingerprint);
        }
        Some(Self { root, files })
    }

    pub(crate) async fn changed_files_since(&self, working_dir: &Path) -> Vec<String> {
        let Some(after) = Self::capture(working_dir).await else {
            return Vec::new();
        };
        if after.root != self.root {
            return Vec::new();
        }

        after
            .files
            .into_iter()
            .filter_map(|(path, fingerprint)| {
                (self.files.get(&path) != Some(&fingerprint)).then_some(path)
            })
            .collect()
    }
}

async fn git_root(working_dir: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .arg("-C")
        .arg(working_dir)
        .arg("rev-parse")
        .arg("--show-toplevel")
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!root.is_empty()).then(|| PathBuf::from(root))
}

async fn git_status_paths(root: &Path) -> Option<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("status")
        .arg("--porcelain=v1")
        .arg("-z")
        .arg("--untracked-files=all")
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse_porcelain_paths(&output.stdout))
}

fn parse_porcelain_paths(output: &[u8]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut entries = output
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty());

    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let status_x = entry[0] as char;
        let path = String::from_utf8_lossy(&entry[3..]).to_string();
        if !path.is_empty() {
            paths.push(path);
        }
        if matches!(status_x, 'R' | 'C') {
            let _ = entries.next();
        }
    }

    paths
}

async fn file_fingerprint(path: &Path) -> FileFingerprint {
    let Ok(metadata) = tokio::fs::metadata(path).await else {
        return FileFingerprint::Missing;
    };
    if !metadata.is_file() {
        return FileFingerprint::Missing;
    }
    let Ok(bytes) = tokio::fs::read(path).await else {
        return FileFingerprint::Missing;
    };
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    FileFingerprint::File {
        len: metadata.len(),
        hash: hasher.finish(),
    }
}

pub(crate) struct BackgroundShellSpec<'a> {
    pub executable: &'a Path,
    pub args: Vec<String>,
    pub command_display: &'a str,
    pub task_kind: &'static str,
    pub description_prefix: &'static str,
    pub start_message: &'static str,
}

pub(crate) async fn execute_background_shell(
    spec: BackgroundShellSpec<'_>,
    working_dir: &Path,
    ctx: &ToolContext,
) -> Result<ToolResult> {
    let Some(runtime_tasks) = &ctx.runtime_tasks else {
        let _child = Command::new(spec.executable)
            .args(&spec.args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        return Ok(ToolResult::success(format!(
            "{}: {}",
            spec.start_message, spec.command_display
        )));
    };

    let tasks_dir = working_dir.join(".yode").join("tasks");
    tokio::fs::create_dir_all(&tasks_dir).await?;
    let output_path = tasks_dir.join(format!("{}-{}.log", spec.task_kind, Uuid::new_v4()));
    let output_path_str = output_path.display().to_string();
    let transcript_path = crate::runtime_tasks::latest_transcript_artifact_path(working_dir);
    let description = format!(
        "{}: {}",
        spec.description_prefix,
        spec.command_display.chars().take(60).collect::<String>()
    );
    let (task, mut cancel_rx) = {
        let mut store = runtime_tasks.lock().await;
        store.create_with_transcript(
            spec.task_kind.to_string(),
            spec.task_kind.to_string(),
            description,
            output_path_str.clone(),
            transcript_path.clone(),
        )
    };

    let task_id = task.id.clone();
    let runtime_tasks = runtime_tasks.clone();
    let working_dir = PathBuf::from(working_dir);
    let executable = spec.executable.to_path_buf();
    let args = spec.args;
    let command_display = spec.command_display.to_string();
    let launch_command = command_display.clone();
    let output_path_spawn = output_path.clone();
    tokio::spawn(async move {
        {
            let mut store = runtime_tasks.lock().await;
            store.mark_running(&task_id);
            store.update_progress(&task_id, format!("Running {}", command_display));
        }

        let stdout_file = match std::fs::File::create(&output_path_spawn) {
            Ok(file) => file,
            Err(err) => {
                runtime_tasks
                    .lock()
                    .await
                    .mark_failed(&task_id, format!("Failed to create output file: {}", err));
                return;
            }
        };
        let stderr_file = match stdout_file.try_clone() {
            Ok(file) => file,
            Err(err) => {
                runtime_tasks.lock().await.mark_failed(
                    &task_id,
                    format!("Failed to clone output file handle: {}", err),
                );
                return;
            }
        };

        let mut child = match Command::new(&executable)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(stdout_file))
            .stderr(Stdio::from(stderr_file))
            .current_dir(&working_dir)
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                runtime_tasks.lock().await.mark_failed(
                    &task_id,
                    format!("Failed to spawn background command: {}", err),
                );
                return;
            }
        };
        if let Some(mut stdin) = child.stdin.take() {
            let (stdin_tx, mut stdin_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            let attached = runtime_tasks
                .lock()
                .await
                .attach_stdin_writer(&task_id, stdin_tx);
            if attached {
                let stdin_task_id = task_id.clone();
                let stdin_runtime_tasks = runtime_tasks.clone();
                tokio::spawn(async move {
                    while let Some(input) = stdin_rx.recv().await {
                        if let Err(err) = stdin.write_all(input.as_bytes()).await {
                            stdin_runtime_tasks.lock().await.update_progress(
                                &stdin_task_id,
                                format!("stdin closed: {}", err),
                            );
                            break;
                        }
                        if let Err(err) = stdin.flush().await {
                            stdin_runtime_tasks.lock().await.update_progress(
                                &stdin_task_id,
                                format!("stdin flush failed: {}", err),
                            );
                            break;
                        }
                    }
                });
            }
        }

        let (done_tx, done_rx) = tokio::sync::watch::channel(false);
        spawn_output_progress_monitor(
            runtime_tasks.clone(),
            task_id.clone(),
            output_path_spawn.clone(),
            done_rx,
        );

        tokio::select! {
            wait_result = child.wait() => {
                let _ = done_tx.send(true);
                match wait_result {
                    Ok(status) if status.success() => {
                        runtime_tasks.lock().await.mark_completed(&task_id);
                    }
                    Ok(status) => {
                        runtime_tasks.lock().await.mark_failed(
                            &task_id,
                            format!("Background command exited with status {}", status),
                        );
                    }
                    Err(err) => {
                        runtime_tasks
                            .lock()
                            .await
                            .mark_failed(&task_id, format!("Failed to wait for command: {}", err));
                    }
                }
            }
            changed = cancel_rx.changed() => {
                if changed.is_ok() && *cancel_rx.borrow() {
                    let _ = child.kill().await;
                    let _ = done_tx.send(true);
                    runtime_tasks.lock().await.mark_cancelled(&task_id);
                }
            }
        }
    });

    Ok(ToolResult::success_with_metadata(
        format!(
            "{} task started: {} ({})",
            spec.description_prefix, task.id, launch_command
        ),
        serde_json::json!({
            "task_id": task.id,
            "task_kind": spec.task_kind,
            "output_path": output_path_str,
            "transcript_path": transcript_path,
            "run_in_background": true,
        }),
    ))
}

fn spawn_output_progress_monitor(
    runtime_tasks: std::sync::Arc<tokio::sync::Mutex<crate::runtime_tasks::RuntimeTaskStore>>,
    task_id: String,
    output_path: PathBuf,
    mut done_rx: tokio::sync::watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3));
        let mut last_preview = String::new();
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Ok(content) = tokio::fs::read_to_string(&output_path).await {
                        if let Some(line) = content.lines().rev().find(|line| !line.trim().is_empty()) {
                            let preview = truncate_progress_line(line, 120);
                            if preview != last_preview {
                                runtime_tasks
                                    .lock()
                                    .await
                                    .update_progress(&task_id, preview.clone());
                                last_preview = preview;
                            }
                        }
                    }
                }
                changed = done_rx.changed() => {
                    if changed.is_ok() && *done_rx.borrow() {
                        break;
                    }
                }
            }
        }
    });
}

fn truncate_progress_line(line: &str, max_chars: usize) -> String {
    if line.chars().count() > max_chars {
        format!("{}...", line.chars().take(max_chars).collect::<String>())
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn timeout_defaults_and_clamps_milliseconds() {
        assert_eq!(
            command_timeout_secs(&json!({})),
            DEFAULT_COMMAND_TIMEOUT_SECS
        );
        assert_eq!(command_timeout_secs(&json!({ "timeout_ms": 2500 })), 2);
        assert_eq!(
            command_timeout_secs(&json!({ "timeout_ms": 999_000 })),
            MAX_COMMAND_TIMEOUT_SECS
        );
    }

    #[test]
    fn timeout_keeps_legacy_seconds_shape_for_small_values() {
        assert_eq!(command_timeout_secs(&json!({ "timeout_ms": 90 })), 90);
        assert_eq!(command_timeout_secs(&json!({ "timeout": 5 })), 5);
    }
}
