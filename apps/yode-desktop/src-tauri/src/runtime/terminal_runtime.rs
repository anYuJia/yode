use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

use super::DesktopRuntime;
use crate::protocol::{
    TerminalExitEvent, TerminalOpenRequest, TerminalOpenResponse, TerminalOutputEvent,
    TerminalResizeRequest, TerminalRunRequest, TerminalRunResponse, TerminalWriteRequest,
};

#[derive(Debug, Clone)]
pub(super) struct TerminalSessionState {
    pub(super) cwd: PathBuf,
    pub(super) env: HashMap<String, String>,
}

pub(super) struct PtySessionState {
    pub(super) master: Box<dyn portable_pty::MasterPty + Send>,
    pub(super) writer: Box<dyn Write + Send>,
    pub(super) child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl DesktopRuntime {
    pub fn terminal_run(&self, request: TerminalRunRequest) -> Result<TerminalRunResponse> {
        let trimmed = request.command.trim();
        if trimmed.is_empty() {
            let cwd = self
                .terminal_session(&request.session_id, request.cwd.as_deref())?
                .cwd
                .display()
                .to_string();
            return Ok(TerminalRunResponse {
                output: String::new(),
                cwd,
                exit_code: 0,
            });
        }

        let mut session = self.terminal_session(&request.session_id, request.cwd.as_deref())?;
        let marker = format!("__YODE_TERMINAL_{}__", Uuid::new_v4().simple());
        let script = format!(
            "{{\n{}\n}}\n__yode_status=$?\nprintf '\\n{}STATUS:%s\\n' \"$__yode_status\"\nprintf '{}PWD:'\npwd\nprintf '{}ENV:'\nenv -0\n",
            trimmed, marker, marker, marker
        );
        let (shell, shell_args) = terminal_shell_command(&session.env);

        let mut command = std::process::Command::new(&shell);
        command.args(shell_args).arg(script);
        let output = command
            .current_dir(&session.cwd)
            .env_clear()
            .envs(&session.env)
            .output()
            .with_context(|| {
                format!(
                    "failed to run terminal command '{}' with shell '{}'",
                    trimmed,
                    shell.display()
                )
            })?;

        let (stdout, cwd, env, exit_code) = parse_terminal_run_stdout(
            &output.stdout,
            &marker,
            &session.cwd,
            &session.env,
            output.status.code().unwrap_or(1),
        );
        session.cwd = cwd;
        session.env = env;
        self.terminal_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal session lock poisoned"))?
            .insert(request.session_id, session.clone());

        let mut text = stdout;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.trim().is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(stderr.trim_end());
        }
        if text.is_empty() && exit_code != 0 {
            text.push_str("命令执行失败，无输出。");
        }

        Ok(TerminalRunResponse {
            output: text,
            cwd: session.cwd.display().to_string(),
            exit_code,
        })
    }

