use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use fs2::FileExt;
use sha2::{Digest, Sha256};

/// 跨进程会话生命周期锁（RAII）。
///
/// 基于操作系统文件锁（Unix flock / Windows LockFileEx）：
/// - 按数据库路径 + 完整 session ID 的 SHA-256 哈希命名，不同数据库或不同会话互不阻塞；
/// - 持有期间关闭/进程崩溃时由操作系统自动释放，不会残留死锁；
/// - 同一数据库路径 + 同一 session 只允许一个持有者，第二个进程立即被拒绝
///   （返回简体中文错误），杜绝 CLI 与多个桌面进程并发操作同一会话时
///   旧快照覆盖新消息。
#[derive(Debug)]
pub struct SessionLock {
    file: File,
    lock_path: PathBuf,
    session_id: String,
}

impl SessionLock {
    /// 尝试获取指定数据库路径下某 session 的跨进程锁。
    /// 锁已被其他进程（或本进程其他连接）持有时返回错误，绝不阻塞等待。
    pub fn acquire(db_path: &Path, session_id: &str) -> Result<Self> {
        let normalized = normalize_db_path(db_path);
        let lock_dir = session_lock_dir(&normalized);
        std::fs::create_dir_all(&lock_dir)
            .with_context(|| format!("无法创建会话锁目录 '{}'", lock_dir.display()))?;
        let lock_path = lock_dir.join(format!("{}.lock", session_lock_key(session_id)));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .with_context(|| format!("无法打开会话锁文件 '{}'", lock_path.display()))?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Self {
                file,
                lock_path,
                session_id: session_id.to_string(),
            }),
            Err(err) => {
                if is_lock_contention(&err) {
                    Err(anyhow::anyhow!(
                        "该会话正在其他进程中运行（对话/压缩/清理等），请等待其完成后重试。"
                    ))
                } else {
                    Err(err).with_context(|| {
                        format!(
                            "无法锁定会话 '{}'（锁文件 {})",
                            session_id,
                            lock_path.display()
                        )
                    })
                }
            }
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

/// `fs2` 在 Unix 上通常映射为 `WouldBlock`；Windows 的 `LockFileEx` 冲突则可能
/// 保留 Win32 `ERROR_LOCK_VIOLATION` (33)，而不映射为 `WouldBlock`。两者语义相同：
/// 都表示锁已被其他持有者占用，应向调用方返回统一的会话冲突错误。
fn is_lock_contention(err: &std::io::Error) -> bool {
    if err.kind() == std::io::ErrorKind::WouldBlock {
        return true;
    }
    #[cfg(windows)]
    {
        const ERROR_LOCK_VIOLATION: i32 = 33;
        if err.raw_os_error() == Some(ERROR_LOCK_VIOLATION) {
            return true;
        }
    }
    false
}

/// 便捷入口：获取指定数据库路径下某 session 的跨进程锁。
pub fn acquire_session_lock(db_path: &Path, session_id: &str) -> Result<SessionLock> {
    SessionLock::acquire(db_path, session_id)
}

impl Drop for SessionLock {
    fn drop(&mut self) {
        // 显式解锁后再关闭文件；即使失败，文件关闭本身也会释放 OS 锁。
        let _ = FileExt::unlock(&self.file);
    }
}

/// 锁目录与数据库文件同目录：`<db 文件名>.locks/`。
/// 不同数据库路径（哪怕是同名文件在不同目录）使用不同锁目录，天然互不干扰。
fn session_lock_dir(db_path: &Path) -> PathBuf {
    let file_name = db_path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "sessions.db".to_string());
    db_path.with_file_name(format!("{file_name}.locks"))
}

