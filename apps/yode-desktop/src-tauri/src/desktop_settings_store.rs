use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{Context, Result};

use yode_core::config::Config;

use crate::protocol::DesktopSettingsStatus;

static DESKTOP_SETTINGS_UPDATE_LOCK: Mutex<()> = Mutex::new(());

pub(super) fn desktop_settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".yode")
        .join("desktop-settings.json")
}

pub(super) async fn read_desktop_settings_async(
) -> Result<serde_json::Map<String, serde_json::Value>> {
    read_desktop_settings_from_path_async(&desktop_settings_path()).await
}

async fn read_desktop_settings_from_path_async(
    path: &Path,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    if !tokio::fs::try_exists(&path).await? {
        return Ok(serde_json::Map::new());
    }
    // 严格解析：无效 JSON / 非对象根节点必须报错，由设置页明确提示，
    // 绝不静默回退默认值让用户误以为设置已加载。
    parse_desktop_settings_strict(&tokio::fs::read(path).await?)
}

/// 检查桌面设置文件加载状态。文件不存在视为合法的空设置；
/// 无效 JSON、非对象根节点或不可读文件返回 `loaded: false` 与中文错误说明。
pub(super) fn desktop_settings_status() -> Result<DesktopSettingsStatus> {
    let path = desktop_settings_path();
    Ok(desktop_settings_status_at(&path))
}

fn desktop_settings_status_at(path: &Path) -> DesktopSettingsStatus {
    let base = DesktopSettingsStatus {
        loaded: false,
        path: path.display().to_string(),
        error: None,
        backup_path: None,
    };
    match std::fs::read(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => DesktopSettingsStatus {
            loaded: true,
            ..base
        },
        Err(error) => DesktopSettingsStatus {
            error: Some(format!("无法读取设置文件：{error}")),
            ..base
        },
        Ok(raw) => match parse_desktop_settings_strict(&raw) {
            Ok(_) => DesktopSettingsStatus {
                loaded: true,
                ..base
            },
            Err(error) => DesktopSettingsStatus {
                error: Some(error.to_string()),
                ..base
            },
        },
    }
}

/// 用户显式恢复损坏的设置文件。
///
/// 顺序保证：在同一锁（进程内锁 + 核心层文件锁）保护下，先创建唯一、私有权限的
/// 备份副本（原文件保留在原路径，拷贝是只读操作），随后才原子替换为空 JSON——
/// 替换失败时原路径及原始内容必须仍完整可读，绝不依赖“原文件已移动到备份”作为
/// 失败恢复。备份名带微秒时间戳与自增序号（create_new 独占），绝不覆盖既有备份。
///
/// 有效文件与不存在文件显式 no-op：不写回、不创建空文件或备份，内容、mtime 与
/// inode 均不变。
pub(super) fn restore_desktop_settings() -> Result<DesktopSettingsStatus> {
    restore_desktop_settings_at_path(&desktop_settings_path())
}

fn restore_desktop_settings_at_path(path: &Path) -> Result<DesktopSettingsStatus> {
    let _process_guard = DESKTOP_SETTINGS_UPDATE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("desktop settings update lock poisoned"))?;
    let base = DesktopSettingsStatus {
        loaded: false,
        path: path.display().to_string(),
        error: None,
        backup_path: None,
    };
    Config::update_config_file_opt(path, |existing| {
        let Some(raw) = existing else {
            // 文件不存在：空配置即合法状态，显式 no-op，不创建任何文件
            let mut status = base;
            status.loaded = true;
            return Ok((status, None));
        };
        if parse_desktop_settings_strict(raw).is_ok() {
            // 有效文件：显式 no-op，内容/mtime/inode 均不变
            let mut status = base;
            status.loaded = true;
            return Ok((status, None));
        }
        // 损坏文件：先创建唯一备份副本（原文件仍在原路径），随后由核心层原子替换。
        let backup = Config::create_config_backup(path)?;
        let mut status = base;
        status.loaded = true;
        status.backup_path = Some(backup.display().to_string());
        let contents =
            serde_json::to_vec_pretty(&serde_json::Map::<String, serde_json::Value>::new())?;
        Ok((status, Some(contents)))
    })
}

