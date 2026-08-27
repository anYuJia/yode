from pathlib import Path

path = Path("crates/yode-tools/src/builtin/codex_compat.rs")
text = path.read_text(encoding="utf-8")

old_effective = '''fn effective_exec_command(command: &str, params: &Value) -> String {
    let Some(shell) = params
        .get("shell")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return command.to_string();
    };

    let flag = if params
        .get("login")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "-lc"
    } else {
        "-c"
    };
    format!("{} {} {}", shell_quote(shell), flag, shell_quote(command))
}
'''
new_effective = '''fn effective_exec_command(command: &str, params: &Value) -> String {
    let Some(shell) = params
        .get("shell")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return command.to_string();
    };

    #[cfg(target_os = "windows")]
    {
        let executable = shell
            .replace('\\\\', "/")
            .rsplit('/')
            .next()
            .unwrap_or(shell)
            .to_ascii_lowercase();
        if matches!(executable.as_str(), "cmd" | "cmd.exe") {
            return command.to_string();
        }
    }

    let flag = if params
        .get("login")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        "-lc"
    } else {
        "-c"
    };
    format!("{} {} {}", shell_quote(shell), flag, shell_quote(command))
}
'''
if old_effective not in text:
    raise SystemExit("effective_exec_command block not found")
text = text.replace(old_effective, new_effective, 1)

marker = '''    use super::{
        ApplyPatchTool, ExecCommandTool, GetContextRemainingTool, RequestUserInputTool,
        ShellCommandTool, UpdatePlanTool, ViewImageTool, WriteStdinTool,
    };
'''
helpers = marker + '''
    #[cfg(target_os = "windows")]
    fn codex_shell_test_case() -> (&'static str, &'static str) {
        ("echo shell-ok", "cmd")
    }

    #[cfg(not(target_os = "windows"))]
    fn codex_shell_test_case() -> (&'static str, &'static str) {
        ("printf shell-ok", "sh")
    }

    #[cfg(target_os = "windows")]
    fn background_stdin_command() -> &'static str {
        "setlocal EnableDelayedExpansion & echo ready & set /p line= & echo got:!line!"
    }

    #[cfg(not(target_os = "windows"))]
    fn background_stdin_command() -> &'static str {
        "printf 'ready\\n'; IFS= read line; printf 'got:%s\\n' \"$line\""
    }
'''
if marker not in text:
    raise SystemExit("test import marker not found")
text = text.replace(marker, helpers, 1)

fn_line = '''    async fn exec_command_accepts_codex_shell_options() {
        let result = ExecCommandTool
'''
fn_replacement = '''    async fn exec_command_accepts_codex_shell_options() {
        let (command, shell) = codex_shell_test_case();
        let result = ExecCommandTool
'''
if fn_line not in text:
    raise SystemExit("shell options test function marker not found")
text = text.replace(fn_line, fn_replacement, 1)

shell_pair = '''                    "cmd": "printf shell-ok",
                    "shell": "sh",
'''
shell_pair_replacement = '''                    "cmd": command,
                    "shell": shell,
'''
if shell_pair not in text:
    raise SystemExit("shell options command pair not found")
text = text.replace(shell_pair, shell_pair_replacement, 1)

background_command = '''                    "cmd": "printf 'ready\\\\n'; IFS= read line; printf 'got:%s\\\\n' \\\"$line\\\"",
'''
count = text.count(background_command)
if count != 2:
    raise SystemExit(f"expected 2 background stdin commands, found {count}")
text = text.replace(background_command, '''                    "cmd": background_stdin_command(),
''')

path.write_text(text, encoding="utf-8")
