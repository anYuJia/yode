/// Completion state for slash commands and @file references.

use crate::commands::context::CompletionContext;
use crate::commands::registry::CommandRegistry;

/// State for slash command completion popup.
pub struct CommandCompletion {
    /// Candidates: (command_name_or_arg, description)
    pub candidates: Vec<(String, String)>,
    /// Currently selected index
    pub selected: Option<usize>,
    /// Additional dynamic commands (e.g., from skills)
    pub dynamic_commands: Vec<(String, String)>,
    /// Args hint to display when a known command + space is typed (no arg completions available)
    pub args_hint: Option<String>,
    /// Whether we're completing args (affects how Tab accept works)
    pub completing_args: bool,
    /// The command prefix when completing args (e.g. "/provider ")
    pub arg_prefix: Option<String>,
}

impl CommandCompletion {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
            selected: None,
            dynamic_commands: Vec::new(),
            args_hint: None,
            completing_args: false,
            arg_prefix: None,
        }
    }

    pub fn is_active(&self) -> bool {
        !self.candidates.is_empty() || self.args_hint.is_some()
    }

    /// Update completions based on current input text, sourcing from the CommandRegistry.
    pub fn update(&mut self, input_text: &str, is_single_line: bool, registry: &CommandRegistry, completion_ctx: &CompletionContext) {
        self.args_hint = None;
        self.completing_args = false;
        self.arg_prefix = None;

        if !is_single_line || !input_text.starts_with('/') {
            self.close();
            return;
        }

        // Check for "known command + space" -> try arg completion
        if input_text.contains(' ') {
            let parts: Vec<&str> = input_text.splitn(2, ' ').collect();
            let cmd_part = parts[0]; // e.g. "/provider"
            let cmd_name = &cmd_part[1..]; // e.g. "provider"
            let after_cmd = parts.get(1).unwrap_or(&""); // e.g. "ope" or ""

            // Parse completed args and current partial
            let arg_tokens: Vec<&str> = after_cmd.split_whitespace().collect();
            let (args_so_far, partial) = if after_cmd.ends_with(' ') || after_cmd.is_empty() {
                // Cursor after space — completing next arg
                (arg_tokens.as_slice(), "")
            } else {
                // Cursor mid-word — completing current partial
                let (completed, last) = arg_tokens.split_at(arg_tokens.len().saturating_sub(1));
                (completed, last.first().copied().unwrap_or(""))
            };

            let arg_candidates = registry.complete_args(cmd_name, args_so_far, partial, completion_ctx);

            if !arg_candidates.is_empty() {
                // Build prefix: everything before the partial being completed
                let prefix = if partial.is_empty() {
                    format!("{} {}", cmd_part, if after_cmd.is_empty() { "" } else { after_cmd })
                } else {
                    let prefix_end = input_text.len() - partial.len();
                    input_text[..prefix_end].to_string()
                };

                self.candidates = arg_candidates.into_iter()
                    .map(|val| (val.clone(), String::new()))
                    .collect();
                self.completing_args = true;
                self.arg_prefix = Some(prefix);
                self.selected = Some(0);
            } else {
                // No arg completions — show hint if available
                if let Some(h) = registry.args_hint(cmd_name) {
                    self.candidates.clear();
                    self.selected = None;
                    self.args_hint = Some(format!("{} {}", cmd_part, h));
                } else {
                    self.close();
                }
            }
            return;
        }

        // Command name completion
        let query = &input_text[1..]; // strip leading /
        let suggestions = registry.complete_command(query);

        let mut matches: Vec<(String, String)> = suggestions
            .into_iter()
            .map(|s| (format!("/{}", s.name), s.description))
            .filter(|(name, _)| name != input_text)
            .collect();

        // Also include dynamic skill commands
        for (name, desc) in &self.dynamic_commands {
            let full_name = format!("/{}", name);
            if full_name.starts_with(input_text) && full_name != input_text {
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
        self.completing_args = false;
        self.arg_prefix = None;
    }

    /// Accept the currently selected completion. Returns the full text to set in input.
    pub fn accept(&mut self) -> Option<String> {
        let result = self.selected
            .and_then(|idx| self.candidates.get(idx))
            .map(|(val, _)| {
                if self.completing_args {
                    // For args: prefix + selected value
                    format!("{}{}", self.arg_prefix.as_deref().unwrap_or(""), val)
                } else {
                    // For commands: just the command name
                    val.clone()
                }
            });
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