fn parse_desktop_settings_strict(raw: &[u8]) -> Result<serde_json::Map<String, serde_json::Value>> {
    let value = serde_json::from_slice::<serde_json::Value>(raw)
        .context("设置文件不是有效 JSON，尚未加载")?;
    value
        .as_object()
        .cloned()
        .context("设置文件根节点必须是 JSON 对象，尚未加载")
}

/// 以单一事务读改写用户级桌面设置。
///
/// 进程内锁覆盖完整 RMW 窗口；核心层文件锁将同一保护扩展到其他 Yode 进程。
/// 所有持久化修改都必须经过此入口，避免两项设置从旧 JSON 快照分别写回时丢字段。
pub(super) async fn update_desktop_settings_async<T, F>(update: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> Result<T> + Send + 'static,
{
    update_desktop_settings_at_path_async(&desktop_settings_path(), update).await
}

async fn update_desktop_settings_at_path_async<T, F>(path: &Path, update: F) -> Result<T>
where
    T: Send + 'static,
    F: FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> Result<T> + Send + 'static,
{
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || update_desktop_settings_at_path(&path, update))
        .await
        .context("桌面设置更新任务异常退出")?
}

fn update_desktop_settings_at_path<T, F>(path: &Path, update: F) -> Result<T>
where
    F: FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> Result<T>,
{
    let _process_guard = DESKTOP_SETTINGS_UPDATE_LOCK
        .lock()
        .map_err(|_| anyhow::anyhow!("desktop settings update lock poisoned"))?;
    Config::update_config_file(path, |existing| {
        let mut settings = parse_desktop_settings_for_update(existing)?;
        let result = update(&mut settings)?;
        Ok((result, serde_json::to_vec_pretty(&settings)?))
    })
}

fn parse_desktop_settings_for_update(
    raw: Option<&[u8]>,
) -> Result<serde_json::Map<String, serde_json::Value>> {
    let Some(raw) = raw else {
        return Ok(serde_json::Map::new());
    };
    parse_desktop_settings_strict(raw)
        .map_err(|error| anyhow::anyhow!("桌面设置文件不可用（{}），已拒绝覆盖原始内容", error))
}

pub(super) fn desktop_string_setting(
    settings: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    fallback: &str,
) -> String {
    settings
        .get(key)
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .to_string()
}

pub(super) fn desktop_bool_setting(
    settings: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    fallback: bool,
) -> bool {
    settings
        .get(key)
        .and_then(|value| {
            value
                .as_bool()
                .or_else(|| value.as_str().and_then(|raw| raw.parse::<bool>().ok()))
        })
        .unwrap_or(fallback)
}

pub(super) fn desktop_u32_setting(
    settings: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    fallback: u32,
) -> u32 {
    settings
        .get(key)
        .and_then(|value| {
            value
                .as_u64()
                .and_then(|raw| u32::try_from(raw).ok())
                .or_else(|| value.as_str().and_then(|raw| raw.parse::<u32>().ok()))
        })
        .unwrap_or(fallback)
}