    pub fn terminal_close(&self, session_id: String) -> Result<()> {
        self.terminal_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal session lock poisoned"))?
            .remove(&session_id);
        if let Some(mut session) = self
            .pty_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("pty session lock poisoned"))?
            .remove(&session_id)
        {
            let _ = session.child.kill();
            let _ = session.child.wait();
        }
        Ok(())
    }

    pub fn terminal_open(
        &self,
        app: AppHandle,
        request: TerminalOpenRequest,
    ) -> Result<TerminalOpenResponse> {
        if self
            .pty_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("pty session lock poisoned"))?
            .contains_key(&request.session_id)
        {
            return Ok(TerminalOpenResponse {
                session_id: request.session_id,
            });
        }

        let cwd = request
            .cwd
            .as_deref()
            .and_then(valid_terminal_cwd)
            .unwrap_or_else(|| self.workspace_path.clone());
        let env: HashMap<String, String> = std::env::vars().collect();
        let (shell, _shell_args) = terminal_shell_command(&env);
        let pty_system = portable_pty::native_pty_system();
        let pair = pty_system
            .openpty(portable_pty::PtySize {
                rows: request.rows.unwrap_or(24).max(1),
                cols: request.cols.unwrap_or(80).max(1),
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open pty")?;
        let mut command = portable_pty::CommandBuilder::new(shell);
        command.cwd(cwd);
        for (key, value) in env {
            command.env(key, value);
        }
        apply_terminal_color_env(&mut command);

        let child = pair
            .slave
            .spawn_command(command)
            .context("failed to spawn shell")?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone pty reader")?;
        let writer = pair
            .master
            .take_writer()
            .context("failed to take pty writer")?;
        let session_id = request.session_id.clone();
        let sessions = Arc::clone(&self.pty_sessions);
        let app_for_output = app.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(n) => {
                        let data = String::from_utf8_lossy(&buffer[..n]).to_string();
                        let _ = app_for_output.emit(
                            "terminal-output",
                            TerminalOutputEvent {
                                session_id: session_id.clone(),
                                data,
                            },
                        );
                    }
                    Err(_) => break,
                }
            }

            if let Ok(mut sessions) = sessions.lock() {
                sessions.remove(&session_id);
            }
            let _ = app.emit(
                "terminal-exit",
                TerminalExitEvent {
                    session_id,
                    exit_code: None,
                },
            );
        });

        self.pty_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("pty session lock poisoned"))?
            .insert(
                request.session_id.clone(),
                PtySessionState {
                    master: pair.master,
                    writer,
                    child,
                },
            );

        Ok(TerminalOpenResponse {
            session_id: request.session_id,
        })
    }

    pub fn terminal_write(&self, request: TerminalWriteRequest) -> Result<()> {
        let mut sessions = self
            .pty_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("pty session lock poisoned"))?;
        let session = sessions
            .get_mut(&request.session_id)
            .ok_or_else(|| anyhow::anyhow!("terminal session not found"))?;
        session.writer.write_all(request.data.as_bytes())?;
        session.writer.flush()?;
        Ok(())
    }

    pub fn terminal_resize(&self, request: TerminalResizeRequest) -> Result<()> {
        let sessions = self
            .pty_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("pty session lock poisoned"))?;
        let session = sessions
            .get(&request.session_id)
            .ok_or_else(|| anyhow::anyhow!("terminal session not found"))?;
        session.master.resize(portable_pty::PtySize {
            rows: request.rows.max(1),
            cols: request.cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    fn terminal_session(
        &self,
        session_id: &str,
        initial_cwd: Option<&str>,
    ) -> Result<TerminalSessionState> {
        let mut sessions = self
            .terminal_sessions
            .lock()
            .map_err(|_| anyhow::anyhow!("terminal session lock poisoned"))?;
        Ok(sessions
            .entry(session_id.to_string())
            .or_insert_with(|| TerminalSessionState {
                cwd: initial_cwd
                    .and_then(valid_terminal_cwd)
                    .unwrap_or_else(|| self.workspace_path.clone()),
                env: std::env::vars().collect(),
            })
            .clone())
    }
}

pub(super) fn valid_terminal_cwd(raw: &str) -> Option<PathBuf> {
    let path = PathBuf::from(raw);
    if path.is_dir() {
        Some(path)
    } else {
        None
    }
}

pub(super) fn terminal_shell_command(
    env: &HashMap<String, String>,
) -> (PathBuf, Vec<&'static str>) {
    let shell = env
        .get("SHELL")
        .filter(|shell| !shell.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/bin/sh"));
    let shell_name = shell
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    if shell_name.contains("zsh") || shell_name.contains("bash") {
        (shell, vec!["-lic"])
    } else {
        (PathBuf::from("/bin/sh"), vec!["-lc"])
    }
}

pub(super) fn apply_terminal_color_env(command: &mut portable_pty::CommandBuilder) {
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("CLICOLOR", "1");
    command.env("FORCE_COLOR", "1");
    command.env("GREP_COLORS", "mt=01;35:fn=36:ln=32:se=2");
}

pub(super) fn parse_terminal_run_stdout(
    stdout: &[u8],
    marker: &str,
    fallback_cwd: &std::path::Path,
    fallback_env: &HashMap<String, String>,
    fallback_exit_code: i32,
) -> (String, PathBuf, HashMap<String, String>, i32) {
    let status_marker = format!("\n{}STATUS:", marker).into_bytes();
    let Some(status_start) = find_bytes(stdout, &status_marker) else {
        return (
            String::from_utf8_lossy(stdout).trim_end().to_string(),
            fallback_cwd.to_path_buf(),
            fallback_env.clone(),
            fallback_exit_code,
        );
    };

    let visible_stdout = String::from_utf8_lossy(&stdout[..status_start])
        .trim_end_matches('\n')
        .to_string();
    let status_value_start = status_start + status_marker.len();
    let status_end = stdout[status_value_start..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map(|offset| status_value_start + offset)
        .unwrap_or(stdout.len());
    let exit_code = String::from_utf8_lossy(&stdout[status_value_start..status_end])
        .trim()
        .parse::<i32>()
        .unwrap_or(fallback_exit_code);

    let pwd_marker = format!("{}PWD:", marker).into_bytes();
    let env_marker = format!("{}ENV:", marker).into_bytes();
    let pwd_start =
        find_bytes_from(stdout, &pwd_marker, status_end).map(|idx| idx + pwd_marker.len());
    let env_start = find_bytes_from(stdout, &env_marker, status_end);

    let cwd = pwd_start
        .and_then(|start| {
            let end = stdout[start..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map(|offset| start + offset)
                .unwrap_or(stdout.len());
            let path = String::from_utf8_lossy(&stdout[start..end])
                .trim()
                .to_string();
            if path.is_empty() {
                None
            } else {
                Some(PathBuf::from(path))
            }
        })
        .unwrap_or_else(|| fallback_cwd.to_path_buf());

    let env = env_start
        .map(|start| parse_null_delimited_env(&stdout[start + env_marker.len()..]))
        .filter(|env| !env.is_empty())
        .unwrap_or_else(|| fallback_env.clone());

    (visible_stdout, cwd, env, exit_code)
}

fn parse_null_delimited_env(bytes: &[u8]) -> HashMap<String, String> {
    bytes
        .split(|byte| *byte == 0)
        .filter_map(|entry| {
            if entry.is_empty() {
                return None;
            }
            let eq = entry.iter().position(|byte| *byte == b'=')?;
            let key = String::from_utf8_lossy(&entry[..eq]).to_string();
            let value = String::from_utf8_lossy(&entry[eq + 1..]).to_string();
            Some((key, value))
        })
        .collect()
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    find_bytes_from(haystack, needle, 0)
}

fn find_bytes_from(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() || start >= haystack.len() {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|offset| start + offset)
}
