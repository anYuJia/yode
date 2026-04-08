/// Input history state and Ctrl+R search mode.

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
    /// Whether in Ctrl+R search mode
    pub search_mode: bool,
    /// Current search query
    pub search_query: String,
    /// Filtered search results (most recent first)
    pub search_results: Vec<String>,
    /// Selected index in search results
    pub search_index: Option<usize>,
}

impl HistoryState {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            max_size: 500,
            browse_index: None,
            saved_input: None,
            search_mode: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_index: None,
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
        self.search_mode = true;
        self.search_query.clear();
        self.search_results = self.entries.iter().rev().cloned().collect();
        self.search_index = if self.search_results.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    /// Update search filter.
    pub fn update_search(&mut self) {
        let query = self.search_query.to_lowercase();
        self.search_results = self
            .entries
            .iter()
            .rev()
            .filter(|h| h.to_lowercase().contains(&query))
            .cloned()
            .collect();
        self.search_index = if self.search_results.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    /// Cycle to next search result.
    pub fn search_next(&mut self) {
        if let Some(idx) = self.search_index {
            if idx + 1 < self.search_results.len() {
                self.search_index = Some(idx + 1);
            }
        }
    }

    /// Exit search mode. Returns selected text if accepted.
    pub fn exit_search(&mut self, accept: bool) -> Option<String> {
        let result = if accept {
            self.search_index
                .and_then(|idx| self.search_results.get(idx).cloned())
        } else {
            None
        };
        self.search_mode = false;
        self.search_query.clear();
        self.search_results.clear();
        self.search_index = None;
        result
    }
}

pub enum BrowseResult {
    Entry(String),
    Restore(Vec<String>),
    None,
}
