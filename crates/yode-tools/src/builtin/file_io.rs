use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

/// Durably replace a file without exposing a partially-written destination.
///
/// The temporary file is created in the destination directory so the final
/// rename stays on the same filesystem. Cancellation is honored before the
/// rename; once the rename starts, the replacement is intentionally atomic.
pub(crate) async fn atomic_write(
    path: &Path,
    content: &[u8],
    cancellation: Option<&CancellationToken>,
) -> Result<()> {
    if cancellation.is_some_and(CancellationToken::is_cancelled) {
        anyhow::bail!("file write cancelled before it started");
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    tokio::fs::create_dir_all(parent)
        .await
        .with_context(|| format!("failed to create parent directory '{}'", parent.display()))?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("yode-write");
    let temp_path = parent.join(format!(".{file_name}.yode-{}.tmp", uuid::Uuid::new_v4()));

    let result = write_and_replace(path, &temp_path, content, cancellation).await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temp_path).await;
    }
    result
}

async fn write_and_replace(
    path: &Path,
    temp_path: &PathBuf,
    content: &[u8],
    cancellation: Option<&CancellationToken>,
) -> Result<()> {
    let existing_permissions = tokio::fs::metadata(path)
        .await
        .ok()
        .map(|metadata| metadata.permissions());
    let mut temp = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(temp_path)
        .await
        .with_context(|| format!("failed to create temporary file '{}'", temp_path.display()))?;

    temp.write_all(content)
        .await
        .with_context(|| format!("failed to write temporary file '{}'", temp_path.display()))?;
    temp.flush()
        .await
        .with_context(|| format!("failed to flush temporary file '{}'", temp_path.display()))?;
    temp.sync_all()
        .await
        .with_context(|| format!("failed to sync temporary file '{}'", temp_path.display()))?;
    drop(temp);

    if let Some(permissions) = existing_permissions {
        tokio::fs::set_permissions(temp_path, permissions)
            .await
            .with_context(|| {
                format!(
                    "failed to preserve permissions on temporary file '{}'",
                    temp_path.display()
                )
            })?;
    }

    if cancellation.is_some_and(CancellationToken::is_cancelled) {
        anyhow::bail!("file write cancelled before atomic replacement");
    }

    tokio::fs::rename(temp_path, path).await.with_context(|| {
        format!(
            "failed to atomically replace '{}' with '{}'",
            path.display(),
            temp_path.display()
        )
    })?;
    sync_parent_directory(parent_for(path)).await?;
    Ok(())
}

fn parent_for(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(unix)]
async fn sync_parent_directory(parent: &Path) -> Result<()> {
    let parent = parent.to_path_buf();
    tokio::task::spawn_blocking(move || {
        std::fs::File::open(&parent)
            .and_then(|directory| directory.sync_all())
            .with_context(|| format!("failed to sync directory '{}'", parent.display()))
    })
    .await
    .context("directory sync task failed")??;
    Ok(())
}

#[cfg(not(unix))]
async fn sync_parent_directory(_parent: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::atomic_write;
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn atomic_write_replaces_content_and_cleans_temporary_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.txt");
        tokio::fs::write(&path, "old").await.unwrap();

        atomic_write(&path, b"new", None).await.unwrap();

        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "new");
        let mut entries = tokio::fs::read_dir(dir.path()).await.unwrap();
        let mut names = Vec::new();
        while let Some(entry) = entries.next_entry().await.unwrap() {
            names.push(entry.file_name());
        }
        assert_eq!(names, vec![std::ffi::OsString::from("state.txt")]);
    }

    #[tokio::test]
    async fn cancelled_atomic_write_keeps_original_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.txt");
        tokio::fs::write(&path, "old").await.unwrap();
        let token = CancellationToken::new();
        token.cancel();

        assert!(atomic_write(&path, b"new", Some(&token)).await.is_err());
        assert_eq!(tokio::fs::read_to_string(&path).await.unwrap(), "old");
    }
}
