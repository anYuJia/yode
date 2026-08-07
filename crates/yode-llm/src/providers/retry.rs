use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::{RequestBuilder, Response, StatusCode};

const MAX_HTTP_RETRIES: u32 = 2;
/// Retry-After 上限：尊重服务端建议但绝不无限等待（60s 封顶）。
const MAX_RETRY_AFTER_SECS: u64 = 60;
/// 整个重试流程的总等待上限（含所有退避）。
const MAX_TOTAL_RETRY_WAIT_SECS: u64 = 30;

pub(crate) async fn send_with_retry(
    mut build_request: impl FnMut() -> RequestBuilder,
    context: &'static str,
) -> Result<Response> {
    let mut last_error = None;
    let retry_started = std::time::Instant::now();
    for attempt in 0..=MAX_HTTP_RETRIES {
        match build_request().send().await {
            Ok(response)
                if is_retryable_status(response.status()) && attempt < MAX_HTTP_RETRIES =>
            {
                let retry_after = parse_retry_after(response.headers());
                tracing::warn!(
                    status = %response.status(),
                    attempt = attempt + 1,
                    max_attempts = MAX_HTTP_RETRIES + 1,
                    retry_after_secs = ?retry_after,
                    "Retrying provider HTTP request"
                );
                let delay = retry_delay(attempt).max(retry_after);
                if retry_started.elapsed() + delay
                    > std::time::Duration::from_secs(MAX_TOTAL_RETRY_WAIT_SECS)
                {
                    tracing::warn!(
                        "Provider retry budget exhausted; giving up after {}",
                        attempt + 1
                    );
                    return Ok(response);
                }
                tokio::time::sleep(delay).await;
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
                if retry_started.elapsed() + retry_delay(attempt)
                    > std::time::Duration::from_secs(MAX_TOTAL_RETRY_WAIT_SECS)
                {
                    break;
                }
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

/// 解析 `Retry-After` 响应头：支持秒数或 HTTP 日期格式；无效时返回零延迟。
fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> std::time::Duration {
    let Some(value) = headers.get(reqwest::header::RETRY_AFTER) else {
        return std::time::Duration::ZERO;
    };
    let value = match value.to_str() {
        Ok(value) => value.trim(),
        Err(_) => return std::time::Duration::ZERO,
    };
    if let Ok(secs) = value.parse::<u64>() {
        return std::time::Duration::from_secs(secs.min(MAX_RETRY_AFTER_SECS));
    }
    // HTTP 日期格式（RFC 1123 / RFC 850）
    if let Ok(date) = chrono::DateTime::parse_from_rfc2822(value) {
        let now = chrono::Utc::now();
        let wait = date
            .with_timezone(&chrono::Utc)
            .signed_duration_since(now)
            .to_std()
            .unwrap_or(std::time::Duration::ZERO);
        return wait.min(std::time::Duration::from_secs(MAX_RETRY_AFTER_SECS));
    }
    std::time::Duration::ZERO
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

    use super::{is_retryable_status, parse_retry_after, redact_url_credentials};

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
    fn parses_retry_after_seconds_with_cap() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("120"),
        );
        // 超过上限会被封顶
        assert_eq!(
            parse_retry_after(&headers),
            std::time::Duration::from_secs(60)
        );

        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("3"),
        );
        assert_eq!(
            parse_retry_after(&headers),
            std::time::Duration::from_secs(3)
        );
    }

    #[test]
    fn parses_retry_after_http_date_and_invalid_values() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("Fri, 06 Nov 2026 08:49:37 GMT"),
        );
        // 未来日期 -> 等待该间隔（封顶）
        let parsed = parse_retry_after(&headers);
        assert!(parsed > std::time::Duration::ZERO);
        assert!(parsed <= std::time::Duration::from_secs(60));

        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("not-a-date"),
        );
        assert_eq!(parse_retry_after(&headers), std::time::Duration::ZERO);

        // 过去的日期 -> 零等待
        headers.insert(
            reqwest::header::RETRY_AFTER,
            reqwest::header::HeaderValue::from_static("Mon, 01 Jan 2001 00:00:00 GMT"),
        );
        assert_eq!(parse_retry_after(&headers), std::time::Duration::ZERO);
    }

    #[test]
    fn redacts_text_without_query_unchanged() {
        let message = "connection timed out";
        assert_eq!(redact_url_credentials(message), message);
    }
}
