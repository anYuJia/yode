/// Completion state for slash commands and @file references.

/// Slash command definition with description and optional args hint.
pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub args_hint: Option<&'static str>,
}

pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand { name: "/help", description: "Show command list", args_hint: None },
    SlashCommand { name: "/clear", description: "Clear chat history", args_hint: Some("[context]") },
    SlashCommand { name: "/exit", description: "Quit application", args_hint: None },
    SlashCommand { name: "/model", description: "Show/switch model", args_hint: Some("<model-name>") },
    SlashCommand { name: "/tools", description: "List registered tools", args_hint: None },
    SlashCommand { name: "/compact", description: "Compress history", args_hint: Some("[keep_last=20]") },
    SlashCommand { name: "/cost", description: "Show token usage & cost", args_hint: None },
    SlashCommand { name: "/diff", description: "Show git diff", args_hint: None },
    SlashCommand { name: "/context", description: "Show context window usage", args_hint: None },
    SlashCommand { name: "/status", description: "Show session status", args_hint: None },
    SlashCommand { name: "/sessions", description: "List recent sessions", args_hint: None },
    SlashCommand { name: "/keys", description: "Show keyboard shortcuts", args_hint: None },
    SlashCommand { name: "/copy", description: "Copy last message to clipboard", args_hint: None },
    SlashCommand { name: "/bug", description: "Generate bug report", args_hint: None },
    SlashCommand { name: "/doctor", description: "Check environment health", args_hint: None },
    SlashCommand { name: "/config", description: "Show current configuration", args_hint: None },
    SlashCommand { name: "/version", description: "Show version info", args_hint: None },
    SlashCommand { name: "/history", description: "Show recent input history", args_hint: Some("[count]") },
    SlashCommand { name: "/time", description: "Show session timing", args_hint: None },
    SlashCommand { name: "/provider", description: "Switch provider", args_hint: Some("<name>") },
    SlashCommand { name: "/providers", description: "List available providers", args_hint: None },
];

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

    /// Update completions based on current input text.
    pub fn update(&mut self, input_text: &str, is_single_line: bool) {
        self.args_hint = None;

        if !is_single_line || !input_text.starts_with('/') {
            self.close();
            return;
        }

        // Check for "known command + space" → show args hint
        if input_text.contains(' ') {
            let cmd_part = input_text.split_whitespace().next().unwrap_or("");
            let hint = SLASH_COMMANDS.iter()
                .find(|c| c.name == cmd_part)
                .and_then(|c| c.args_hint);
            if let Some(h) = hint {
                self.candidates.clear();
                self.selected = None;
                self.args_hint = Some(format!("{} {}", cmd_part, h));
            } else {
                self.close();
            }
            return;
        }

        // Prefix matches first
        let mut prefix_matches: Vec<(String, String)> = SLASH_COMMANDS
            .iter()
            .filter(|cmd| cmd.name.starts_with(input_text) && cmd.name != input_text)
            .map(|cmd| (cmd.name.to_string(), cmd.description.to_string()))
            .collect();

        // Dynamic skill commands — prefix
        for (name, desc) in &self.dynamic_commands {
            let full_name = format!("/{}", name);
            if full_name.starts_with(input_text) && full_name != input_text {
                prefix_matches.push((full_name, desc.clone()));
            }
        }

        // Substring fallback: only when no prefix matches found
        if prefix_matches.is_empty() {
            let query = &input_text[1..]; // strip leading /
            let mut sub_matches: Vec<(String, String)> = SLASH_COMMANDS
                .iter()
                .filter(|cmd| {
                    let name_bare = &cmd.name[1..]; // strip /
                    name_bare.contains(query) && cmd.name != input_text
                })
                .map(|cmd| (cmd.name.to_string(), cmd.description.to_string()))
                .collect();

            for (name, desc) in &self.dynamic_commands {
                if name.contains(query) {
                    let full_name = format!("/{}", name);
                    if full_name != input_text {
                        sub_matches.push((full_name, desc.clone()));
                    }
                }
            }

            sub_matches.sort_by_key(|(cmd, _)| cmd.len());
            self.candidates = sub_matches;
        } else {
            prefix_matches.sort_by_key(|(cmd, _)| cmd.len());
            self.candidates = prefix_matches;
        }

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
