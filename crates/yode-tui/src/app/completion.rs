/// Completion state for slash commands and @file references.

use crate::commands::registry::CommandRegistry;

/// State for slash command completion popup.
pub struct CommandCompletion {
    /// Candidates: (command_name, description)
    pub candidates: Vec<(String, String)>,
    /// Currently selected index
    pub selected: Option<usize>,
    /// Additional dynamic commands (e.g., from skills)
    pub dynamic_commands: Vec<(String, String)>,
    /// Args hint to display when a known command + space is typed
    pub args_hint: Option<String>,
}

impl CommandCompletion {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
            selected: None,
            dynamic_commands: Vec::new(),
            args_hint: None,
        }
    }

    pub fn is_active(&self) -> bool {
        !self.candidates.is_empty() || self.args_hint.is_some()
    }

    /// Update completions based on current input text, sourcing from the CommandRegistry.
    pub fn update(&mut self, input_text: &str, is_single_line: bool, registry: &CommandRegistry) {
        self.args_hint = None;

        if !is_single_line || !input_text.starts_with('/') {
            self.close();
            return;
        }

        // Check for "known command + space" -> show args hint
        if input_text.contains(' ') {
            let cmd_part = input_text.split_whitespace().next().unwrap_or("");
            let cmd_name = &cmd_part[1..]; // strip leading /
            let hint = registry.args_hint(cmd_name);
            if let Some(h) = hint {
                self.candidates.clear();
                self.selected = None;
                self.args_hint = Some(format!("{} {}", cmd_part, h));
            } else {
                self.close();
            }
            return;
        }

        // Use registry for command name completion
        let query = &input_text[1..]; // strip leading /
        let suggestions = registry.complete_command(query);

        let mut matches: Vec<(String, String)> = suggestions
            .into_iter()
            .map(|s| (format!("/{}", s.name), s.description))
            .filter(|(name, _)| name != input_text)
            .collect();

        // Also include dynamic skill commands that aren't in the registry
        for (name, desc) in &self.dynamic_commands {
            let full_name = format!("/{}", name);
            if full_name.starts_with(input_text) && full_name != input_text {
                // Only add if not already present from registry
                if !matches.iter().any(|(n, _)| n == &full_name) {
                    matches.push((full_name, desc.clone()));
                }
            }
        }

        matches.sort_by_key(|(cmd, _)| cmd.len());
        self.candidates = matches;

        if self.candidates.is_empty() {
            self.selected = None;
        } else if self.selected.map_or(true, |i| i >= self.candidates.len()) {
            self.selected = Some(0);
        }
    }

    pub fn close(&mut self) {
        self.candidates.clear();
        self.selected = None;
        self.args_hint = None;
    }

    /// Accept the currently selected completion. Returns the command name if accepted.
    pub fn accept(&mut self) -> Option<String> {
        let result = self.selected
            .and_then(|idx| self.candidates.get(idx))
            .map(|(cmd, _)| cmd.clone());
        self.close();
        result
    }

    /// Cycle to next candidate.
    pub fn cycle(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        let idx = self.selected.unwrap_or(0);
        self.selected = Some((idx + 1) % self.candidates.len());
    }

    /// Cycle to previous candidate.
    pub fn cycle_back(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        let idx = self.selected.unwrap_or(0);
        self.selected = Some(if idx == 0 { self.candidates.len() - 1 } else { idx - 1 });
    }
}

/// State for @file path completion.
pub struct FileCompletion {
    pub candidates: Vec<String>,
    pub selected: Option<usize>,
}

impl FileCompletion {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
            selected: None,
        }
    }

    pub fn is_active(&self) -> bool {
        !self.candidates.is_empty()
    }

    /// Update file completions based on text after the last @.
    pub fn update(&mut self, full_text: &str) {
        if let Some(at_pos) = full_text.rfind('@') {
            let after_at = &full_text[at_pos + 1..];
            if !after_at.contains(' ') && after_at.len() < 200 {
                let prefix = after_at;
                let (dir, file_prefix) = if prefix.contains('/') {
                    let last_slash = prefix.rfind('/').unwrap();
                    (&prefix[..=last_slash], &prefix[last_slash + 1..])
                } else {
                    (".", prefix)
                };

                self.candidates.clear();
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with(file_prefix) && !name.starts_with('.') {
                            let full = if dir == "." {
                                name
                            } else {
                                format!("{}{}", dir, name)
                            };
                            let display = if entry.file_type().map_or(false, |t| t.is_dir()) {
                                format!("{}/", full)
                            } else {
                                full
                            };
                            self.candidates.push(display);
                        }
                    }
                }
                self.candidates.sort();
                self.selected = if self.candidates.is_empty() { None } else { Some(0) };
                return;
            }
        }
        self.close();
    }

    pub fn close(&mut self) {
        self.candidates.clear();
        self.selected = None;
    }

    /// Accept selected file path. Returns the file path to insert.
    pub fn accept(&mut self) -> Option<String> {
        let result = self.selected
            .and_then(|idx| self.candidates.get(idx).cloned());
        self.close();
        result
    }

    pub fn cycle(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        let idx = self.selected.unwrap_or(0);
        self.selected = Some((idx + 1) % self.candidates.len());
    }

    pub fn cycle_back(&mut self) {
        if self.candidates.is_empty() {
            return;
        }
        let idx = self.selected.unwrap_or(0);
        self.selected = Some(if idx == 0 { self.candidates.len() - 1 } else { idx - 1 });
    }
}
