use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::{RequestBuilder, Response, StatusCode};

const MAX_HTTP_RETRIES: u32 = 2;

pub(crate) async fn send_with_retry(
    mut build_request: impl FnMut() -> RequestBuilder,
    context: &'static str,
) -> Result<Response> {
    let mut last_error = None;
    for attempt in 0..=MAX_HTTP_RETRIES {
        match build_request().send().await {
            Ok(response)
                if is_retryable_status(response.status()) && attempt < MAX_HTTP_RETRIES =>
            {
                tracing::warn!(
                    status = %response.status(),
                    attempt = attempt + 1,
                    max_attempts = MAX_HTTP_RETRIES + 1,
                    "Retrying provider HTTP request"
                );
                tokio::time::sleep(retry_delay(attempt)).await;
            }
            Ok(response) => return Ok(response),
            Err(err) if is_retryable_reqwest_error(&err) && attempt < MAX_HTTP_RETRIES => {
                tracing::warn!(
                    error = %err,
                    attempt = attempt + 1,
                    max_attempts = MAX_HTTP_RETRIES + 1,
                    "Retrying provider HTTP request after transport error"
                );
                last_error = Some(err);
                tokio::time::sleep(retry_delay(attempt)).await;
            }
            Err(err) => return Err(err).with_context(|| context),
        }
    }
    Err(last_error
        .map(anyhow::Error::from)
        .unwrap_or_else(|| anyhow::anyhow!("provider HTTP request exhausted retries")))
    .with_context(|| context)
}

pub(crate) fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::INTERNAL_SERVER_ERROR
        || status == StatusCode::BAD_GATEWAY
        || status == StatusCode::SERVICE_UNAVAILABLE
        || status == StatusCode::GATEWAY_TIMEOUT
}

fn is_retryable_reqwest_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_request()
}

fn retry_delay(attempt: u32) -> Duration {
    Duration::from_millis(250 * 2u64.pow(attempt.min(3)))
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

    use super::is_retryable_status;

    #[test]
    fn retry_status_policy_matches_provider_contract() {
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(StatusCode::FORBIDDEN));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));
    }
}
