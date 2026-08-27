use std::path::{Component, Path, PathBuf};

#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;

use super::DesktopRuntime;

impl DesktopRuntime {
    pub async fn edit_diff_artifact_read(&self, path: String) -> Result<String> {
        read_edit_diff_artifact_from_roots(&path, &self.edit_diff_artifact_roots()?).await
    }

    fn edit_diff_artifact_roots(&self) -> Result<Vec<PathBuf>> {
        let active_session_id = self
            .active_session_id
            .lock()
            .map_err(|_| anyhow::anyhow!("active session lock poisoned"))?
            .clone();
        let mut roots = Vec::new();
        if let Some(session_id) = active_session_id {
            if let Some(session) = self.db.get_session(&session_id)? {
                if let Some(project_root) = session.project_root {
                    if !project_root.trim().is_empty() {
                        roots.push(PathBuf::from(project_root));
                    }
                }
            }
        }
        roots.push(self.workspace_path.clone());
        roots.dedup();
        Ok(roots)
    }
}

#[cfg(test)]
pub(super) async fn read_edit_diff_artifact_from_roots(
    path: &str,
    roots: &[PathBuf],
) -> Result<String> {
    read_edit_diff_artifact_from_roots_impl(path, roots).await
}

#[cfg(not(test))]
async fn read_edit_diff_artifact_from_roots(path: &str, roots: &[PathBuf]) -> Result<String> {
    read_edit_diff_artifact_from_roots_impl(path, roots).await
}

async fn read_edit_diff_artifact_from_roots_impl(path: &str, roots: &[PathBuf]) -> Result<String> {
    let clean = path.trim();
    if clean.is_empty() {
        anyhow::bail!("diff artifact path is empty");
    }
    if clean.contains('\0') {
        anyhow::bail!("diff artifact path contains invalid characters");
    }

    let relative = Path::new(clean);
    if relative.is_absolute() {
        anyhow::bail!("diff artifact path must be relative");
    }
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        anyhow::bail!("diff artifact path contains unsafe components");
    }

    let mut searched = Vec::new();
    let mut last_error: Option<anyhow::Error> = None;
    let mut candidate_roots = Vec::new();
    for root in roots {
        candidate_roots.push(root.clone());
        if let Ok(mut entries) = tokio::fs::read_dir(root).await {
            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                if entry.file_type().await?.is_dir() {
                    candidate_roots.push(path);
                }
            }
        }
    }
    candidate_roots.dedup();

    for root in &candidate_roots {
        let allowed_dir = root.join(".yode").join("edit-diffs");
        searched.push(allowed_dir.display().to_string());
        let target = root.join(relative);
        let canonical_target = match tokio::fs::canonicalize(&target).await {
            Ok(path) => path,
            Err(err) => {
                last_error = Some(
                    anyhow::anyhow!(err).context(format!("Failed to access {}", target.display())),
                );
                continue;
            }
        };
        let canonical_allowed = match tokio::fs::canonicalize(&allowed_dir).await {
            Ok(path) => path,
            Err(err) => {
                last_error = Some(
                    anyhow::anyhow!(err)
                        .context(format!("Failed to access {}", allowed_dir.display())),
                );
                continue;
            }
        };
        if !canonical_target.starts_with(&canonical_allowed) {
            last_error = Some(anyhow::anyhow!(
                "diff artifact path is outside .yode/edit-diffs"
            ));
            continue;
        }

        let relative_target = match canonical_target.strip_prefix(&canonical_allowed) {
            Ok(relative) if !relative.as_os_str().is_empty() => relative.to_path_buf(),
            _ => {
                last_error = Some(anyhow::anyhow!("diff artifact path points at its root"));
                continue;
            }
        };

        // canonicalize 只用于确定允许的根；实际读取必须从该根目录句柄打开，避免
        // 检查后攻击者替换符号链接导致 TOCTOU 越界读取。
        return tokio::task::spawn_blocking(move || {
            read_artifact_beneath(&canonical_allowed, &relative_target)
        })
        .await
        .map_err(|err| anyhow::anyhow!("读取 diff 工件任务失败：{}", err))?;
    }

    let searched = if searched.is_empty() {
        "no project roots".to_string()
    } else {
        searched.join(", ")
    };
    if let Some(error) = last_error {
        anyhow::bail!(
            "Failed to read diff artifact {}; searched {}; last error: {}",
            clean,
            searched,
            error
        );
    }
    anyhow::bail!(
        "Failed to read diff artifact {}; searched {}",
        clean,
        searched
    )
}

