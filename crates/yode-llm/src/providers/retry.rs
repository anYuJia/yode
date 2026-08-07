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
                    error = %redact_url_credentials(&err.to_string()),
                    attempt = attempt + 1,
                    max_attempts = MAX_HTTP_RETRIES + 1,
                    "Retrying provider HTTP request after transport error"
                );
                last_error = Some(err);
                tokio::time::sleep(retry_delay(attempt)).await;
            }
            Err(err) => {
                // reqwest 传输错误 Display 会包含完整请求 URL；
                // Gemini 等提供商把 API key 拼进 URL，必须脱敏后才可进入日志或错误文本
                return Err(anyhow::anyhow!(
                    "{}",
                    redact_url_credentials(&err.to_string())
                ))
                .with_context(|| context);
            }
        }
    }
    Err(last_error
        .map(|err| anyhow::anyhow!("{}", redact_url_credentials(&err.to_string())))
        .unwrap_or_else(|| anyhow::anyhow!("provider HTTP request exhausted retries")))
    .with_context(|| context)
}

/// 剔除文本中的 URL query 密钥参数（key=xxx、api_key=xxx、token=xxx、access_token=xxx），
/// 防止 API key 进入日志或错误信息。
pub(crate) fn redact_url_credentials(text: &str) -> String {
    let mut redacted = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(qpos) = rest.find('?') {
        redacted.push_str(&rest[..=qpos]);
        rest = &rest[qpos + 1..];
        let Some(amp) = rest.find('&').or_else(|| rest.find(' ')) else {
            // 剩余内容都是 query 段
            let cleaned = redact_query_pairs(rest);
            redacted.push_str(&cleaned);
            return redacted;
        };
        let pair = &rest[..amp];
        let cleaned = redact_query_pairs(pair);
        redacted.push_str(&cleaned);
        rest = &rest[amp..];
    }
    redacted.push_str(rest);
    redacted
}

fn redact_query_pairs(pair: &str) -> String {
    let lower = pair.to_ascii_lowercase();
    for needle in ["key=", "api_key=", "token=", "access_token=", "apikey="] {
        if lower.starts_with(needle) {
            return format!("{}REDACTED", &pair[..needle.len()]);
        }
    }
    pair.to_string()
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

    use super::{is_retryable_status, redact_url_credentials};

    #[test]
    fn retry_status_policy_matches_provider_contract() {
        assert!(is_retryable_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(!is_retryable_status(StatusCode::BAD_REQUEST));
        assert!(!is_retryable_status(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable_status(StatusCode::FORBIDDEN));
        assert!(!is_retryable_status(StatusCode::NOT_FOUND));
    }

    #[test]
    fn redacts_key_params_from_error_text() {
        let message =
            "error sending request for url (https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent?key=sk-super-secret-123)";
        let redacted = redact_url_credentials(message);
        assert!(!redacted.contains("sk-super-secret-123"));
        assert!(redacted.contains("key=REDACTED"));
        assert!(redacted.contains("gemini-2.5-pro"));
    }

    #[test]
    fn redacts_multiple_query_params_and_keeps_rest() {
        let message = "connect failed for https://host/m?key=secret-key&alt=sse more context";
        let redacted = redact_url_credentials(message);
        assert!(!redacted.contains("secret-key"));
        assert!(redacted.contains("alt=sse"));
        assert!(redacted.contains("more context"));
    }

    #[test]
    fn redacts_text_without_query_unchanged() {
        let message = "connection timed out";
        assert_eq!(redact_url_credentials(message), message);
    }
}
