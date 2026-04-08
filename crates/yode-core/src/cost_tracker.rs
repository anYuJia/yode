use std::time::Instant;

use serde::{Deserialize, Serialize};

// ─── Model Cost Tables ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub struct ModelCosts {
    /// Price per million input tokens (USD)
    pub input_per_million: f64,
    /// Price per million output tokens (USD)
    pub output_per_million: f64,
    /// Price per million cache write tokens (USD)
    pub cache_write_per_million: f64,
    /// Price per million cache read tokens (USD)
    pub cache_read_per_million: f64,
}

impl ModelCosts {
    /// Look up costs by model name.
    pub fn for_model(model: &str) -> Self {
        let m = model.to_lowercase();
        if m.contains("claude-opus") || m.contains("claude-3-opus") {
            Self {
                input_per_million: 15.0,
                output_per_million: 75.0,
                cache_write_per_million: 18.75,
                cache_read_per_million: 1.5,
            }
        } else if m.contains("claude-sonnet-4")
            || m.contains("claude-3-5-sonnet")
            || m.contains("claude-3.5-sonnet")
        {
            Self {
                input_per_million: 3.0,
                output_per_million: 15.0,
                cache_write_per_million: 3.75,
                cache_read_per_million: 0.3,
            }
        } else if m.contains("claude-haiku")
            || m.contains("claude-3-haiku")
            || m.contains("claude-3.5-haiku")
        {
            Self {
                input_per_million: 0.80,
                output_per_million: 4.0,
                cache_write_per_million: 1.0,
                cache_read_per_million: 0.08,
            }
        } else if m.contains("gpt-4o") {
            Self {
                input_per_million: 2.50,
                output_per_million: 10.0,
                cache_write_per_million: 2.50,
                cache_read_per_million: 1.25,
            }
        } else if m.contains("gpt-4-turbo") {
            Self {
                input_per_million: 10.0,
                output_per_million: 30.0,
                cache_write_per_million: 10.0,
                cache_read_per_million: 5.0,
            }
        } else if m.contains("gpt-3.5") {
            Self {
                input_per_million: 0.50,
                output_per_million: 1.50,
                cache_write_per_million: 0.50,
                cache_read_per_million: 0.25,
            }
        } else if m.contains("deepseek") {
            Self {
                input_per_million: 0.27,
                output_per_million: 1.10,
                cache_write_per_million: 0.27,
                cache_read_per_million: 0.07,
            }
        } else {
            // Default to moderate pricing
            Self {
                input_per_million: 3.0,
                output_per_million: 15.0,
                cache_write_per_million: 3.75,
                cache_read_per_million: 0.3,
            }
        }
    }
}

// ─── Usage Data ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UsageData {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_write_tokens: u64,
    pub cache_read_tokens: u64,
    pub api_calls: u64,
    pub tool_calls: u64,
}

impl UsageData {
    pub fn total_tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens
    }
}

// ─── Cost Tracker ───────────────────────────────────────────────────────────

/// Tracks token usage and estimated cost for a session.
pub struct CostTracker {
    model: String,
    costs: ModelCosts,
    usage: UsageData,
    session_start: Instant,
    /// Optional budget limit in USD
    budget_limit: Option<f64>,
}

