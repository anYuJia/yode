#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("LLM call timed out after {timeout_secs}s")]
    LlmTimeout { timeout_secs: u64 },

    #[error("LLM call failed after {attempts} attempts: {message}")]
    LlmRetryExhausted { attempts: u32, message: String },

    #[error("Permission denied for tool {tool}: {reason}")]
    PermissionDenied { tool: String, reason: String },
}

#[cfg(test)]
mod tests {
    use super::EngineError;

    #[test]
    fn engine_errors_have_operator_facing_messages() {
        assert_eq!(
            EngineError::LlmTimeout { timeout_secs: 120 }.to_string(),
            "LLM call timed out after 120s"
        );
        assert_eq!(
            EngineError::PermissionDenied {
                tool: "bash".to_string(),
                reason: "requires confirmation".to_string(),
            }
            .to_string(),
            "Permission denied for tool bash: requires confirmation"
        );
    }
}
