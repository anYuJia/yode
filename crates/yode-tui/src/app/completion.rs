/// Completion state for slash commands and @file references.

/// Slash command definition with description.
pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
}

pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand { name: "/help", description: "Show command list" },
    SlashCommand { name: "/clear", description: "Clear chat history" },
    SlashCommand { name: "/exit", description: "Quit application" },
    SlashCommand { name: "/model", description: "Show current model" },
    SlashCommand { name: "/tools", description: "List registered tools" },
    SlashCommand { name: "/compact", description: "Compress history" },
    SlashCommand { name: "/cost", description: "Show token usage & cost" },
    SlashCommand { name: "/diff", description: "Show git diff" },
    SlashCommand { name: "/context", description: "Show context window usage" },
    SlashCommand { name: "/status", description: "Show session status" },
    SlashCommand { name: "/sessions", description: "List recent sessions" },
    SlashCommand { name: "/keys", description: "Show keyboard shortcuts" },
    SlashCommand { name: "/copy", description: "Copy last message to clipboard" },
    SlashCommand { name: "/bug", description: "Generate bug report" },
    SlashCommand { name: "/doctor", description: "Check environment health" },
    SlashCommand { name: "/config", description: "Show current configuration" },
];

/// State for slash command completion popup.
pub struct CommandCompletion {
    /// Candidates: (command_name, description)
    pub candidates: Vec<(String, String)>,
    /// Currently selected index
    pub selected: Option<usize>,
    /// Additional dynamic commands (e.g., from skills)
    pub dynamic_commands: Vec<(String, String)>,
}

impl CommandCompletion {
    pub fn new() -> Self {
        Self {
            candidates: Vec::new(),
            selected: None,
            dynamic_commands: Vec::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        !self.candidates.is_empty()
    }

    /// Update completions based on current input text.
    pub fn update(&mut self, input_text: &str, is_single_line: bool) {
        if is_single_line && input_text.starts_with('/') && !input_text.contains(' ') {
            // Static commands
            let mut all: Vec<(String, String)> = SLASH_COMMANDS
                .iter()
                .filter(|cmd| cmd.name.starts_with(input_text) && cmd.name != input_text)
                .map(|cmd| (cmd.name.to_string(), cmd.description.to_string()))
                .collect();

            // Dynamic skill commands
            for (name, desc) in &self.dynamic_commands {
                let full_name = format!("/{}", name);
                if full_name.starts_with(input_text) && full_name != input_text {
                    all.push((full_name, desc.clone()));
                }
            }

            self.candidates = all;
            if self.candidates.is_empty() {
                self.selected = None;
            } else if self.selected.is_none() {
                self.selected = Some(0);
            }
        } else {
            self.close();
        }
    }

    pub fn close(&mut self) {
        self.candidates.clear();
        self.selected = None;
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
}
