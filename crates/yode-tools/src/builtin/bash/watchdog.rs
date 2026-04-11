use super::output::looks_like_interactive_prompt;
use super::*;

use tokio::io::AsyncReadExt;

pub(super) enum StallResult {
    Completed(std::process::Output),
    Stalled(String),
    Timeout,
    Error(String),
}

impl BashTool {
    pub(super) async fn run_with_stall_watchdog(
        &self,
        child: &mut tokio::process::Child,
        timeout: Duration,
        progress_tx: Option<mpsc::UnboundedSender<ToolProgress>>,
    ) -> StallResult {
        let start = std::time::Instant::now();
        let mut last_output_time = std::time::Instant::now();
        let mut accumulated_stdout = Vec::new();

        let mut stdout = match child.stdout.take() {
            Some(s) => s,
            None => {
                return match tokio::time::timeout(timeout, child.wait()).await {
                    Ok(Ok(status)) => {
                        let mut stderr_buf = Vec::new();
                        if let Some(mut stderr) = child.stderr.take() {
                            let _ = stderr.read_to_end(&mut stderr_buf).await;
                        }
                        StallResult::Completed(std::process::Output {
                            status,
                            stdout: Vec::new(),
                            stderr: stderr_buf,
                        })
                    }
                    Ok(Err(e)) => StallResult::Error(e.to_string()),
                    Err(_) => StallResult::Timeout,
                };
            }
        };

        let mut buf = vec![0u8; 4096];

        loop {
            let elapsed = start.elapsed();
            if elapsed >= timeout {
                return StallResult::Timeout;
            }

            let remaining = timeout - elapsed;
            let check_interval = Duration::from_millis(STALL_CHECK_INTERVAL_MS);
            let wait_time = remaining.min(check_interval);

            tokio::select! {
                n = stdout.read(&mut buf) => {
                    match n {
                        Ok(0) => {
                            let remaining = timeout.saturating_sub(start.elapsed());
                            match tokio::time::timeout(remaining, child.wait()).await {
                                Ok(Ok(status)) => {
                                    let mut stderr_buf = Vec::new();
                                    if let Some(mut stderr) = child.stderr.take() {
                                        let _ = stderr.read_to_end(&mut stderr_buf).await;
                                    }
                                    return StallResult::Completed(std::process::Output {
                                        status,
                                        stdout: accumulated_stdout,
                                        stderr: stderr_buf,
                                    });
                                }
                                Ok(Err(e)) => return StallResult::Error(e.to_string()),
                                Err(_) => return StallResult::Timeout,
                            }
                        }
                        Ok(n) => {
                            let chunk = &buf[..n];
                            if let Some(ref tx) = progress_tx {
                                let message = String::from_utf8_lossy(chunk).to_string();
                                let _ = tx.send(ToolProgress {
                                    message,
                                    percent: None,
                                });
                            }
                            accumulated_stdout.extend_from_slice(chunk);
                            last_output_time = std::time::Instant::now();
                        }
                        Err(e) => return StallResult::Error(e.to_string()),
                    }
                }
                _ = tokio::time::sleep(wait_time) => {
                    let stall_duration = last_output_time.elapsed();
                    if stall_duration >= Duration::from_millis(STALL_THRESHOLD_MS) {
                        let tail_start = accumulated_stdout.len().saturating_sub(STALL_TAIL_BYTES);
                        let tail = String::from_utf8_lossy(&accumulated_stdout[tail_start..]);

                        if looks_like_interactive_prompt(&tail) {
                            return StallResult::Stalled(tail.to_string());
                        }
                    }
                }
            }
        }
    }
}
