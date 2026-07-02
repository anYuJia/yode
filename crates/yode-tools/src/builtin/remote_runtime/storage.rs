use std::path::Path;

use anyhow::Result;
use tokio::io::AsyncWriteExt;

pub(super) async fn create_dir_all(path: &Path) -> Result<()> {
    Ok(tokio::fs::create_dir_all(path).await?)
}

pub(super) async fn read_text(path: &Path) -> Result<String> {
    Ok(tokio::fs::read_to_string(path).await?)
}

pub(super) async fn write_text(path: &Path, body: impl AsRef<[u8]>) -> Result<()> {
    Ok(tokio::fs::write(path, body).await?)
}

pub(super) async fn append_line(path: &Path, line: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(line.as_bytes()).await?;
    file.write_all(b"\n").await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{append_line, create_dir_all, read_text};

    #[tokio::test]
    async fn remote_storage_appends_lines() {
        let dir =
            std::env::temp_dir().join(format!("yode-remote-storage-{}", uuid::Uuid::new_v4()));
        let path = dir.join("events.jsonl");

        create_dir_all(&dir).await.unwrap();
        append_line(&path, r#"{"cursor":1}"#).await.unwrap();
        append_line(&path, r#"{"cursor":2}"#).await.unwrap();

        assert_eq!(
            read_text(&path).await.unwrap(),
            "{\"cursor\":1}\n{\"cursor\":2}\n"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
