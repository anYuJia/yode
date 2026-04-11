use std::path::Path;

use anyhow::Result;

const TRANSCRIPT_WRITE_RETRIES: usize = 3;

pub(super) fn write_string_with_retry(path: &Path, content: &str) -> Result<()> {
    let mut last_error = None;
    for attempt in 0..TRANSCRIPT_WRITE_RETRIES {
        match std::fs::write(path, content) {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = Some(error);
                if attempt + 1 < TRANSCRIPT_WRITE_RETRIES {
                    std::thread::sleep(std::time::Duration::from_millis(25 * (attempt as u64 + 1)));
                }
            }
        }
    }
    Err(last_error.unwrap().into())
}
