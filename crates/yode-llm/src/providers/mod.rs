pub mod anthropic;
pub(crate) mod error_shared;
pub mod gemini;
pub(crate) mod http_client;
pub mod openai;
pub mod openai_compat;
pub(crate) mod retry;
pub(crate) mod streaming_shared;

use serde::Serialize;
use tracing::warn;

pub use anthropic::AnthropicProvider;
pub use gemini::GeminiProvider;
pub use openai::OpenAiProvider;

pub(crate) fn debug_requests_enabled() -> bool {
    std::env::var("YODE_DEBUG_PROVIDER_REQUESTS").is_ok_and(|value| value == "1")
}

pub(crate) async fn write_debug_artifact(
    provider: &str,
    kind: &str,
    payload: impl Serialize,
) -> Option<std::path::PathBuf> {
    if !debug_requests_enabled() {
        return None;
    }

    let debug_dir = match std::env::current_dir() {
        Ok(current_dir) => current_dir
            .join(".yode")
            .join("debug")
            .join("provider-requests")
            .join(provider),
        Err(error) => {
            warn!("Failed to resolve provider debug artifact directory: {error}");
            return None;
        }
    };

    let timestamp_ms = match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => duration.as_millis(),
        Err(error) => {
            warn!("Failed to timestamp provider debug artifact: {error}");
            return None;
        }
    };
    let path = debug_dir.join(format!("{timestamp_ms}-{kind}.json"));
    let rendered = match serde_json::to_string_pretty(&payload) {
        Ok(rendered) => rendered,
        Err(error) => {
            warn!(
                provider,
                kind, "Failed to serialize provider debug artifact: {error}"
            );
            return None;
        }
    };
    if let Err(error) = write_debug_artifact_file(&path, rendered).await {
        warn!(
            provider,
            kind,
            path = %path.display(),
            "Failed to write provider debug artifact: {error}"
        );
        return None;
    }
    Some(path)
}

async fn write_debug_artifact_file(
    path: &std::path::Path,
    rendered: String,
) -> std::io::Result<()> {
    let Some(debug_dir) = path.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "debug artifact path has no parent directory",
        ));
    };
    tokio::fs::create_dir_all(debug_dir).await?;
    tokio::fs::write(path, rendered).await
}

#[cfg(test)]
mod tests {
    use super::write_debug_artifact_file;

    fn unique_test_path(name: &str) -> std::path::PathBuf {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "yode-llm-{name}-{}-{timestamp}",
            std::process::id()
        ))
    }

    #[tokio::test]
    async fn debug_artifact_file_writer_creates_parent_directories() {
        let dir = unique_test_path("debug-artifact");
        let path = dir.join("nested").join("request.json");

        write_debug_artifact_file(&path, "{\"ok\":true}".to_string())
            .await
            .unwrap();

        assert_eq!(
            tokio::fs::read_to_string(&path).await.unwrap(),
            "{\"ok\":true}"
        );
        let _ = tokio::fs::remove_dir_all(dir).await;
    }

    #[tokio::test]
    async fn debug_artifact_file_writer_rejects_parentless_paths() {
        let error = write_debug_artifact_file(std::path::Path::new("/"), "{}".to_string())
            .await
            .unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}
