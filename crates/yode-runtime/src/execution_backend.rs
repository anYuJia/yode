use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::process::Command;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionBackendKind {
    Local,
    Docker,
    Ssh,
    Cloud,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionBackendCapabilities {
    pub isolated_filesystem: bool,
    pub isolated_processes: bool,
    pub remote: bool,
    pub supports_network_control: bool,
    pub supports_workspace_mount: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionRequest {
    pub command: String,
    pub workspace: PathBuf,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

fn default_timeout() -> u64 { 600 }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub backend: ExecutionBackendKind,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub metadata: Value,
}

impl ExecutionResult {
    pub fn success(&self) -> bool { self.exit_code == 0 }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendAvailability {
    pub kind: ExecutionBackendKind,
    pub available: bool,
    pub detail: String,
}

#[async_trait]
pub trait ExecutionBackend: Send + Sync {
    fn kind(&self) -> ExecutionBackendKind;
    fn capabilities(&self) -> ExecutionBackendCapabilities;
    async fn availability(&self) -> BackendAvailability;
    async fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionResult>;
}

#[derive(Debug, Clone, Default)]
pub struct LocalExecutionBackend;

#[async_trait]
impl ExecutionBackend for LocalExecutionBackend {
    fn kind(&self) -> ExecutionBackendKind { ExecutionBackendKind::Local }

    fn capabilities(&self) -> ExecutionBackendCapabilities {
        ExecutionBackendCapabilities {
            isolated_filesystem: false,
            isolated_processes: false,
            remote: false,
            supports_network_control: false,
            supports_workspace_mount: true,
        }
    }

    async fn availability(&self) -> BackendAvailability {
        BackendAvailability { kind: self.kind(), available: true, detail: "local process execution available".to_string() }
    }

    async fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionResult> {
        ensure_workspace(&request.workspace)?;
        let start = Instant::now();
        let mut command = platform_shell(&request.command);
        command.current_dir(&request.workspace).envs(&request.env);
        let output = timeout_output(command, request.timeout_secs).await?;
        Ok(to_result(self.kind(), output, start, json!({"workspace":request.workspace})))
    }
}

#[derive(Debug, Clone)]
pub struct DockerExecutionBackend {
    pub image: String,
    pub network: String,
    pub read_only_root: bool,
}

impl DockerExecutionBackend {
    pub fn new(image: impl Into<String>) -> Result<Self> {
        let image = image.into();
        validate_docker_image(&image)?;
        Ok(Self { image, network: "none".to_string(), read_only_root: true })
    }
}

#[async_trait]
impl ExecutionBackend for DockerExecutionBackend {
    fn kind(&self) -> ExecutionBackendKind { ExecutionBackendKind::Docker }

    fn capabilities(&self) -> ExecutionBackendCapabilities {
        ExecutionBackendCapabilities {
            isolated_filesystem: true,
            isolated_processes: true,
            remote: false,
            supports_network_control: true,
            supports_workspace_mount: true,
        }
    }

    async fn availability(&self) -> BackendAvailability {
        command_availability(self.kind(), "docker", &["version", "--format", "{{.Server.Version}}"] ).await
    }

    async fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionResult> {
        ensure_workspace(&request.workspace)?;
        validate_docker_image(&self.image)?;
        if !matches!(self.network.as_str(), "none" | "bridge" | "host") {
            bail!("unsupported Docker network policy '{}'; use none, bridge, or host", self.network);
        }
        let workspace = request.workspace.canonicalize().unwrap_or_else(|_| request.workspace.clone());
        let mut args = vec![
            "run".to_string(), "--rm".to_string(),
            "--network".to_string(), self.network.clone(),
            "--mount".to_string(), format!("type=bind,src={},dst=/workspace", workspace.display()),
            "--workdir".to_string(), "/workspace".to_string(),
        ];
        if self.read_only_root {
            args.extend(["--read-only".to_string(), "--tmpfs".to_string(), "/tmp:rw,nosuid,nodev".to_string()]);
        }
        for (key, value) in &request.env {
            validate_env_key(key)?;
            args.extend(["--env".to_string(), format!("{key}={value}")]);
        }
        args.extend([
            self.image.clone(), "sh".to_string(), "-lc".to_string(), request.command.clone()
        ]);
        let start = Instant::now();
        let mut command = Command::new("docker");
        command.args(&args);
        let output = timeout_output(command, request.timeout_secs).await?;
        Ok(to_result(self.kind(), output, start, json!({"image":self.image,"network":self.network,"workspace":workspace})))
    }
}

#[derive(Debug, Clone)]
pub struct SshExecutionBackend {
    pub target: String,
    pub remote_workspace: String,
    pub identity_file: Option<PathBuf>,
}

impl SshExecutionBackend {
    pub fn new(target: impl Into<String>, remote_workspace: impl Into<String>) -> Result<Self> {
        let target = target.into();
        validate_ssh_target(&target)?;
        let remote_workspace = remote_workspace.into();
        validate_remote_path(&remote_workspace)?;
        Ok(Self { target, remote_workspace, identity_file: None })
    }
}

#[async_trait]
impl ExecutionBackend for SshExecutionBackend {
    fn kind(&self) -> ExecutionBackendKind { ExecutionBackendKind::Ssh }

    fn capabilities(&self) -> ExecutionBackendCapabilities {
        ExecutionBackendCapabilities {
            isolated_filesystem: false,
            isolated_processes: false,
            remote: true,
            supports_network_control: false,
            supports_workspace_mount: false,
        }
    }

    async fn availability(&self) -> BackendAvailability {
        command_availability(self.kind(), "ssh", &["-V"]).await
    }

    async fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionResult> {
        validate_ssh_target(&self.target)?;
        validate_remote_path(&self.remote_workspace)?;
        let mut command = Command::new("ssh");
        command.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=10"]);
        if let Some(identity) = &self.identity_file {
            command.arg("-i").arg(identity);
        }
        let remote = format!("cd {} && {}", shell_quote(&self.remote_workspace), request.command);
        command.arg(&self.target).arg(remote);
        let start = Instant::now();
        let output = timeout_output(command, request.timeout_secs).await?;
        Ok(to_result(self.kind(), output, start, json!({"target":self.target,"remote_workspace":self.remote_workspace})))
    }
}

#[derive(Debug, Clone)]
pub struct CloudExecutionBackend {
    pub endpoint: String,
    pub bearer_token: Option<String>,
}

impl CloudExecutionBackend {
    pub fn new(endpoint: impl Into<String>) -> Result<Self> {
        let endpoint = endpoint.into();
        validate_cloud_endpoint(&endpoint)?;
        Ok(Self { endpoint, bearer_token: None })
    }
}

#[async_trait]
impl ExecutionBackend for CloudExecutionBackend {
    fn kind(&self) -> ExecutionBackendKind { ExecutionBackendKind::Cloud }

    fn capabilities(&self) -> ExecutionBackendCapabilities {
        ExecutionBackendCapabilities {
            isolated_filesystem: true,
            isolated_processes: true,
            remote: true,
            supports_network_control: true,
            supports_workspace_mount: false,
        }
    }

    async fn availability(&self) -> BackendAvailability {
        let url = format!("{}/health", self.endpoint.trim_end_matches('/'));
        let client = reqwest::Client::builder().timeout(Duration::from_secs(5)).build();
        match client {
            Ok(client) => match client.get(&url).send().await {
                Ok(response) if response.status().is_success() => BackendAvailability { kind:self.kind(), available:true, detail:format!("cloud endpoint reachable: {}", self.endpoint) },
                Ok(response) => BackendAvailability { kind:self.kind(), available:false, detail:format!("cloud health returned {}", response.status()) },
                Err(error) => BackendAvailability { kind:self.kind(), available:false, detail:error.to_string() },
            },
            Err(error) => BackendAvailability { kind:self.kind(), available:false, detail:error.to_string() },
        }
    }

    async fn execute(&self, request: &ExecutionRequest) -> Result<ExecutionResult> {
        validate_cloud_endpoint(&self.endpoint)?;
        let url = format!("{}/v1/execute", self.endpoint.trim_end_matches('/'));
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(request.timeout_secs.clamp(1, 3600)))
            .build()?;
        let mut builder = client.post(url).json(&json!({
            "command": request.command,
            "workspace": request.workspace,
            "timeout_secs": request.timeout_secs,
            "env": request.env,
        }));
        if let Some(token) = self.bearer_token.as_deref().filter(|token| !token.trim().is_empty()) {
            builder = builder.bearer_auth(token);
        }
        let start = Instant::now();
        let response = builder.send().await?.error_for_status()?;
        let mut result: ExecutionResult = response.json().await.context("invalid cloud execution response")?;
        result.backend = self.kind();
        if result.duration_ms == 0 { result.duration_ms = start.elapsed().as_millis() as u64; }
        if result.metadata.is_null() { result.metadata = json!({}); }
        Ok(result)
    }
}

pub async fn detect_standard_backends() -> Vec<BackendAvailability> {
    let local = LocalExecutionBackend;
    let docker = DockerExecutionBackend::new("alpine:3.20").expect("static Docker image is valid");
    let local_result = local.availability().await;
    let docker_result = docker.availability().await;
    let ssh_result = command_availability(ExecutionBackendKind::Ssh, "ssh", &["-V"]).await;
    vec![local_result, docker_result, ssh_result]
}

async fn timeout_output(mut command: Command, timeout_secs: u64) -> Result<std::process::Output> {
    let child = command.spawn().context("failed to spawn execution backend process")?;
    tokio::time::timeout(Duration::from_secs(timeout_secs.clamp(1, 3600)), child.wait_with_output())
        .await
        .context("execution backend timed out")??
        .pipe(Ok)
}

trait Pipe: Sized { fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T { f(self) } }
impl<T> Pipe for T {}

fn to_result(kind: ExecutionBackendKind, output: std::process::Output, start: Instant, metadata: Value) -> ExecutionResult {
    ExecutionResult {
        backend: kind,
        exit_code: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        duration_ms: start.elapsed().as_millis() as u64,
        metadata,
    }
}

async fn command_availability(kind: ExecutionBackendKind, executable: &str, args: &[&str]) -> BackendAvailability {
    match Command::new(executable).args(args).output().await {
        Ok(output) if output.status.success() => BackendAvailability { kind, available:true, detail:format!("{executable} available") },
        Ok(output) => BackendAvailability { kind, available:false, detail:String::from_utf8_lossy(&output.stderr).trim().to_string() },
        Err(error) => BackendAvailability { kind, available:false, detail:error.to_string() },
    }
}

fn platform_shell(command: &str) -> Command {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = Command::new("cmd.exe");
        cmd.args(["/d", "/s", "/c", command]);
        cmd
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut cmd = Command::new("sh");
        cmd.args(["-lc", command]);
        cmd
    }
}

fn ensure_workspace(path: &Path) -> Result<()> {
    if !path.is_dir() { bail!("execution workspace is not a directory: {}", path.display()); }
    Ok(())
}

fn validate_docker_image(image: &str) -> Result<()> {
    if image.trim().is_empty() || image.starts_with('-') || image.chars().any(|ch| ch.is_whitespace() || ch.is_control()) {
        bail!("invalid Docker image name");
    }
    Ok(())
}

fn validate_ssh_target(target: &str) -> Result<()> {
    if target.trim().is_empty() || target.starts_with('-') || target.chars().any(|ch| ch.is_whitespace() || ch.is_control() || matches!(ch, ';' | '|' | '&' | '`' | '$')) {
        bail!("invalid SSH target");
    }
    Ok(())
}

fn validate_remote_path(path: &str) -> Result<()> {
    if path.trim().is_empty() || path.contains('\n') || path.contains('\r') || path.contains('\0') { bail!("invalid remote workspace path"); }
    Ok(())
}

fn validate_cloud_endpoint(endpoint: &str) -> Result<()> {
    if !(endpoint.starts_with("https://") || endpoint.starts_with("http://127.0.0.1:") || endpoint.starts_with("http://localhost:")) {
        bail!("cloud endpoint must use HTTPS (localhost HTTP is allowed for development)");
    }
    Ok(())
}

fn validate_env_key(key: &str) -> Result<()> {
    if key.is_empty() || !key.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') { bail!("invalid environment variable key '{key}'"); }
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_backend_identifiers() {
        assert!(DockerExecutionBackend::new("rust:1.80").is_ok());
        assert!(DockerExecutionBackend::new("--privileged").is_err());
        assert!(SshExecutionBackend::new("user@example.com", "/srv/repo").is_ok());
        assert!(SshExecutionBackend::new("-oProxyCommand=evil", "/srv/repo").is_err());
        assert!(CloudExecutionBackend::new("https://runner.example.com").is_ok());
        assert!(CloudExecutionBackend::new("http://runner.example.com").is_err());
    }

    #[tokio::test]
    async fn local_backend_executes_command() {
        let dir = tempfile::tempdir().unwrap();
        let backend = LocalExecutionBackend;
        let request = ExecutionRequest { command: "printf yode".to_string(), workspace: dir.path().to_path_buf(), timeout_secs: 5, env: BTreeMap::new() };
        let result = backend.execute(&request).await.unwrap();
        assert!(result.success());
        assert!(result.stdout.contains("yode"));
    }
}
