use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct DenialRecordView {
    pub tool_name: String,
    pub count: u32,
    pub consecutive: u32,
    pub last_at: String,
}

#[derive(Debug)]
struct DenialState {
    count: u32,
    consecutive: u32,
    last_time: Instant,
    last_at: String,
}

#[derive(Debug)]
pub struct DenialTracker {
    states: HashMap<String, DenialState>,
    expiry: Duration,
}

impl DenialTracker {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            expiry: Duration::from_secs(30 * 60),
        }
    }

    pub fn record_denial(&mut self, key: &str) {
        let state = self.states.entry(key.to_string()).or_insert(DenialState {
            count: 0,
            consecutive: 0,
            last_time: Instant::now(),
            last_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        });
        state.count += 1;
        state.consecutive += 1;
        state.last_time = Instant::now();
        state.last_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        self.cleanup_expired();
    }

    pub fn record_success(&mut self, key: &str) {
        if let Some(state) = self.states.get_mut(key) {
            state.consecutive = 0;
        }
    }

    /// Whether the user has denied this tool type enough times to warrant auto-skipping.
    pub fn should_auto_skip(&self, key: &str) -> bool {
        if let Some(state) = self.states.get(key) {
            let threshold = match key {
                "bash" => 5,
                "write_file" | "edit_file" => 3,
                _ => 3,
            };
            state.consecutive >= threshold
        } else {
            false
        }
    }

    pub fn denial_count(&self, key: &str) -> u32 {
        self.states.get(key).map(|s| s.count).unwrap_or(0)
    }

    pub fn recent_entries(&self, limit: usize) -> Vec<DenialRecordView> {
        let mut entries = self
            .states
            .iter()
            .map(|(tool_name, state)| (tool_name, state))
            .collect::<Vec<_>>();
        entries.sort_by(|a, b| b.1.last_time.cmp(&a.1.last_time));
        entries
            .into_iter()
            .take(limit)
            .map(|(tool_name, state)| DenialRecordView {
                tool_name: tool_name.clone(),
                count: state.count,
                consecutive: state.consecutive,
                last_at: state.last_at.clone(),
            })
            .collect()
    }

    fn cleanup_expired(&mut self) {
        let now = Instant::now();
        self.states
            .retain(|_, state| now.duration_since(state.last_time) < self.expiry);
    }
}

impl Default for DenialTracker {
    fn default() -> Self {
        Self::new()
    }
}
