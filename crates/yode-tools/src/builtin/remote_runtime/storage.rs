use std::io::Write;
use std::path::Path;

use anyhow::Result;

pub(super) trait RemoteStorage {
    fn create_dir_all(&self, path: &Path) -> Result<()>;
    fn exists(&self, path: &Path) -> bool;
    fn read_text(&self, path: &Path) -> Result<String>;
    fn write_text(&self, path: &Path, body: &str) -> Result<()>;
    fn append_line(&self, path: &Path, line: &str) -> Result<()>;
}

#[derive(Debug, Default, Clone, Copy)]
pub(super) struct LocalRemoteStorage;

impl RemoteStorage for LocalRemoteStorage {
    fn create_dir_all(&self, path: &Path) -> Result<()> {
        Ok(std::fs::create_dir_all(path)?)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_text(&self, path: &Path) -> Result<String> {
        Ok(std::fs::read_to_string(path)?)
    }

    fn write_text(&self, path: &Path, body: &str) -> Result<()> {
        Ok(std::fs::write(path, body)?)
    }

    fn append_line(&self, path: &Path, line: &str) -> Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }
}

pub(super) fn local_remote_storage() -> LocalRemoteStorage {
    LocalRemoteStorage
}

#[cfg(test)]
mod tests {
    use super::{local_remote_storage, RemoteStorage};

    #[test]
    fn local_remote_storage_appends_lines() {
        let dir =
            std::env::temp_dir().join(format!("yode-remote-storage-{}", uuid::Uuid::new_v4()));
        let path = dir.join("events.jsonl");
        let storage = local_remote_storage();

        storage.create_dir_all(&dir).unwrap();
        storage.append_line(&path, r#"{"cursor":1}"#).unwrap();
        storage.append_line(&path, r#"{"cursor":2}"#).unwrap();

        assert_eq!(
            storage.read_text(&path).unwrap(),
            "{\"cursor\":1}\n{\"cursor\":2}\n"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
