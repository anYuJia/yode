use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use crate::tool::ToolProgress;

/// Helper to run a command and stream its output as tool progress.
pub async fn run_command_with_progress(
    mut cmd: Command,
    timeout: Duration,
    progress_tx: Option<mpsc::UnboundedSender<ToolProgress>>,
) -> Result<(std::process::ExitStatus, String, String), String> {
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn process: {}", e))?;
    
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let mut stdout_reader = BufReader::new(stdout).lines();
    let mut stderr_reader = BufReader::new(stderr).lines();

    let mut out_stdout = Vec::new();
    let mut out_stderr = Vec::new();

    let timeout_fut = tokio::time::sleep(timeout);
    tokio::pin!(timeout_fut);

    loop {
        tokio::select! {
            line = stdout_reader.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        if let Some(ref tx) = progress_tx {
                            let _ = tx.send(ToolProgress {
                                message: l.clone(),
                                percent: None,
                            });
                        }
                        out_stdout.push(l);
                    }
                    Ok(None) => {}
                    Err(_) => break Err("Failed to read stdout".to_string()),
                }
            }
            line = stderr_reader.next_line() => {
                match line {
                    Ok(Some(l)) => {
                        if let Some(ref tx) = progress_tx {
                            let _ = tx.send(ToolProgress {
                                message: format!("[stderr] {}", l),
                                percent: None,
                            });
                        }
                        out_stderr.push(l);
                    }
                    Ok(None) => {}
                    Err(_) => break Err("Failed to read stderr".to_string()),
                }
            }
            _ = &mut timeout_fut => {
                let _ = child.kill().await;
                return Err(format!("Command timed out after {} seconds", timeout.as_secs()));
            }
            status = child.wait() => {
                let status = status.map_err(|e| format!("Process wait failed: {}", e))?;
                
                // Read remaining output
                while let Ok(Some(l)) = stdout_reader.next_line().await {
                    out_stdout.push(l);
                }
                while let Ok(Some(l)) = stderr_reader.next_line().await {
                    out_stderr.push(l);
                }

                return Ok((status, out_stdout.join("\n"), out_stderr.join("\n")));
            }
        }
    }
}
