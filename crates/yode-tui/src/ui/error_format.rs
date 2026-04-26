#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    ContextLimit,
    Authentication,
    RateLimit,
    ProviderRejected,
    Timeout,
    Generic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ErrorView {
    pub kind: ErrorKind,
    pub title: String,
    pub detail_lines: Vec<String>,
}

pub(crate) fn parse_error_view(content: &str) -> ErrorView {
    let summary = first_nonempty_line(content).unwrap_or("Unknown error");
    let normalized = summary.to_ascii_lowercase();

    if normalized.contains("prompt too long")
        || normalized.contains("maximum context")
        || normalized.contains("context length")
        || normalized.contains("context window")
        || normalized.contains("too many tokens")
    {
        return ErrorView {
            kind: ErrorKind::ContextLimit,
            title: "Context limit reached".to_string(),
            detail_lines: vec![
                "The request exceeded the model context window.".to_string(),
                "Use /compact or /clear, then retry.".to_string(),
            ],
        };
    }

    if normalized.contains("invalid api key")
        || normalized.contains("unauthorized")
        || normalized.contains("authentication failed")
        || normalized.contains("token revoked")
        || normalized.contains("401")
    {
        return ErrorView {
            kind: ErrorKind::Authentication,
            title: "Authentication failed".to_string(),
            detail_lines: vec![
                "The current provider rejected the credentials.".to_string(),
                "Check provider settings, API keys, or environment variables.".to_string(),
            ],
        };
    }

    if normalized.contains("rate limit")
        || normalized.contains("too many requests")
        || normalized.contains("429")
    {
        return ErrorView {
            kind: ErrorKind::RateLimit,
            title: "Rate limited".to_string(),
            detail_lines: vec![
                "The provider asked us to slow down.".to_string(),
                "Wait and retry, or switch model/provider.".to_string(),
            ],
        };
    }

    if normalized.contains("credit")
        || normalized.contains("billing")
        || normalized.contains("quota")
        || normalized.contains("额度不足")
        || normalized.contains("403")
    {
        return ErrorView {
            kind: ErrorKind::ProviderRejected,
            title: "Provider rejected request".to_string(),
            detail_lines: vec![
                "Billing, quota, or org permissions blocked the request.".to_string(),
                "Check credits, billing, quota, or org access.".to_string(),
            ],
        };
    }

    if normalized.contains("timed out") || normalized.contains("timeout") {
        return ErrorView {
            kind: ErrorKind::Timeout,
            title: "Request timed out".to_string(),
            detail_lines: vec![
                "The model or tool did not finish in time.".to_string(),
                "Retry, reduce scope, or increase the timeout.".to_string(),
            ],
        };
    }

    let mut details = vec![truncate_error_summary(summary, 140)];
    let extra_lines = content.lines().filter(|line| !line.trim().is_empty()).count();
    if extra_lines > 1 {
        details.push(format!("{} more lines in full error output.", extra_lines - 1));
    }
    ErrorView {
        kind: ErrorKind::Generic,
        title: "Error".to_string(),
        detail_lines: details,
    }
}

fn first_nonempty_line(content: &str) -> Option<&str> {
    content.lines().map(str::trim).find(|line| !line.is_empty())
}

fn truncate_error_summary(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        format!("{}...", text.chars().take(max_chars).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_error_view;

    #[test]
    fn parses_context_limit_errors() {
        let view = parse_error_view(
            "OpenAI API error (400): This model's maximum context length is 128000 tokens.",
        );
        assert_eq!(view.kind, super::ErrorKind::ContextLimit);
        assert_eq!(view.title, "Context limit reached");
        assert!(view.detail_lines[1].contains("/compact"));
    }

    #[test]
    fn parses_authentication_errors() {
        let view = parse_error_view("Anthropic API error (401): invalid api key");
        assert_eq!(view.kind, super::ErrorKind::Authentication);
        assert_eq!(view.title, "Authentication failed");
    }

    #[test]
    fn falls_back_to_generic_summary() {
        let view = parse_error_view("something odd happened\nwith more detail");
        assert_eq!(view.kind, super::ErrorKind::Generic);
        assert_eq!(view.title, "Error");
        assert_eq!(view.detail_lines[0], "something odd happened");
    }

    #[test]
    fn generic_error_summary_truncates_long_api_lines() {
        let content = format!(
            "provider returned: {}",
            "x".repeat(200)
        );
        let view = parse_error_view(&content);
        assert_eq!(view.kind, super::ErrorKind::Generic);
        assert!(view.detail_lines[0].ends_with("..."));
        assert!(view.detail_lines[0].chars().count() < content.chars().count());
    }
}
