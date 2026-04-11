use super::*;

impl BashTool {
    pub(super) async fn execute_background(
        &self,
        command: &str,
        working_dir: &Path,
        ctx: &ToolContext,
    ) -> Result<ToolResult> {
        let Some(runtime_tasks) = &ctx.runtime_tasks else {
            let _child = Command::new("sh")
                .arg("-c")
                .arg(command)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            return Ok(ToolResult::success(format!(
                "Command started in background: {}",
                command
            )));
        };

        let tasks_dir = working_dir.join(".yode").join("tasks");
        tokio::fs::create_dir_all(&tasks_dir).await?;
        let output_path = tasks_dir.join(format!("bash-{}.log", Uuid::new_v4()));
        let output_path_str = output_path.display().to_string();
        let transcript_path =
            crate::runtime_tasks::latest_transcript_artifact_path(working_dir);
        let description = format!(
            "Background bash: {}",
            command.chars().take(60).collect::<String>()
        );
        let (task, mut cancel_rx) = {
            let mut store = runtime_tasks.lock().await;
            store.create_with_transcript(
                "bash".to_string(),
                "bash".to_string(),
                description,
                output_path_str.clone(),
                transcript_path.clone(),
            )
        };

        let task_id = task.id.clone();
        let runtime_tasks = runtime_tasks.clone();
        let working_dir = PathBuf::from(working_dir);
        let command = command.to_string();
        let launch_command = command.clone();
        let output_path_spawn = output_path.clone();
        tokio::spawn(async move {
            {
                let mut store = runtime_tasks.lock().await;
                store.mark_running(&task_id);
                store.update_progress(&task_id, format!("Running {}", command));
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

            let mut child = match Command::new("sh")
                .arg("-c")
                .arg(&command)
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

            let (done_tx, mut done_rx) = tokio::sync::watch::channel(false);
            let runtime_tasks_monitor = runtime_tasks.clone();
            let task_id_monitor = task_id.clone();
            let output_path_monitor = output_path_spawn.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
                let mut last_preview = String::new();
                loop {
                    tokio::select! {
                        _ = interval.tick() => {
                            if let Ok(content) = tokio::fs::read_to_string(&output_path_monitor).await {
                                if let Some(line) = content.lines().rev().find(|line| !line.trim().is_empty()) {
                                    let preview = if line.chars().count() > 120 {
                                        let shortened = line.chars().take(120).collect::<String>();
                                        format!("{}...", shortened)
                                    } else {
                                        line.to_string()
                                    };
                                    if preview != last_preview {
                                        runtime_tasks_monitor
                                            .lock()
                                            .await
                                            .update_progress(&task_id_monitor, preview.clone());
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
            format!("Background task started: {} ({})", task.id, launch_command),
            json!({
                "task_id": task.id,
                "task_kind": "bash",
                "output_path": output_path_str,
                "transcript_path": transcript_path,
                "run_in_background": true,
            }),
        ))
    }
}
