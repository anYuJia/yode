//! 插件信任的权威存储：存放在用户主目录（仓库之外），以 canonical path +
//! manifest 内容哈希绑定。
//!
//! 安全契约：
//! - 仓库内 plugin.toml 的 `trust`/`enabled` 字段永远不具权威性，恶意仓库
//!   不能自授信。
//! - 信任记录绑定插件目录的 canonical path 和 plugin.toml 的内容哈希；
//!   任一变化（目录移动、manifest 修改）都会使既有信任失效，需要重新授权。
//! - Blocked 状态不可被普通路径覆盖。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::plugins::PluginTrustState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginTrustEntry {
    /// 插件根目录的 canonical path。
    pub path: PathBuf,
    /// plugin.toml 内容的 SHA-256 十六进制摘要。
    pub manifest_sha256: String,
    pub trust: PluginTrustState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginTrustStore {
    /// key 为 canonical path 的字符串形式。
    #[serde(default)]
    pub plugins: BTreeMap<String, PluginTrustEntry>,
}

impl PluginTrustStore {
    /// 默认信任存储路径：`~/.yode/plugin-trust.toml`。
    pub fn default_path() -> Option<PathBuf> {
        dirs::home_dir().map(|home| home.join(".yode").join("plugin-trust.toml"))
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

    /// 计算 plugin.toml 内容摘要（十六进制）。
    pub fn manifest_sha256(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// 绑定路径与哈希的有效信任状态；记录缺失或哈希不匹配时返回 None
    /// （调用方应回退到 Installed，即需要重新授权）。
    pub fn state_for(
        &self,
        canonical_path: &Path,
        manifest_sha256: &str,
    ) -> Option<PluginTrustState> {
        self.plugins
            .get(canonical_path.to_string_lossy().as_ref())
            .and_then(|entry| {
                if entry.manifest_sha256 == manifest_sha256 {
                    Some(entry.trust)
                } else {
                    None
                }
            })
    }

    /// 写入并持久化一条信任记录。Blocked 状态无论来源如何都保持最高优先级。
    pub fn set(
        &mut self,
        canonical_path: &Path,
        manifest_sha256: String,
        trust: PluginTrustState,
    ) -> Result<(), String> {
        let key = canonical_path.to_string_lossy().to_string();
        if let Some(existing) = self.plugins.get(&key) {
            if existing.trust == PluginTrustState::Blocked
                && trust != PluginTrustState::Blocked
                && existing.manifest_sha256 == manifest_sha256
            {
                return Err("插件已被阻止（blocked），无法重新启用。".to_string());
            }
        }
        self.plugins.insert(
            key,
            PluginTrustEntry {
                path: canonical_path.to_path_buf(),
                manifest_sha256,
                trust,
            },
        );
        let path = Self::default_path().ok_or_else(|| "无法定位用户主目录".to_string())?;
        persist(self, &path)
    }

    /// 写入并持久化到指定文件（测试与桌面端可注入自定义位置）。
    pub fn set_at(
        &mut self,
        canonical_path: &Path,
        manifest_sha256: String,
        trust: PluginTrustState,
        store_path: &Path,
    ) -> Result<(), String> {
        let key = canonical_path.to_string_lossy().to_string();
        if let Some(existing) = self.plugins.get(&key) {
            if existing.trust == PluginTrustState::Blocked
                && trust != PluginTrustState::Blocked
                && existing.manifest_sha256 == manifest_sha256
            {
                return Err("插件已被阻止（blocked），无法重新启用。".to_string());
            }
        }
        self.plugins.insert(
            key,
            PluginTrustEntry {
                path: canonical_path.to_path_buf(),
                manifest_sha256,
                trust,
            },
        );
        persist(self, store_path)
    }

    pub fn remove(&mut self, canonical_path: &Path) -> Result<(), String> {
        let key = canonical_path.to_string_lossy().to_string();
        if self.plugins.remove(&key).is_some() {
            let path = Self::default_path().ok_or_else(|| "无法定位用户主目录".to_string())?;
            persist(self, &path)?;
        }
        Ok(())
    }
}

fn persist(store: &PluginTrustStore, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| format!("无法创建信任存储目录 {}: {err}", parent.display()))?;
    }
    let serialized =
        toml::to_string_pretty(store).map_err(|err| format!("无法序列化信任存储: {err}"))?;
    let temporary = path.with_file_name(format!(".plugin-trust.toml.tmp-{}", std::process::id()));
    std::fs::write(&temporary, serialized)
        .map_err(|err| format!("无法写入信任存储 {}: {err}", temporary.display()))?;
    std::fs::rename(&temporary, path)
        .map_err(|err| format!("无法更新信任存储 {}: {err}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("plugin-trust.toml");
        (dir, path)
    }

    #[test]
    fn state_for_requires_matching_path_and_hash() {
        let (_dir, path) = temp_store();
        let mut store = PluginTrustStore::default();
        store
            .set_at(
                Path::new("/repo/.yode/plugins/demo"),
                "abc123".to_string(),
                PluginTrustState::Enabled,
                &path,
            )
            .unwrap();

        assert_eq!(
            store.state_for(Path::new("/repo/.yode/plugins/demo"), "abc123"),
            Some(PluginTrustState::Enabled)
        );
        // 路径相同但哈希变化 -> 信任失效
        assert_eq!(
            store.state_for(Path::new("/repo/.yode/plugins/demo"), "changed"),
            None
        );
        // 哈希相同但路径不同 -> 不匹配
        assert_eq!(
            store.state_for(Path::new("/other/.yode/plugins/demo"), "abc123"),
            None
        );
    }

    #[test]
    fn store_round_trips_through_disk() {
        let (_dir, path) = temp_store();
        let mut store = PluginTrustStore::default();
        store
            .set_at(
                Path::new("/p"),
                "h".to_string(),
                PluginTrustState::Blocked,
                &path,
            )
            .unwrap();

        let reloaded = PluginTrustStore::load_from(&path);
        assert_eq!(
            reloaded.state_for(Path::new("/p"), "h"),
            Some(PluginTrustState::Blocked)
        );
    }

    #[test]
    fn blocked_cannot_be_re_enabled() {
        let (_dir, path) = temp_store();
        let mut store = PluginTrustStore::default();
        store
            .set_at(
                Path::new("/p"),
                "h".to_string(),
                PluginTrustState::Blocked,
                &path,
            )
            .unwrap();
        assert!(store
            .set_at(
                Path::new("/p"),
                "h".to_string(),
                PluginTrustState::Enabled,
                &path,
            )
            .is_err());
        // 但 manifest 变化后可以重新评估（重新授权路径）
        store
            .set_at(
                Path::new("/p"),
                "new-hash".to_string(),
                PluginTrustState::Enabled,
                &path,
            )
            .unwrap();
        assert_eq!(
            store.state_for(Path::new("/p"), "new-hash"),
            Some(PluginTrustState::Enabled)
        );
    }

    #[test]
    fn manifest_sha256_is_stable_and_content_sensitive() {
        let a = PluginTrustStore::manifest_sha256("name = \"demo\"");
        let b = PluginTrustStore::manifest_sha256("name = \"demo\"");
        let c = PluginTrustStore::manifest_sha256("name = \"demo2\"");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }
}
