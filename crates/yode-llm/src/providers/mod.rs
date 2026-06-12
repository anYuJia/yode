pub mod anthropic;
pub(crate) mod error_shared;
pub mod gemini;
pub(crate) mod http_client;
pub mod openai;
pub mod openai_compat;
pub(crate) mod retry;
pub(crate) mod streaming_shared;

use serde::Serialize;

pub use anthropic::AnthropicProvider;
pub use gemini::GeminiProvider;
pub use openai::OpenAiProvider;

pub(crate) fn debug_requests_enabled() -> bool {
    std::env::var("YODE_DEBUG_PROVIDER_REQUESTS").is_ok_and(|value| value == "1")
}

pub(crate) fn write_debug_artifact(
    provider: &str,
    kind: &str,
    payload: impl Serialize,
) -> Option<std::path::PathBuf> {
    if !debug_requests_enabled() {
        return None;
    }

    let debug_dir = std::env::current_dir()
        .ok()?
        .join(".yode")
        .join("debug")
        .join("provider-requests")
        .join(provider);
    std::fs::create_dir_all(&debug_dir).ok()?;

    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    let path = debug_dir.join(format!("{timestamp_ms}-{kind}.json"));
    let rendered = serde_json::to_string_pretty(&payload).ok()?;
    std::fs::write(&path, rendered).ok()?;
    Some(path)
}
