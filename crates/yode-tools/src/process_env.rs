//! 子进程最小环境构造器。
//!
//! 安全契约：Hook、stdio MCP 等由仓库内容驱动的子进程一律 `env_clear()`，
//! 只放行白名单基础变量 + 调用方显式声明的变量，防止父进程的 API key、
//! 凭据、代理配置等通过环境泄漏给仓库可控的代码。

use tokio::process::Command;

/// 基础白名单：子进程解析外部程序、定位用户目录、临时目录和本地化所必需。
///
/// Windows 的系统根目录、用户 profile、程序目录和临时目录属于 shell/Win32
/// 运行时的核心启动上下文，不是应用凭据。`cmd.exe` 与部分 Win32 运行时在
/// `env_clear()` 后缺少其中部分变量时可能无法初始化，因此保留一组最小但完整
/// 的 core env。这里仍然不会继承 API key、token、代理、云凭据等敏感变量。
pub const MINIMAL_ENV_ALLOWLIST: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    "TMP",
    "TEMP",
    // Windows core path/shell bootstrap.
    "PATHEXT",
    "SHELL",
    "COMSPEC",
    "SystemRoot",
    "WINDIR",
    "SYSTEMDRIVE",
    // Windows user/profile context required by Win32 APIs.
    "USERNAME",
    "USERDOMAIN",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    // Program/data roots used while resolving Windows runtime components.
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMW6432",
    "PROGRAMDATA",
    "APPDATA",
    "LOCALAPPDATA",
    // Optional shell-discovery hints; values are paths, not credentials.
    "POWERSHELL",
    "PWSH",
];

/// 清空继承环境并只放行白名单变量。调用方随后可以 `.env(...)` 显式补充
/// 用户明确授权的变量。
pub fn apply_minimal_env(cmd: &mut Command) {
    cmd.env_clear();
    for key in MINIMAL_ENV_ALLOWLIST {
        if let Ok(value) = std::env::var(key) {
            cmd.env(key, value);
        }
    }

    // PATHEXT 在 Windows 命令解析中属于核心语义。极简/测试宿主偶尔不提供它，
    // 使用系统默认值即可恢复 .EXE/.CMD/.BAT 解析，且不会扩大凭据暴露面。
    #[cfg(windows)]
    if std::env::var_os("PATHEXT").is_none() {
        cmd.env("PATHEXT", ".COM;.EXE;.BAT;.CMD");
    }
}

/// 让子进程运行在新的进程组（Unix）。这样终止时可以通过进程组 ID 一起
/// 杀死全部后代进程，避免孙进程/管道兄弟进程残留。
pub fn spawn_in_new_process_group(cmd: &mut Command) {
    #[cfg(unix)]
    {
        cmd.process_group(0);
    }
    #[cfg(not(unix))]
    {
        let _ = cmd;
    }
}

/// 终止整个进程组（Unix）并等待回收。返回是否成功终止。
pub async fn kill_process_group(child: &mut tokio::process::Child) -> Result<(), String> {
    #[cfg(unix)]
    {
        let pid = child.id().ok_or_else(|| "子进程已不存在".to_string())?;
        // 向进程组发送 SIGTERM，随后 SIGKILL 兜底
        unsafe {
            libc::kill(-(pid as libc::pid_t), libc::SIGTERM);
        }
        let _ = child.wait().await;
        unsafe {
            libc::kill(-(pid as libc::pid_t), libc::SIGKILL);
        }
        let _ = child.wait().await;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        child
            .kill()
            .await
            .map_err(|err| format!("Failed to kill child: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn minimal_env_clears_inherited_variables() {
        std::env::set_var("YODE_SECRET_TEST_TOKEN", "super-secret");

        #[cfg(windows)]
        let mut cmd = Command::new("cmd.exe");
        #[cfg(not(windows))]
        let mut cmd = Command::new("sh");
        apply_minimal_env(&mut cmd);
        #[cfg(windows)]
        cmd.args([
            "/d",
            "/s",
            "/c",
            "if defined YODE_SECRET_TEST_TOKEN (echo leaked) else (echo unset) && if defined PATH echo path-ok",
        ]);
        #[cfg(not(windows))]
        cmd.arg("-c")
            .arg(r#"printf '%s' "${YODE_SECRET_TEST_TOKEN:-unset} ${PATH:-no-path}" "$@""#)
            .arg("sh");

        let output = cmd.output().await.unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();

        // 未授权的变量必须被清空
        assert!(
            stdout.starts_with("unset"),
            "secret leaked into child env: {stdout}"
        );
        // 白名单变量（PATH）保留
        #[cfg(windows)]
        assert!(
            stdout.contains("path-ok"),
            "whitelisted PATH was not preserved: {stdout}"
        );
        #[cfg(not(windows))]
        assert!(
            stdout.contains('/'),
            "whitelisted PATH was not preserved: {stdout}"
        );

        std::env::remove_var("YODE_SECRET_TEST_TOKEN");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn minimal_env_keeps_windows_cmd_bootstrap() {
        let mut cmd = Command::new("cmd.exe");
        apply_minimal_env(&mut cmd);
        cmd.args(["/d", "/s", "/c", "echo ok"]);

        let output = cmd.output().await.expect("cmd should start");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "cmd failed under minimal environment: status={} stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("ok"),
            "cmd produced unexpected stdout: {stdout:?}"
        );
    }
}

#[cfg(all(test, unix))]
mod process_group_tests {
    use super::{kill_process_group, spawn_in_new_process_group};

    /// CANCEL-001：取消时整个进程组（含孙进程）被终止并回收。
    #[tokio::test]
    async fn kill_process_group_terminates_grandchildren() {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg("sleep 300 & wait")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        spawn_in_new_process_group(&mut cmd);
        let mut child = cmd.spawn().unwrap();
        let pid = child.id().unwrap();

        // 等待孙进程（sleep）出现
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        kill_process_group(&mut child).await.unwrap();
        let _ = child.wait().await;

        // 组内不应残留任何进程（主进程与孙进程都被回收）。
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        let probe = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("ps -o pid= -g {} 2>/dev/null | wc -l", pid))
            .output()
            .unwrap();
        let count: i64 = String::from_utf8_lossy(&probe.stdout)
            .trim()
            .parse()
            .unwrap_or(-1);
        assert_eq!(
            count, 0,
            "process group {pid} still has {count} live processes after kill"
        );
    }
}