/// 完整 session ID 的 SHA-256 十六进制（64 位十六进制，抗碰撞、确定）。
fn session_lock_key(session_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(session_id.as_bytes());
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 规范化数据库路径：存在则 canonicalize，否则转为绝对路径，
/// 保证 CLI 与桌面端（及测试）对同一数据库推导出同一锁目录。
pub fn normalize_db_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

/// 导出文件的唯一命名与原子写入：
/// - 目标名 = `{base}-{uuid}.md`，同一秒内连续导出绝不互相覆盖；
/// - 先写同目录临时文件，成功后 rename 原子替换；
/// - 失败时清理临时文件并返回错误，不会留下半截导出文件，也不会触碰已有导出文件。
pub fn write_unique_export_file(dir: &Path, base: &str, content: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("无法创建导出目录 '{}'", dir.display()))?;
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let file_name = format!("{base}-{suffix}.md");
    let path = dir.join(&file_name);
    let temp = dir.join(format!(".{file_name}.yode-{suffix}.tmp"));
    let result = (|| -> Result<()> {
        std::fs::write(&temp, content)?;
        std::fs::rename(&temp, &path)?;
        Ok(())
    })();
    if let Err(err) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(err).with_context(|| format!("导出文件写入失败: {}", path.display()));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_session_second_lock_is_rejected_with_chinese_message() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("sessions.db");
        let lock = SessionLock::acquire(&db_path, "session-aaaa-1111").unwrap();

        let err = SessionLock::acquire(&db_path, "session-aaaa-1111")
            .expect_err("同一 session 第二个持有者必须被拒绝");
        assert!(
            err.to_string().contains("该会话正在其他进程中运行"),
            "拒绝原因必须是简体中文: {}",
            err
        );
        assert_eq!(lock.session_id(), "session-aaaa-1111");

        drop(lock);
        SessionLock::acquire(&db_path, "session-aaaa-1111").expect("锁释放后应可重新获取");
    }

    #[cfg(windows)]
    #[test]
    fn windows_error_lock_violation_is_contention() {
        let err = std::io::Error::from_raw_os_error(33);
        assert!(is_lock_contention(&err));
    }

    #[test]
    fn different_sessions_can_lock_in_parallel() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("sessions.db");
        let lock_a = SessionLock::acquire(&db_path, "session-aaaa-1111").unwrap();
        let lock_b = SessionLock::acquire(&db_path, "session-bbbb-2222").unwrap();
        assert_ne!(lock_a.lock_path(), lock_b.lock_path());
        assert_ne!(lock_a.session_id(), lock_b.session_id());
    }

    #[test]
    fn different_databases_allow_same_session_in_parallel() {
        let dir = tempfile::tempdir().unwrap();
        let db_a = dir.path().join("a/sessions.db");
        let db_b = dir.path().join("b/sessions.db");
        let _lock_a = SessionLock::acquire(&db_a, "session-cccc-3333").unwrap();
        let _lock_b = SessionLock::acquire(&db_b, "session-cccc-3333").unwrap();
    }

    #[test]
    fn lock_key_is_deterministic_and_path_normalization_agrees() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("sessions.db");
        assert_eq!(
            session_lock_key("session-aaaa-1111"),
            session_lock_key("session-aaaa-1111")
        );
        assert_ne!(
            session_lock_key("session-aaaa-1111"),
            session_lock_key("session-aaaa-1112")
        );
        // 相对路径与绝对路径规范化后应指向同一锁目录
        let normalized = normalize_db_path(&db_path);
        let lock_a = SessionLock::acquire(&normalized, "session-aaaa-1111").unwrap();
        let lock_path_a = lock_a.lock_path().to_path_buf();
        drop(lock_a);
        let lock_b = SessionLock::acquire(&db_path, "session-aaaa-1111").unwrap();
        assert_eq!(lock_path_a, lock_b.lock_path());
    }

    #[test]
    fn export_files_are_unique_and_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let export_dir = dir.path().join(".yode").join("exports");
        let path_a =
            write_unique_export_file(&export_dir, "session-aaaa-20260101-120000", "第一份导出")
                .unwrap();
        let path_b =
            write_unique_export_file(&export_dir, "session-aaaa-20260101-120000", "第二份导出")
                .unwrap();

        assert_ne!(path_a, path_b, "同一秒连续导出必须生成不同文件");
        assert_eq!(std::fs::read_to_string(&path_a).unwrap(), "第一份导出");
        assert_eq!(std::fs::read_to_string(&path_b).unwrap(), "第二份导出");
        assert!(path_a.exists());
        assert!(path_b.exists());
    }

    #[test]
    fn export_write_failure_leaves_old_files_and_no_partial_output() {
        let dir = tempfile::tempdir().unwrap();
        let export_dir = dir.path().join("exports");
        let existing =
            write_unique_export_file(&export_dir, "session-aaaa-1111", "已有导出").unwrap();
        let before = std::fs::read_to_string(&existing).unwrap();

        // 超长 base 名触发 ENAMETOOLONG，写入必须失败
        let long_base = "x".repeat(300);
        let err = write_unique_export_file(&export_dir, &long_base, "不应写入")
            .expect_err("超长文件名导出必须失败");
        assert!(!err.to_string().is_empty());

        let leftovers = std::fs::read_dir(&export_dir)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
            .count();
        assert_eq!(leftovers, 0, "失败后不得残留临时文件");
        assert_eq!(existing.metadata().unwrap().len() as usize, before.len());
        assert_eq!(
            std::fs::read_to_string(&existing).unwrap(),
            before,
            "已有导出文件必须保持完整"
        );
    }
}
