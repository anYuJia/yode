use serde_json::Value;

pub(crate) const DEFAULT_COMMAND_TIMEOUT_SECS: u64 = 120;
pub(crate) const MAX_COMMAND_TIMEOUT_SECS: u64 = 600;

pub(crate) fn command_timeout_secs(params: &Value) -> u64 {
    match params
        .get("timeout_ms")
        .and_then(|value| value.as_u64())
        .or_else(|| params.get("timeout").and_then(|value| value.as_u64()))
    {
        Some(timeout_ms) if timeout_ms >= 1000 => (timeout_ms / 1000).min(MAX_COMMAND_TIMEOUT_SECS),
        Some(timeout_ms) => timeout_ms.min(MAX_COMMAND_TIMEOUT_SECS),
        None => DEFAULT_COMMAND_TIMEOUT_SECS,
    }
}

pub(crate) fn timeout_ms_description() -> String {
    format!(
        "Optional timeout in milliseconds (max {}ms). Default: {}ms.",
        MAX_COMMAND_TIMEOUT_SECS * 1000,
        DEFAULT_COMMAND_TIMEOUT_SECS * 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn timeout_defaults_and_clamps_milliseconds() {
        assert_eq!(
            command_timeout_secs(&json!({})),
            DEFAULT_COMMAND_TIMEOUT_SECS
        );
        assert_eq!(command_timeout_secs(&json!({ "timeout_ms": 2500 })), 2);
        assert_eq!(
            command_timeout_secs(&json!({ "timeout_ms": 999_000 })),
            MAX_COMMAND_TIMEOUT_SECS
        );
    }

    #[test]
    fn timeout_keeps_legacy_seconds_shape_for_small_values() {
        assert_eq!(command_timeout_secs(&json!({ "timeout_ms": 90 })), 90);
        assert_eq!(command_timeout_secs(&json!({ "timeout": 5 })), 5);
    }
}