impl CostTracker {
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            costs: ModelCosts::for_model(model),
            usage: UsageData::default(),
            session_start: Instant::now(),
            budget_limit: None,
        }
    }

    pub fn set_budget_limit(&mut self, limit: f64) {
        self.budget_limit = Some(limit);
    }

    /// Update model (e.g., when user switches models mid-session).
    pub fn set_model(&mut self, model: &str) {
        self.model = model.to_string();
        self.costs = ModelCosts::for_model(model);
    }

    /// Record usage from an API response.
    pub fn record_usage(&mut self, input_tokens: u64, output_tokens: u64) {
        self.usage.input_tokens += input_tokens;
        self.usage.output_tokens += output_tokens;
        self.usage.api_calls += 1;
    }

    /// Record cache usage from an API response.
    pub fn record_cache_usage(&mut self, cache_write: u64, cache_read: u64) {
        self.usage.cache_write_tokens += cache_write;
        self.usage.cache_read_tokens += cache_read;
    }

    /// Record a tool call.
    pub fn record_tool_call(&mut self) {
        self.usage.tool_calls += 1;
    }

    /// Get the estimated total cost in USD.
    pub fn estimated_cost(&self) -> f64 {
        let input_cost =
            self.usage.input_tokens as f64 * self.costs.input_per_million / 1_000_000.0;
        let output_cost =
            self.usage.output_tokens as f64 * self.costs.output_per_million / 1_000_000.0;
        let cache_write_cost =
            self.usage.cache_write_tokens as f64 * self.costs.cache_write_per_million / 1_000_000.0;
        let cache_read_cost =
            self.usage.cache_read_tokens as f64 * self.costs.cache_read_per_million / 1_000_000.0;
        input_cost + output_cost + cache_write_cost + cache_read_cost
    }

    /// Check if budget limit has been exceeded.
    pub fn is_over_budget(&self) -> bool {
        if let Some(limit) = self.budget_limit {
            self.estimated_cost() > limit
        } else {
            false
        }
    }

    /// Remaining budget (None if no limit set).
    pub fn remaining_budget(&self) -> Option<f64> {
        self.budget_limit
            .map(|limit| (limit - self.estimated_cost()).max(0.0))
    }

    pub fn usage(&self) -> &UsageData {
        &self.usage
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    pub fn session_duration(&self) -> std::time::Duration {
        self.session_start.elapsed()
    }

    /// Format a human-readable cost summary.
    pub fn summary(&self) -> String {
        let cost = self.estimated_cost();
        let duration = self.session_duration();
        let mins = duration.as_secs() / 60;
        let secs = duration.as_secs() % 60;

        let mut s = format!(
            "Cost: ${:.4} | Tokens: {}in/{}out | API calls: {} | Tools: {} | Time: {}m{}s",
            cost,
            format_tokens(self.usage.input_tokens),
            format_tokens(self.usage.output_tokens),
            self.usage.api_calls,
            self.usage.tool_calls,
            mins,
            secs,
        );

        if let Some(limit) = self.budget_limit {
            s.push_str(&format!(" | Budget: ${:.2}/{:.2}", cost, limit));
        }

        s
    }
}

fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_costs_lookup() {
        let costs = ModelCosts::for_model("claude-sonnet-4-20250514");
        assert_eq!(costs.input_per_million, 3.0);
        assert_eq!(costs.output_per_million, 15.0);

        let costs = ModelCosts::for_model("claude-opus-4");
        assert_eq!(costs.input_per_million, 15.0);

        let costs = ModelCosts::for_model("gpt-4o");
        assert_eq!(costs.input_per_million, 2.50);
    }

    #[test]
    fn test_cost_calculation() {
        let mut tracker = CostTracker::new("claude-sonnet-4");
        tracker.record_usage(1_000_000, 100_000); // 1M in, 100K out
        let cost = tracker.estimated_cost();
        // 1M * 3.0/1M + 100K * 15.0/1M = 3.0 + 1.5 = 4.5
        assert!((cost - 4.5).abs() < 0.001);
    }

    #[test]
    fn test_budget_limit() {
        let mut tracker = CostTracker::new("claude-sonnet-4");
        tracker.set_budget_limit(5.0);
        tracker.record_usage(1_000_000, 100_000); // $4.50
        assert!(!tracker.is_over_budget());

        tracker.record_usage(500_000, 0); // +$1.50 = $6.00
        assert!(tracker.is_over_budget());
    }

    #[test]
    fn test_usage_tracking() {
        let mut tracker = CostTracker::new("claude-sonnet-4");
        tracker.record_usage(100, 50);
        tracker.record_usage(200, 100);
        tracker.record_tool_call();
        tracker.record_tool_call();

        let usage = tracker.usage();
        assert_eq!(usage.input_tokens, 300);
        assert_eq!(usage.output_tokens, 150);
        assert_eq!(usage.api_calls, 2);
        assert_eq!(usage.tool_calls, 2);
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5K");
        assert_eq!(format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn test_summary() {
        let mut tracker = CostTracker::new("claude-sonnet-4");
        tracker.record_usage(10000, 5000);
        let summary = tracker.summary();
        assert!(summary.contains("Cost: $"));
        assert!(summary.contains("Tokens:"));
    }
}