pub(super) fn desktop_string_list_setting(
    settings: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Vec<String> {
    settings
        .get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Barrier};

    use serde_json::json;

    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("yode-settings-{label}-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn primitive_setting_helpers_parse_expected_shapes() {
        let mut settings = serde_json::Map::new();
        settings.insert("name".to_string(), json!("Yode"));
        settings.insert("enabled".to_string(), json!("true"));
        settings.insert("limit".to_string(), json!("42"));

        assert_eq!(
            desktop_string_setting(&settings, "name", "fallback"),
            "Yode"
        );
        assert!(desktop_bool_setting(&settings, "enabled", false));
        assert_eq!(desktop_u32_setting(&settings, "limit", 1), 42);
        assert_eq!(
            desktop_string_setting(&settings, "missing", "fallback"),
            "fallback"
        );
    }

    #[test]
    fn concurrent_updates_preserve_different_keys_and_valid_json() {
        let directory = TestDirectory::new("concurrent");
        let path = Arc::new(directory.path().join(".yode").join("desktop-settings.json"));
        let writers = 12;
        let barrier = Arc::new(Barrier::new(writers));
        let mut handles = Vec::with_capacity(writers);

        for index in 0..writers {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                update_desktop_settings_at_path(&path, |settings| {
                    settings.insert(format!("key-{index}"), json!(index));
                    Ok(())
                })
                .unwrap();
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }

        let raw = std::fs::read(&*path).unwrap();
        let persisted = serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&raw)
            .expect("并发更新后必须保留完整 JSON");
        for index in 0..writers {
            assert_eq!(persisted.get(&format!("key-{index}")), Some(&json!(index)));
        }
    }

    #[test]
    fn failed_replacement_keeps_target_and_cleans_temporary_file() {
        let directory = TestDirectory::new("failed-replacement");
        let parent = directory.path().join(".yode");
        let path = parent.join("desktop-settings.json");
        let replacement_blocker = path.clone();

        let error = update_desktop_settings_at_path(&path, move |settings| {
            settings.insert("should-not-persist".to_string(), json!(true));
            std::fs::create_dir(&replacement_blocker)?;
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("无法原子替换配置文件"));
        assert!(path.is_dir());
        assert!(std::fs::read_dir(&parent).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".desktop-settings.json.tmp-")));
    }

    #[test]
    fn invalid_existing_json_is_not_overwritten_by_an_update() {
        let directory = TestDirectory::new("invalid-json");
        let parent = directory.path().join(".yode");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("desktop-settings.json");
        let invalid = b"{incomplete";
        std::fs::write(&path, invalid).unwrap();

        let error = update_desktop_settings_at_path(&path, |settings| {
            settings.insert("should-not-persist".to_string(), json!(true));
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("已拒绝覆盖原始内容"));
        assert_eq!(std::fs::read(&path).unwrap(), invalid);
    }

    #[test]
    fn status_reports_corrupted_json_without_touching_the_file() {
        let directory = TestDirectory::new("status-corrupt");
        let parent = directory.path().join(".yode");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("desktop-settings.json");
        let invalid = b"{incomplete";
        std::fs::write(&path, invalid).unwrap();

        let status = desktop_settings_status_at(&path);
        assert!(!status.loaded);
        let error = status.error.expect("损坏文件必须报告错误");
        assert!(error.contains("不是有效 JSON"));
        // 状态查询绝不改写原文件
        assert_eq!(std::fs::read(&path).unwrap(), invalid);
    }

    #[test]
    fn status_reports_non_object_root_as_not_loaded() {
        let directory = TestDirectory::new("status-non-object");
        let parent = directory.path().join(".yode");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("desktop-settings.json");
        std::fs::write(&path, b"[1, 2, 3]").unwrap();

        let status = desktop_settings_status_at(&path);
        assert!(!status.loaded);
        assert!(status
            .error
            .as_deref()
            .is_some_and(|error| error.contains("必须是 JSON 对象")));
    }

    #[cfg(unix)]
    #[test]
    fn status_reports_unreadable_file_as_not_loaded() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TestDirectory::new("status-permission");
        let parent = directory.path().join(".yode");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("desktop-settings.json");
        std::fs::write(&path, b"{}").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();

        let status = desktop_settings_status_at(&path);
        // 不可读文件同样如实报告（测试进程通常仍可读；仅在权限确实生效时断言）
        assert!(!status.loaded);
        assert!(status
            .error
            .as_deref()
            .is_some_and(|error| error.contains("无法读取")));
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }

    #[test]
    fn status_reports_missing_file_as_loaded_empty() {
        let directory = TestDirectory::new("status-missing");
        let path = directory.path().join(".yode").join("desktop-settings.json");

        let status = desktop_settings_status_at(&path);
        assert!(status.loaded);
        assert!(status.error.is_none());
    }

    #[test]
    fn restore_backs_up_corrupted_file_and_writes_fresh_settings() {
        let directory = TestDirectory::new("restore-corrupt");
        let parent = directory.path().join(".yode");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("desktop-settings.json");
        let invalid = b"{incomplete";
        std::fs::write(&path, invalid).unwrap();

        let status = restore_desktop_settings_at_path(&path).unwrap();
        assert!(status.loaded);
        assert!(status.error.is_none());
        let backup_path = PathBuf::from(status.backup_path.expect("恢复必须生成备份"));
        // 原始损坏内容被完整备份
        assert_eq!(std::fs::read(&backup_path).unwrap(), invalid);
        // 备份副本同样收紧为 0600
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&backup_path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        // 新配置是空对象且可解析，读取恢复正常
        let restored: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(restored.is_empty());
        assert!(desktop_settings_status_at(&path).loaded);
    }

    #[test]
    fn restore_never_touches_a_valid_settings_file() {
        let directory = TestDirectory::new("restore-valid");
        let parent = directory.path().join(".yode");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("desktop-settings.json");
        let valid = br#"{"yode-browser-enabled": true}"#;
        std::fs::write(&path, valid).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let before = std::fs::metadata(&path).unwrap();
            let status = restore_desktop_settings_at_path(&path).unwrap();
            assert!(status.loaded);
            assert!(status.backup_path.is_none());
            let after = std::fs::metadata(&path).unwrap();
            // 真正无副作用：内容、mtime、inode 均不变
            assert_eq!(std::fs::read(&path).unwrap(), valid);
            assert_eq!(after.ino(), before.ino());
            assert_eq!(after.mtime(), before.mtime());
        }
        #[cfg(not(unix))]
        {
            let status = restore_desktop_settings_at_path(&path).unwrap();
            assert!(status.loaded);
            assert!(status.backup_path.is_none());
            assert_eq!(std::fs::read(&path).unwrap(), valid);
        }
        // 目录中不出现备份文件
        assert!(std::fs::read_dir(&parent).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".bak-")
        }));
    }

    #[test]
    fn restore_on_missing_file_is_a_true_no_op() {
        let directory = TestDirectory::new("restore-missing");
        let parent = directory.path().join(".yode");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("desktop-settings.json");

        let status = restore_desktop_settings_at_path(&path).unwrap();
        assert!(status.loaded);
        assert!(status.backup_path.is_none());
        // 不创建设置文件、空文件或备份（目录中仅允许锁 sidecar）
        assert!(!path.exists());
        assert!(std::fs::read_dir(&parent).unwrap().all(|entry| {
            let name = entry.unwrap().file_name().to_string_lossy().to_string();
            name != "desktop-settings.json"
                && !name.contains(".bak-")
                && name != ".desktop-settings.json.tmp-"
        }));
    }

    #[test]
    fn restore_never_overwrites_an_existing_backup() {
        let directory = TestDirectory::new("restore-collision");
        let parent = directory.path().join(".yode");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("desktop-settings.json");

        // 第一次恢复：损坏内容 A 进入备份 A
        std::fs::write(&path, b"{incomplete").unwrap();
        let first = restore_desktop_settings_at_path(&path).unwrap();
        let backup_a = PathBuf::from(first.backup_path.expect("备份 A"));
        assert_eq!(std::fs::read(&backup_a).unwrap(), b"{incomplete");

        // 再次损坏并恢复：必须生成新的备份 B，绝不覆盖备份 A
        std::fs::write(&path, b"[also broken").unwrap();
        let second = restore_desktop_settings_at_path(&path).unwrap();
        let backup_b = PathBuf::from(second.backup_path.expect("备份 B"));
        assert_ne!(backup_a, backup_b, "备份名不可冲突");
        assert_eq!(std::fs::read(&backup_b).unwrap(), b"[also broken");
        assert_eq!(std::fs::read(&backup_a).unwrap(), b"{incomplete");
    }

    #[cfg(unix)]
    #[test]
    fn backup_creation_failure_leaves_original_untouched() {
        use std::os::unix::fs::PermissionsExt;

        // 备份原语层面的失败注入：父目录只读导致 create_new 失败时，
        // 原文件必须保持原样（create_config_backup 不会修改原文件）。
        let directory = TestDirectory::new("backup-create-fail");
        let parent = directory.path().join(".yode");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("desktop-settings.json");
        std::fs::write(&path, b"{incomplete").unwrap();

        // 先探测：目录只读后是否真的不可写（root 可绕过权限检查）
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o555)).unwrap();
        let probe = parent.join("probe-write");
        let writable = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&probe)
            .is_ok();
        let _ = std::fs::remove_file(&probe);
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
        if writable {
            return; // 以 root 运行：跳过本注入测试
        }

        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o555)).unwrap();
        let error = Config::create_config_backup(&path).unwrap_err();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(error.to_string().contains("备份"));
        // 原路径内容完整可读，未被覆盖、未被移动
        assert_eq!(std::fs::read(&path).unwrap(), b"{incomplete");
        // 没有生成任何备份
        assert!(std::fs::read_dir(&parent).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".bak-")
        }));
    }

    #[test]
    fn restore_replacement_failure_keeps_original_and_backup_intact() {
        // 真实 restore 路径 + 原子替换 rename 前失败注入：
        // 备份已成功创建，但替换新配置失败时，原路径必须仍是原始损坏文件
        // 且内容完整；备份也完整；无临时文件和空备份残留。
        let directory = TestDirectory::new("restore-replace-inject");
        let parent = directory.path().join(".yode");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("desktop-settings.json");
        let invalid = b"{incomplete";
        std::fs::write(&path, invalid).unwrap();

        Config::inject_atomic_replace_rename_failure();
        let error = restore_desktop_settings_at_path(&path).unwrap_err();
        assert!(error.to_string().contains("注入：原子替换 rename 前失败"));

        // 原路径仍是原始损坏文件且内容完整
        assert_eq!(std::fs::read(&path).unwrap(), invalid);
        // 备份完整存在（唯一一个，内容为原始损坏内容）
        let backups = std::fs::read_dir(&parent)
            .unwrap()
            .filter_map(|entry| {
                let name = entry.unwrap().file_name().to_string_lossy().to_string();
                name.contains(".bak-").then_some(parent.join(name))
            })
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1, "应恰好生成一个备份");
        assert_eq!(std::fs::read(&backups[0]).unwrap(), invalid);
        // 无临时文件残留
        assert!(std::fs::read_dir(&parent).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".desktop-settings.json.tmp-")));

        // 注入消费后再次恢复成功：损坏文件被替换为空 JSON，读取恢复正常
        let status = restore_desktop_settings_at_path(&path).unwrap();
        assert!(status.loaded);
        let restored: serde_json::Map<String, serde_json::Value> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(restored.is_empty());
        assert!(desktop_settings_status_at(&path).loaded);
    }

    #[tokio::test]
    async fn manual_fix_makes_status_and_reads_healthy_again() {
        let directory = TestDirectory::new("manual-fix");
        let parent = directory.path().join(".yode");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join("desktop-settings.json");
        std::fs::write(&path, b"{incomplete").unwrap();
        assert!(!desktop_settings_status_at(&path).loaded);

        // 用户手动修复文件后，无需任何恢复操作即可恢复正常读取
        std::fs::write(&path, br#"{"yode-git-branch-prefix": "codex/"}"#).unwrap();
        let status = desktop_settings_status_at(&path);
        assert!(status.loaded);
        let settings = read_desktop_settings_from_path_async(&path).await.unwrap();
        assert_eq!(
            settings
                .get("yode-git-branch-prefix")
                .and_then(|v| v.as_str()),
            Some("codex/")
        );
    }

    #[tokio::test]
    async fn async_update_persists_private_complete_json() {
        let directory = TestDirectory::new("private-json");
        let path = directory.path().join(".yode").join("desktop-settings.json");

        update_desktop_settings_at_path_async(&path, |settings| {
            settings.insert("enabled".to_string(), json!(true));
            Ok(())
        })
        .await
        .unwrap();

        let persisted = serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(
            &std::fs::read(&path).unwrap(),
        )
        .unwrap();
        assert_eq!(persisted.get("enabled"), Some(&json!(true)));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                std::fs::metadata(path.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
    }
}
