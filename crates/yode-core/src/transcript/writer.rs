use std::path::Path;

use anyhow::Result;

use crate::session_artifact::atomic_write_sync;

pub(super) fn write_string_with_retry(path: &Path, content: &str) -> Result<()> {
    atomic_write_sync(path, content)
}
