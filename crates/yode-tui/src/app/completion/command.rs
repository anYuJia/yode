use crate::commands::context::CompletionContext;
use crate::commands::registry::CommandRegistry;

/// State for slash command completion popup.
pub struct CommandCompletion {
    /// Candidates: (command_name_or_arg, description)
    pub candidates: Vec<(String, String)>,
    /// Currently selected index
    pub selected: Option<usize>,
    /// Index of the first visible item in the list
    pub window_start: usize,
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
            window_start: 0,
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
    pub fn update(
        &mut self,
        input_text: &str,
        is_single_line: bool,
        registry: &CommandRegistry,
        completion_ctx: &CompletionContext,
    ) {
        self.args_hint = None;
        self.completing_args = false;
        self.arg_prefix = None;
        self.window_start = 0;

        if !is_single_line || !input_text.starts_with('/') {
            self.close();
            return;
        }

        if input_text.contains(' ') {
            let parts: Vec<&str> = input_text.splitn(2, ' ').collect();
            let cmd_part = parts[0];
            let cmd_name = &cmd_part[1..];
            let after_cmd = parts.get(1).unwrap_or(&"");

            let arg_tokens: Vec<&str> = after_cmd.split_whitespace().collect();
            let (args_so_far, partial) = if after_cmd.ends_with(' ') || after_cmd.is_empty() {
                (arg_tokens.as_slice(), "")
            } else {
                let (completed, last) = arg_tokens.split_at(arg_tokens.len().saturating_sub(1));
                (completed, last.first().copied().unwrap_or(""))
            };

            let arg_candidates =
                registry.complete_args(cmd_name, args_so_far, partial, completion_ctx);

            if !arg_candidates.is_empty() {
                let prefix = if partial.is_empty() {
                    format!(
                        "{} {}",
                        cmd_part,
                        if after_cmd.is_empty() { "" } else { after_cmd }
                    )
                } else {
                    let prefix_end = input_text.len() - partial.len();
                    input_text[..prefix_end].to_string()
                };

                self.candidates = arg_candidates
                    .into_iter()
                    .map(|value| (value.clone(), String::new()))
                    .collect();
                self.completing_args = true;
                self.arg_prefix = Some(prefix);
                self.selected = Some(0);
            } else if let Some(hint) = registry.args_hint(cmd_name) {
                self.candidates.clear();
                self.selected = None;
                self.args_hint = Some(format!("{} {}", cmd_part, hint));
            } else {
                self.close();
            }
            return;
        }

        let query = &input_text[1..];
        let suggestions = registry.complete_command(query);

        let mut matches: Vec<(String, String)> = suggestions
            .into_iter()
            .map(|suggestion| (format!("/{}", suggestion.name), suggestion.description))
            .filter(|(name, _)| name != input_text)
            .collect();

        for (name, description) in &self.dynamic_commands {
            let full_name = format!("/{}", name);
            if full_name.starts_with(input_text) && full_name != input_text {
                if !matches.iter().any(|(existing, _)| existing == &full_name) {
                    matches.push((full_name, description.clone()));
                }
            }
        }

        matches.sort_by_key(|(command, _)| command.len());
        self.candidates = matches;

        if self.candidates.is_empty() {
            self.selected = None;
        } else if self.selected.map_or(true, |index| index >= self.candidates.len()) {
            self.selected = Some(0);
        }
    }

    pub fn close(&mut self) {
        self.candidates.clear();
        self.selected = None;
        self.window_start = 0;
        self.args_hint = None;
        self.completing_args = false;
        self.arg_prefix = None;
    }

    /// Accept the currently selected completion. Returns the full text to set in input.
    pub fn accept(&mut self) -> Option<String> {
        let result = self
            .selected
            .and_then(|index| self.candidates.get(index))
            .map(|(value, _)| {
                if self.completing_args {
                    format!("{}{}", self.arg_prefix.as_deref().unwrap_or(""), value)
                } else {
                    value.clone()
                }
            });
        self.close();
        result
    }

    /// Cycle to next candidate.
    pub fn cycle(&mut self) {
        cycle_selection(
            &self.candidates,
            &mut self.selected,
            &mut self.window_start,
            5,
            true,
        );
    }

    /// Cycle to previous candidate.
    pub fn cycle_back(&mut self) {
        cycle_selection(
            &self.candidates,
            &mut self.selected,
            &mut self.window_start,
            5,
            false,
        );
    }
}

fn cycle_selection<T>(
    candidates: &[T],
    selected: &mut Option<usize>,
    window_start: &mut usize,
    max_visible: usize,
    forward: bool,
) {
    if candidates.is_empty() {
        return;
    }
    let current = selected.unwrap_or(0);
    let total = candidates.len();

    if *window_start >= total {
        *window_start = 0;
    }

    if forward {
        let visible_end = *window_start + max_visible;
        if current + 1 >= total {
            *window_start = 0;
            *selected = Some(0);
        } else if selected.map_or(false, |index| index >= visible_end - 1) && visible_end < total {
            *window_start += 1;
            *selected = Some(current + 1);
        } else {
            *selected = Some(current + 1);
        }
    } else if current == 0 {
        *window_start = if total > max_visible {
            total - max_visible
        } else {
            0
        };
        *selected = Some(total - 1);
    } else if selected.map_or(false, |index| index == *window_start) && *window_start > 0 {
        *window_start -= 1;
        *selected = Some(current - 1);
    } else {
        *selected = Some(current - 1);
    }
}
