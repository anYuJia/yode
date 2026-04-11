pub(crate) fn format_api_error(
    provider: &str,
    status: reqwest::StatusCode,
    parsed_message: Option<String>,
    fallback_body: &str,
) -> anyhow::Error {
    if let Some(message) = parsed_message {
        anyhow::anyhow!("{} API error ({}): {}", provider, status, message)
    } else {
        anyhow::anyhow!("{} API error ({}): {}", provider, status, fallback_body)
    }
}
