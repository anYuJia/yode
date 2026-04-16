#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ErrorKind {
    /// 429 Too Many Requests — retry with long backoff
    RateLimit,
    /// 500/502/503/504, timeout, network — retry with standard backoff
    Transient,
    /// 400/401/403/404 etc. — do not retry
    Fatal,
}

/// Maximum retry count for retryable errors.
const MAX_RETRIES: u32 = 9;
/// Maximum retry count for rate-limit (429) errors.
const MAX_RATE_LIMIT_RETRIES: u32 = 9;

pub(super) fn classify_error(err: &anyhow::Error) -> ErrorKind {
    let msg = format!("{:#}", err);
    if msg.contains("429") || msg.contains("rate_limit") || msg.contains("Too Many Requests") {
        ErrorKind::RateLimit
    } else if msg.contains("500")
        || msg.contains("502")
        || msg.contains("503")
        || msg.contains("504")
        || msg.contains("timeout")
        || msg.contains("超时")
        || msg.contains("timed out")
        || msg.contains("connection")
        || msg.contains("Connection")
        || msg.contains("ECONNRESET")
        || msg.contains("ECONNREFUSED")
        || msg.contains("Broken pipe")
        || msg.contains("reset by peer")
        || msg.contains("Failed to send")
        || msg.contains("failed to send")
        || msg.contains("dns error")
        || msg.contains("DNS error")
        || msg.contains("hyper")
        || msg.contains("reqwest")
        || msg.contains("network")
        || msg.contains("Network")
        || msg.contains("temporarily unavailable")
        || msg.contains("connect error")
        || msg.contains("Connect error")
    {
        ErrorKind::Transient
    } else {
        ErrorKind::Fatal
    }
}

/// Compute retry delay based on error kind and attempt number.
pub(super) fn retry_delay(kind: ErrorKind, attempt: u32) -> std::time::Duration {
    match kind {
        ErrorKind::RateLimit => {
            let secs = match attempt {
                0 => 5,
                1 => 10,
                2 => 15,
                3 => 20,
                _ => 30,
            };
            std::time::Duration::from_secs(secs)
        }
        ErrorKind::Transient => {
            let base_secs = 2u64.pow(attempt.min(4) + 1);
            let jitter = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                % 1000;
            std::time::Duration::from_millis((base_secs * 1000) + jitter as u64)
        }
        ErrorKind::Fatal => std::time::Duration::from_secs(0),
    }
}

pub(super) fn hex_short(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(6)
        .map(|byte| format!("{:02x}", byte))
        .collect()
}

/// Max retries for a given error kind.
pub(super) fn max_retries_for(kind: ErrorKind) -> u32 {
    match kind {
        ErrorKind::RateLimit => MAX_RATE_LIMIT_RETRIES,
        ErrorKind::Transient => MAX_RETRIES,
        ErrorKind::Fatal => 0,
    }
}

pub(super) fn total_attempts_for(kind: ErrorKind) -> u32 {
    max_retries_for(kind).saturating_add(1)
}

pub(super) fn summarize_retry_error_message(message: &str) -> String {
    let first_line = message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("request failed");
    let squashed = first_line.split_whitespace().collect::<Vec<_>>().join(" ");
    if squashed.chars().count() <= 140 {
        squashed
    } else {
        format!("{}...", squashed.chars().take(140).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::{summarize_retry_error_message, total_attempts_for, ErrorKind};

    #[test]
    fn total_attempts_are_capped_at_ten() {
        assert_eq!(total_attempts_for(ErrorKind::Transient), 10);
        assert_eq!(total_attempts_for(ErrorKind::RateLimit), 10);
    }

    #[test]
    fn retry_error_summary_uses_first_non_empty_line() {
        let summary = summarize_retry_error_message("\n  connection reset by peer\nmore detail");
        assert_eq!(summary, "connection reset by peer");
    }
}
