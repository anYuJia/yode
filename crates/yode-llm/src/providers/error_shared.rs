pub(crate) async fn read_error_body(
    provider: &str,
    status: reqwest::StatusCode,
    response: reqwest::Response,
) -> String {
    match response.text().await {
        Ok(body) => body,
        Err(err) => {
            tracing::warn!(
                provider,
                status = %status,
                error = %err,
                "failed to read provider error response body"
            );
            format!("<failed to read error response body: {err}>")
        }
    }
}

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
