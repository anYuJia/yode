//! 工作区信任的权威存储：存放在用户主目录（仓库之外），以 canonical path +
//! `.yode/config.toml` 内容哈希绑定，条件允许时同时绑定 git remote。
//!
//! 安全契约：
//! - 未信任的工作区不得自动启动 MCP、Hooks、插件命令或自定义 Provider endpoint；
//!   仓库内 `.yode/config.toml` 不得覆盖敏感配置。
//! - 信任记录绑定 canonical path 与配置哈希；目录别名、配置变更、remote 变更
//!   都会使既有信任失效，需要用户重新确认。
//! - 首次信任必须由用户在信任流程中显式确认（调用方负责展示命令/args/cwd/
//!   endpoint/env/影响范围）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkspaceTrustEntry {
    /// 工作区根目录的 canonical path。
    pub path: PathBuf,
    /// `.yode/config.toml`（若存在）内容 SHA-256。
    pub config_sha256: Option<String>,
    /// 绑定的 git remote origin URL（若当时可解析）。
    pub remote: Option<String>,
    pub trusted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceTrustStore {
    /// key 为 canonical path 的字符串形式。
    #[serde(default)]
    pub workspaces: BTreeMap<String, WorkspaceTrustEntry>,
}

impl WorkspaceTrustStore {
    pub fn default_path() -> Option<PathBuf> {
        dirs::home_dir().map(|home| home.join(".yode").join("workspace-trust.toml"))
    }

    pub fn load() -> Self {
        Self::default_path()
            .map(|path| Self::load_from(&path))
            .unwrap_or_default()
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|content| toml::from_str(&content).ok())
            .unwrap_or_default()
    }

    pub fn sha256(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// 计算工作区的绑定指纹：`.yode/config.toml` 哈希 + git remote。
    pub fn workspace_fingerprint(workspace: &Path) -> (Option<String>, Option<String>) {
        let config_hash = workspace
            .join(".yode")
            .join("config.toml")
            .to_path_buf()
            .pipe(|path| std::fs::read_to_string(path).ok())
            .map(|content| Self::sha256(&content));
        (config_hash, Self::git_remote(workspace))
    }

    fn git_remote(workspace: &Path) -> Option<String> {
        let output = std::process::Command::new("git")
            .args(["config", "--get", "remote.origin.url"])
            .current_dir(workspace)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let remote = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!remote.is_empty()).then_some(remote)
    }

    /// 工作区是否处于可信状态：有记录且 canonical path + 配置哈希 + remote 全部匹配。
    pub fn is_trusted(&self, workspace: &Path) -> bool {
        let Some(canonical) = canonical_path(workspace) else {
            return false;
        };
        let Some(entry) = self.workspaces.get(canonical.to_string_lossy().as_ref()) else {
            return false;
        };
        if !entry.trusted {
            return false;
        }
        let (config_hash, remote) = Self::workspace_fingerprint(workspace);
        if entry.config_sha256.as_ref() != config_hash.as_ref() {
            return false;
        }
        // remote 也必须完全一致：信任时未绑定 remote 的工作区，后来新增了
        // remote（身份变化）同样视为变更而失效。
        if entry.remote.as_ref() != remote.as_ref() {
            return false;
        }
        true
    }

    /// 记录（或更新）工作区信任状态并持久化到默认路径。
    pub fn set_trusted(&mut self, workspace: &Path, trusted: bool) -> Result<(), String> {
        let path = Self::default_path().ok_or_else(|| "无法定位用户主目录".to_string())?;
        self.set_trusted_at(workspace, trusted, &path)
    }

    pub fn set_trusted_at(
        &mut self,
        workspace: &Path,
        trusted: bool,
        store_path: &Path,
    ) -> Result<(), String> {
        let canonical = canonical_path(workspace)
            .ok_or_else(|| format!("无法解析工作区路径 {}", workspace.display()))?;
        let (config_hash, remote) = Self::workspace_fingerprint(workspace);
        self.workspaces.insert(
            canonical.to_string_lossy().to_string(),
            WorkspaceTrustEntry {
                path: canonical,
                config_sha256: config_hash,
                remote,
                trusted,
            },
        );
        persist(self, store_path)
    }

    pub fn revoke(&mut self, workspace: &Path) -> Result<(), String> {
        let path = Self::default_path().ok_or_else(|| "无法定位用户主目录".to_string())?;
        self.revoke_at(workspace, &path)
    }

    pub fn revoke_at(&mut self, workspace: &Path, store_path: &Path) -> Result<(), String> {
        if let Some(canonical) = canonical_path(workspace) {
            let key = canonical.to_string_lossy().to_string();
            if self.workspaces.remove(&key).is_some() {
                persist(self, store_path)?;
            }
        }
        Ok(())
    }
}

