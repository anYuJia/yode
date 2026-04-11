/// Input history state and Ctrl+R search mode.

#[derive(Default)]
struct SearchState {
    mode: bool,
    query: String,
    results: Vec<String>,
    index: Option<usize>,
}

/// Input history manager with search functionality.
pub struct HistoryState {
    /// All input history entries
    entries: Vec<String>,
    /// Maximum history size
    max_size: usize,
    /// Current position when browsing (None = not browsing)
    browse_index: Option<usize>,
    /// Saved input when user starts browsing history
    saved_input: Option<Vec<String>>,
    search: SearchState,
}

impl HistoryState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_size: 500,
            browse_index: None,
            saved_input: None,
            search: SearchState::default(),
        }
    }

    /// Add an entry to history (deduplicates consecutive).
    pub fn push(&mut self, entry: String) {
        if self.entries.last().map_or(true, |last| last != &entry) {
            self.entries.push(entry);
            if self.entries.len() > self.max_size {
                self.entries.remove(0);
            }
        }
    }

    /// Get all history entries (for persistence).
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    // ── Browse mode (Up/Down arrows) ────────────────────────────────

    /// Start browsing from the end, saving current input.
    pub fn start_browse(&mut self, current_input: Vec<String>) {
        if self.entries.is_empty() {
            return;
        }
        self.saved_input = Some(current_input);
        self.browse_index = Some(self.entries.len() - 1);
    }

    /// Whether currently browsing history.
    pub fn is_browsing(&self) -> bool {
        self.browse_index.is_some()
    }

    /// Move to previous (older) entry. Returns the entry text if moved.
    pub fn browse_prev(&mut self) -> Option<&str> {
        if let Some(idx) = self.browse_index {
            if idx > 0 {
                self.browse_index = Some(idx - 1);
                return Some(&self.entries[idx - 1]);
            }
        }
        None
    }

    /// Move to next (newer) entry. Returns entry text, or None to restore saved.
    pub fn browse_next(&mut self) -> BrowseResult {
        if let Some(idx) = self.browse_index {
            if idx < self.entries.len() - 1 {
                self.browse_index = Some(idx + 1);
                BrowseResult::Entry(self.entries[idx + 1].clone())
            } else {
                // Past the end — restore saved input
                self.browse_index = None;
                BrowseResult::Restore(
                    self.saved_input
                        .take()
                        .unwrap_or_else(|| vec![String::new()]),
                )
            }
        } else {
            BrowseResult::None
        }
    }

    /// Get the current browse entry.
    pub fn current_browse_entry(&self) -> Option<&str> {
        self.browse_index.map(|idx| self.entries[idx].as_str())
    }

    /// Cancel browsing, restoring saved input.
    pub fn cancel_browse(&mut self) -> Option<Vec<String>> {
        self.browse_index = None;
        self.saved_input.take()
    }

    // ── Search mode (Ctrl+R) ────────────────────────────────────────

    /// Enter search mode.
    pub fn enter_search(&mut self) {
        self.search.mode = true;
        self.search.query.clear();
        self.refresh_search();
    }

    pub fn is_searching(&self) -> bool {
        self.search.mode
    }

    pub fn search_query(&self) -> &str {
        &self.search.query
    }

    pub fn search_results(&self) -> &[String] {
        &self.search.results
    }

    pub fn search_index(&self) -> Option<usize> {
        self.search.index
    }

    pub fn current_search_result(&self) -> Option<&str> {
        self.search
            .index
            .and_then(|idx| self.search.results.get(idx).map(String::as_str))
    }

    pub fn append_search_char(&mut self, c: char) {
        self.search.query.push(c);
        self.refresh_search();
    }

    pub fn pop_search_char(&mut self) {
        self.search.query.pop();
        self.refresh_search();
    }

    /// Cycle to next search result.
    pub fn search_next(&mut self) {
        if let Some(idx) = self.search.index {
            if idx + 1 < self.search.results.len() {
                self.search.index = Some(idx + 1);
            }
        }
    }

    /// Exit search mode. Returns selected text if accepted.
    pub fn exit_search(&mut self, accept: bool) -> Option<String> {
        let result = if accept {
            self.search
                .index
                .and_then(|idx| self.search.results.get(idx).cloned())
        } else {
            None
        };
        self.search = SearchState::default();
        result
    }

    fn refresh_search(&mut self) {
        let query = self.search.query.to_lowercase();
        self.search.results = self
            .entries
            .iter()
            .rev()
            .filter(|h| h.to_lowercase().contains(&query))
            .cloned()
            .collect();
        self.search.index = if self.search.results.is_empty() {
            None
        } else {
            Some(0)
        };
    }
}

pub enum BrowseResult {
    Entry(String),
    Restore(Vec<String>),
    None,
}

#[cfg(test)]
mod tests {
    use super::{BrowseResult, HistoryState};

    #[test]
    fn push_deduplicates_only_consecutive_entries() {
        let mut history = HistoryState::new();
        history.push("a".into());
        history.push("a".into());
        history.push("b".into());
        history.push("a".into());

        assert_eq!(
            history.entries(),
            &["a".to_string(), "b".to_string(), "a".to_string()]
        );
    }

    #[test]
    fn browse_restores_saved_input_after_latest_entry() {
        let mut history = HistoryState::new();
        history.push("first".into());
        history.push("second".into());

        history.start_browse(vec!["draft".into()]);
        assert_eq!(history.current_browse_entry(), Some("second"));

        match history.browse_next() {
            BrowseResult::Restore(lines) => assert_eq!(lines, vec!["draft".to_string()]),
            _ => panic!("expected restore after moving past the newest history entry"),
        }
    }

    #[test]
    fn search_state_filters_and_resets_cleanly() {
        let mut history = HistoryState::new();
        history.push("cargo test".into());
        history.push("git status".into());

        history.enter_search();
        assert!(history.is_searching());
        history.append_search_char('g');
        history.append_search_char('i');

        assert_eq!(history.search_results(), &["git status".to_string()]);
        assert_eq!(history.current_search_result(), Some("git status"));

        let selected = history.exit_search(true);
        assert_eq!(selected.as_deref(), Some("git status"));
        assert!(!history.is_searching());
        assert!(history.search_results().is_empty());
    }
}