#[cfg(unix)]
fn read_artifact_beneath(root: &Path, relative: &Path) -> Result<String> {
    use std::ffi::CString;
    use std::io::Read;
    use std::os::fd::{FromRawFd, RawFd};
    use std::os::unix::ffi::OsStrExt;

    let root_bytes = CString::new(root.as_os_str().as_bytes())?;
    // O_NOFOLLOW 仅允许打开真实目录，后续每一级也同样拒绝符号链接。
    let mut dir_fd = unsafe {
        libc::open(
            root_bytes.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    if dir_fd < 0 {
        return Err(std::io::Error::last_os_error()).context("无法安全打开 diff 工件根目录");
    }

    let components = relative.components().collect::<Vec<_>>();
    if components.is_empty() {
        unsafe { libc::close(dir_fd) };
        anyhow::bail!("diff artifact path is empty");
    }
    for component in &components[..components.len() - 1] {
        let name = match component {
            std::path::Component::Normal(name) => CString::new(name.as_bytes())?,
            _ => {
                unsafe { libc::close(dir_fd) };
                anyhow::bail!("diff artifact path contains unsafe components");
            }
        };
        let next_fd = unsafe {
            libc::openat(
                dir_fd,
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if next_fd < 0 {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(dir_fd) };
            return Err(err).context("diff artifact path contains a symlink or is inaccessible");
        }
        unsafe { libc::close(dir_fd) };
        dir_fd = next_fd;
    }

    let leaf = match components.last().unwrap() {
        std::path::Component::Normal(name) => CString::new(name.as_bytes())?,
        _ => {
            unsafe { libc::close(dir_fd) };
            anyhow::bail!("diff artifact path contains unsafe components");
        }
    };
    let file_fd = unsafe {
        libc::openat(
            dir_fd,
            leaf.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )
    };
    unsafe { libc::close(dir_fd) };
    if file_fd < 0 {
        return Err(std::io::Error::last_os_error()).context("无法安全打开 diff 工件");
    }
    let mut file = unsafe { std::fs::File::from_raw_fd(file_fd as RawFd) };
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        anyhow::bail!("diff artifact path is not a regular file");
    }
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(content)
}

#[cfg(windows)]
fn read_artifact_beneath(root: &Path, relative: &Path) -> Result<String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetFinalPathNameByHandleW, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_GENERIC_READ, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        VOLUME_NAME_DOS,
    };

    fn wide(value: &OsStr) -> Vec<u16> {
        value.encode_wide().chain(std::iter::once(0)).collect()
    }

    fn final_path(handle: isize) -> Result<PathBuf> {
        let mut buffer = vec![0u16; 32_768];
        let len = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                VOLUME_NAME_DOS,
            )
        };
        if len == 0 || len as usize >= buffer.len() {
            anyhow::bail!("无法解析 diff 工件最终路径");
        }
        let path = String::from_utf16_lossy(&buffer[..len as usize]);
        Ok(PathBuf::from(path.trim_start_matches(r"\\?\")))
    }

    let root_canonical = std::fs::canonicalize(root)?;
    let target = root.join(relative);
    let target_wide = wide(target.as_os_str());
    let handle = unsafe {
        CreateFileW(
            target_wide.as_ptr(),
            FILE_GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT,
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(std::io::Error::last_os_error().into());
    }

    let final_path = match final_path(handle) {
        Ok(path) => path,
        Err(err) => {
            unsafe { CloseHandle(handle) };
            return Err(err);
        }
    };
    if !final_path.starts_with(&root_canonical) {
        unsafe { CloseHandle(handle) };
        anyhow::bail!("diff artifact path resolves outside .yode/edit-diffs");
    }

    let mut file = unsafe { std::fs::File::from_raw_handle(handle as *mut std::ffi::c_void) };
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        anyhow::bail!("diff artifact path is not a regular file");
    }
    use std::io::Read;
    let mut content = String::new();
    file.read_to_string(&mut content)?;
    Ok(content)
}