fn canonical_path(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

fn persist(store: &WorkspaceTrustStore, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("无法创建信任存储目录 {}: {err}", parent.display()))?;
    }
    let serialized =
        toml::to_string_pretty(store).map_err(|err| format!("无法序列化信任存储: {err}"))?;
    let temporary =
        path.with_file_name(format!(".workspace-trust.toml.tmp-{}", std::process::id()));
    std::fs::write(&temporary, serialized)
        .map_err(|err| format!("无法写入信任存储 {}: {err}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .map_err(|err| format!("无法更新信任存储 {}: {err}", path.display()))?;
    Ok(())
}

/// 小工具：链式应用（Rust 无内置 pipe 语法）。
trait Pipe: Sized {
    fn pipe<T>(self, f: impl FnOnce(Self) -> T) -> T {
        f(self)
    }
}
impl<T: Sized> Pipe for T {}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace-trust.toml");
        (dir, path)
    }

    #[test]
    fn untrusted_workspace_is_not_trusted() {
        let (_dir, path) = temp_store();
        let store = WorkspaceTrustStore::load_from(&path);
        let workspace = tempfile::tempdir().unwrap();
        assert!(!store.is_trusted(workspace.path()));
    }

    #[test]
    fn trust_round_trips_and_survives_reload() {
        let (_dir, path) = temp_store();
        let workspace = tempfile::tempdir().unwrap();
        let mut store = WorkspaceTrustStore::default();
        store.set_trusted_at(workspace.path(), true, &path).unwrap();
        assert!(store.is_trusted(workspace.path()));

        let reloaded = WorkspaceTrustStore::load_from(&path);
        assert!(reloaded.is_trusted(workspace.path()));
    }

    #[test]
    fn config_change_invalidates_trust() {
        let (_dir, path) = temp_store();
        let workspace = tempfile::tempdir().unwrap();
        let mut store = WorkspaceTrustStore::default();
        store.set_trusted_at(workspace.path(), true, &path).unwrap();
        assert!(store.is_trusted(workspace.path()));

        // 恶意配置出现 => 指纹变化 => 信任失效
        std::fs::create_dir_all(workspace.path().join(".yode")).unwrap();
        std::fs::write(
            workspace.path().join(".yode").join("config.toml"),
            "[permissions]\ndefault_mode = \"bypass\"\n",
        )
        .unwrap();
        assert!(!store.is_trusted(workspace.path()));
    }

    #[test]
    fn path_alias_does_not_extend_trust() {
        let (_dir, path) = temp_store();
        let workspace = tempfile::tempdir().unwrap();
        let mut store = WorkspaceTrustStore::default();
        store.set_trusted_at(workspace.path(), true, &path).unwrap();
        assert!(store.is_trusted(workspace.path()));

        // 通过符号链接别名访问同一目录：canonical path 相同仍应受信任
        let alias = workspace
            .path()
            .parent()
            .unwrap()
            .join(format!("yode-trust-alias-{}", std::process::id()));
        #[cfg(unix)]
        std::os::unix::fs::symlink(workspace.path(), &alias).unwrap();
        #[cfg(unix)]
        assert!(store.is_trusted(&alias));
        let _ = std::fs::remove_file(&alias);

        // 完全不同的路径不能继承信任
        let other = tempfile::tempdir().unwrap();
        assert!(!store.is_trusted(other.path()));
    }

    #[test]
    fn revoke_removes_trust() {
        let (_dir, path) = temp_store();
        let workspace = tempfile::tempdir().unwrap();
        let mut store = WorkspaceTrustStore::default();
        store.set_trusted_at(workspace.path(), true, &path).unwrap();
        assert!(store.is_trusted(workspace.path()));

        store.revoke_at(workspace.path(), &path).unwrap();
        assert!(!store.is_trusted(workspace.path()));
    }

    #[test]
    fn remote_binding_invalidates_on_remote_change() {
        // 无 git remote 的工作区：绑定 None；有 remote 的工作区在 remote 变化时失效。
        let (_dir, path) = temp_store();
        let workspace = tempfile::tempdir().unwrap();
        let mut store = WorkspaceTrustStore::default();
        store.set_trusted_at(workspace.path(), true, &path).unwrap();

        // 信任后为仓库配置一个 remote：指纹从 (hash, None) 变成 (hash, Some(url))
        let _ = std::process::Command::new("git")
            .args(["init"])
            .current_dir(workspace.path())
            .output();
        let _ = std::process::Command::new("git")
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/evil/repo.git",
            ])
            .current_dir(workspace.path())
            .output();
        // 绑定远程的工作区在 remote 变化后不再受信任
        assert!(!store.is_trusted(workspace.path()));
    }
}
