/// Slash command execution and shell command handling.

use std::path::PathBuf;
use std::sync::Arc;

use arboard::Clipboard;
use tokio::sync::Mutex;
use yode_core::db::Database;
use yode_core::engine::AgentEngine;
use yode_tools::registry::ToolRegistry;

use super::completion::SLASH_COMMANDS;
use super::{App, ChatEntry, ChatRole};

/// Dangerous command patterns for safety warnings.
const DANGEROUS_PATTERNS: &[&str] = &[
    "rm -rf",
    "rm -r /",
    "rmdir /",
    "mkfs",
    "dd if=",
    ":(){",           // fork bomb
    "chmod -R 777",
    "chmod -R 000",
    "chown -R",
    "> /dev/sda",
    "wget | sh",
    "curl | sh",
    "curl | bash",
    "wget | bash",
    "shutdown",
    "reboot",
    "init 0",
    "kill -9 -1",
    "pkill -9",
    "DROP TABLE",
    "DROP DATABASE",
    "TRUNCATE",
    "--no-preserve-root",
];

/// Check if a command is potentially dangerous.
pub fn is_dangerous_command(cmd: &str) -> Option<&'static str> {
    let lower = cmd.to_lowercase();
    for pattern in DANGEROUS_PATTERNS {
        if lower.contains(&pattern.to_lowercase()) {
            return Some(pattern);
        }
    }
    None
}

impl App {
    /// Handle slash commands. Returns true if input was a command.
    pub(crate) fn handle_slash_command(
        &mut self,
        input: &str,
        tools: &Arc<ToolRegistry>,
        engine: &Arc<Mutex<AgentEngine>>,
    ) -> bool {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return false;
        }

        let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
        let cmd = parts[0];
        let arg = parts.get(1).copied().unwrap_or("").trim();

        match cmd {
            "/help" => {
                let mut help = String::from("Available commands:\n");
                for sc in SLASH_COMMANDS {
                    help.push_str(&format!("  {:<12} — {}\n", sc.name, sc.description));
                }
                help.push_str("\nType /keys for keyboard shortcut reference.");
                self.add_system_message(help);
            }
            "/keys" => {
                let keys = concat!(
                    "Keyboard shortcuts:\n",
                    "\n",
                    "  Editing:\n",
                    "    Ctrl+A / Home   — Move to line start\n",
                    "    Ctrl+E / End    — Move to line end\n",
                    "    Ctrl+U          — Clear entire line\n",
                    "    Ctrl+K          — Delete to end of line\n",
                    "    Ctrl+W          — Delete previous word\n",
                    "    Ctrl+J          — Insert newline\n",
                    "    Shift+Enter     — Insert newline\n",
                    "    Tab             — Autocomplete\n",
                    "    Shift+Tab       — Reverse autocomplete\n",
                    "\n",
                    "  Navigation:\n",
                    "    Up/Down         — Browse history (single-line) or navigate (multi-line)\n",
                    "    Ctrl+R          — Reverse search history\n",
                    "    PageUp/PageDown — Scroll chat\n",
                    "    Ctrl+End        — Scroll to bottom\n",
                    "\n",
                    "  Session:\n",
                    "    Esc / Ctrl+C    — Stop generation\n",
                    "    Ctrl+L          — Clear screen\n",
                    "    Shift+Tab       — Cycle permission mode (when no popup)\n",
                    "\n",
                    "  Special input:\n",
                    "    !command        — Execute shell command directly\n",
                    "    @file           — Attach file as context\n",
                    "    /command        — Slash commands\n",
                );
                self.add_system_message(keys.to_string());
            }
            "/clear" => {
                self.chat_entries.clear();
                self.add_system_message("Chat history cleared.".to_string());
            }
            "/exit" => {
                self.should_quit = true;
            }
            "/model" => {
                if arg.is_empty() {
                    // Show current model + available models
                    let models_list = if self.provider_models.is_empty() {
                        "  (unrestricted)".to_string()
                    } else {
                        self.provider_models.iter()
                            .map(|m| {
                                if *m == self.session.model {
                                    format!("  * {} (current)", m)
                                } else {
                                    format!("    {}", m)
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    };
                    self.add_system_message(format!(
                        "Current model: {}\nProvider: {}\nAvailable models:\n{}",
                        self.session.model, self.provider_name, models_list
                    ));
                } else {
                    // Switch model
                    let new_model = arg.to_string();
                    if !self.provider_models.is_empty() && !self.provider_models.contains(&new_model) {
                        self.add_system_message(format!(
                            "Model '{}' is not available for provider '{}'. Available models:\n  {}",
                            new_model, self.provider_name,
                            self.provider_models.join("\n  ")
                        ));
                    } else {
                        self.session.model = new_model.clone();
                        if let Ok(mut eng) = engine.try_lock() {
                            eng.set_model(new_model.clone());
                        }
                        self.add_system_message(format!("Switched to model: {}", new_model));
                    }
                }
            }
            "/provider" => {
                if arg.is_empty() {
                    self.add_system_message(format!(
                        "Current provider: {}\nUse /provider <name> to switch, /providers to list all.",
                        self.provider_name
                    ));
                } else {
                    let new_provider = arg.to_string();
                    if let Some(provider) = self.provider_registry.get(&new_provider) {
                        let new_models = self.all_provider_models.get(&new_provider).cloned().unwrap_or_default();
                        let new_model = new_models.first().cloned().unwrap_or_else(|| self.session.model.clone());
                        if let Ok(mut eng) = engine.try_lock() {
                            eng.set_provider(provider, new_provider.clone());
                            eng.set_model(new_model.clone());
                        }
                        self.provider_name = new_provider.clone();
                        self.provider_models = new_models;
                        self.session.model = new_model.clone();
                        self.add_system_message(format!(
                            "Switched to provider: {}, model: {}", new_provider, new_model
                        ));
                    } else {
                        let available: Vec<String> = self.all_provider_models.keys().cloned().collect();
                        self.add_system_message(format!(
                            "Provider '{}' not found. Available: {}", new_provider, available.join(", ")
                        ));
                    }
                }
            }
            "/providers" => {
                let mut lines = String::from("Available providers:\n");
                for (name, models) in &self.all_provider_models {
                    let marker = if *name == self.provider_name { " *" } else { "  " };
                    let model_str = if models.is_empty() {
                        "(unrestricted)".to_string()
                    } else {
                        models.join(", ")
                    };
                    lines.push_str(&format!("{} {:<15} — {}\n", marker, name, model_str));
                }
                self.add_system_message(lines);
            }
            "/tools" => {
                let tool_list: Vec<String> = tools
                    .definitions()
                    .iter()
                    .map(|t| format!("  {:<20} — {}", t.name, t.description))
                    .collect();
                self.add_system_message(format!(
                    "Registered tools ({}):\n{}",
                    tool_list.len(),
                    tool_list.join("\n")
                ));
            }
            "/compact" => {
                if self.chat_entries.len() > 20 {
                    let start = self.chat_entries.len() - 20;
                    self.chat_entries = self.chat_entries[start..].to_vec();
                }
                self.add_system_message("History compacted.".to_string());
            }
            "/cost" => {
                let cost = estimate_cost(&self.session.model, self.session.input_tokens, self.session.output_tokens);
                self.add_system_message(format!(
                    "Token usage:\n  Input tokens:  {}\n  Output tokens: {}\n  Total tokens:  {}\n  Tool calls:    {}\n  Est. cost:     ${:.4}",
                    self.session.input_tokens, self.session.output_tokens,
                    self.session.total_tokens, self.session.tool_call_count, cost
                ));
            }
            "/diff" => {
                let output = std::process::Command::new("git")
                    .args(["diff", "--stat"])
                    .output();
                let content = match output {
                    Ok(o) if o.status.success() => {
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        if stdout.is_empty() {
                            "No uncommitted changes.".to_string()
                        } else {
                            stdout.to_string()
                        }
                    }
                    Ok(o) => {
                        let stderr = String::from_utf8_lossy(&o.stderr);
                        format!("git error: {}", stderr.trim())
                    }
                    Err(e) => format!("Failed to run git: {}", e),
                };
                self.add_system_message(format!("Git diff:\n{}", content));
            }
            "/context" => {
                let total_chars: usize = self.chat_entries.iter().map(|e| e.content.len()).sum();
                let est_tokens = total_chars / 4;
                let pct = if self.session.total_tokens > 0 {
                    (est_tokens as f64 / 128000.0 * 100.0).min(100.0)
                } else {
                    0.0
                };
                self.add_system_message(format!(
                    "Context window:\n  Chat entries:    {}\n  Est. context:    ~{} tokens\n  API tokens used: {}\n  Window usage:    {:.1}%",
                    self.chat_entries.len(), est_tokens, self.session.total_tokens, pct
                ));
            }
            "/status" => {
                let session_short = &self.session.session_id[..self.session.session_id.len().min(8)];
                let always_allow = if self.session.always_allow_tools.is_empty() {
                    "none".to_string()
                } else {
                    self.session.always_allow_tools.join(", ")
                };
                let duration = self.session_start.elapsed();
                let mins = duration.as_secs() / 60;
                let secs = duration.as_secs() % 60;
                let cost = estimate_cost(&self.session.model, self.session.input_tokens, self.session.output_tokens);
                self.add_system_message(format!(
                    "Session status:\n  Session:         {}\n  Model:           {}\n  Working dir:     {}\n  Permission mode: {}\n  Duration:        {}m {}s\n  Tokens:          {} (in: {}, out: {})\n  Tool calls:      {}\n  Est. cost:       ${:.4}\n  Always-allow:    {}\n  Terminal:        {}",
                    session_short, self.session.model, self.session.working_dir,
                    self.session.permission_mode.label(),
                    mins, secs,
                    self.session.total_tokens, self.session.input_tokens, self.session.output_tokens,
                    self.session.tool_call_count, cost, always_allow,
                    self.terminal_caps.summary()
                ));
            }
            "/sessions" => {
                // Open DB to list recent sessions
                let db_path = dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".yode")
                    .join("sessions.db");
                match Database::open(&db_path) {
                    Ok(db) => {
                        match db.list_sessions(10) {
                            Ok(sessions) if sessions.is_empty() => {
                                self.add_system_message("No saved sessions found.".to_string());
                            }
                            Ok(sessions) => {
                                let mut lines = String::from("Recent sessions:\n");
                                for s in &sessions {
                                    let id_short = &s.id[..s.id.len().min(8)];
                                    let age = chrono::Utc::now().signed_duration_since(s.updated_at);
                                    let age_str = if age.num_days() > 0 {
                                        format!("{}d ago", age.num_days())
                                    } else if age.num_hours() > 0 {
                                        format!("{}h ago", age.num_hours())
                                    } else {
                                        format!("{}m ago", age.num_minutes().max(1))
                                    };
                                    let name = s.name.as_deref().unwrap_or("-");
                                    lines.push_str(&format!(
                                        "  {}  {:<12} {:<8} {}\n",
                                        id_short, s.model, age_str, name
                                    ));
                                }
                                lines.push_str("\nResume with: yode --resume <session-id>");
                                self.add_system_message(lines);
                            }
                            Err(e) => {
                                self.add_system_message(format!("Failed to list sessions: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        self.add_system_message(format!("Failed to open session database: {}", e));
                    }
                }
            }
            "/copy" => {
                // Find last assistant message
                let last_assistant = self.chat_entries.iter().rev().find(|e| {
                    matches!(e.role, ChatRole::Assistant)
                });

                if let Some(entry) = last_assistant {
                    match Clipboard::new() {
                        Ok(mut clipboard) => {
                            match clipboard.set_text(&entry.content) {
                                Ok(_) => {
                                    let preview: String = entry.content.chars().take(50).collect();
                                    let ellipsis = if entry.content.len() > 50 { "..." } else { "" };
                                    self.add_system_message(format!(
                                        "Copied to clipboard:\n  {}{}{}",
                                        preview, ellipsis,
                                        if entry.content.len() > 50 {
                                            format!("\n  ({} characters total)", entry.content.len())
                                        } else {
                                            String::new()
                                        }
                                    ));
                                }
                                Err(e) => {
                                    self.add_system_message(format!(
                                        "Failed to copy to clipboard: {}", e
                                    ));
                                }
                            }
                        }
                        Err(e) => {
                            self.add_system_message(format!(
                                "Failed to access clipboard: {}", e
                            ));
                        }
                    }
                } else {
                    self.add_system_message("No assistant message to copy.".to_string());
                }
            }
            "/bug" => {
                let session_short = &self.session.session_id[..self.session.session_id.len().min(8)];
                let os_info = format!("{} {}", std::env::consts::OS, std::env::consts::ARCH);
                let recent_msgs: Vec<String> = self.chat_entries.iter().rev().take(5).map(|e| {
                    let role = match &e.role {
                        ChatRole::User => "User",
                        ChatRole::Assistant => "Assistant",
                        ChatRole::System => "System",
                        ChatRole::ToolCall { name } => return format!("  ToolCall({}): ...", name),
                        ChatRole::ToolResult { name, .. } => return format!("  ToolResult({}): ...", name),
                        _ => "Other",
                    };
                    let preview: String = e.content.chars().take(80).collect();
                    format!("  {}: {}", role, preview)
                }).collect();
                self.add_system_message(format!(
                    "Bug report:\n  Version:    yode {}\n  OS:         {}\n  Terminal:   {}\n  Session:    {}\n  Model:      {}\n  Tokens:     {}\n\nRecent messages (last 5):\n{}",
                    env!("CARGO_PKG_VERSION"),
                    os_info,
                    self.terminal_caps.summary(),
                    session_short,
                    self.session.model,
                    self.session.total_tokens,
                    recent_msgs.into_iter().rev().collect::<Vec<_>>().join("\n")
                ));
            }
            "/doctor" => {
                let mut checks = Vec::new();

                // Check API key
                let has_api_key = std::env::var("ANTHROPIC_API_KEY").is_ok()
                    || std::env::var("OPENAI_API_KEY").is_ok();
                checks.push(if has_api_key {
                    "  [ok] API key configured"
                } else {
                    "  [!!] No API key found (ANTHROPIC_API_KEY or OPENAI_API_KEY)"
                });

                // Check git
                let git_ok = std::process::Command::new("git")
                    .arg("--version")
                    .output()
                    .map(|o| o.status.success())
                    .unwrap_or(false);
                checks.push(if git_ok {
                    "  [ok] git available"
                } else {
                    "  [!!] git not found"
                });

                // Check terminal capabilities
                checks.push(if self.terminal_caps.truecolor {
                    "  [ok] Truecolor support"
                } else {
                    "  [--] No truecolor (using 256 colors)"
                });
                if self.terminal_caps.in_tmux {
                    checks.push("  [--] Running inside tmux");
                }
                if self.terminal_caps.in_ssh {
                    checks.push("  [--] Running over SSH");
                }

                // Check tools
                let tool_count = tools.definitions().len();
                checks.push(if tool_count > 0 {
                    "  [ok] Tools registered"
                } else {
                    "  [!!] No tools registered"
                });

                self.add_system_message(format!(
                    "Environment check:\n{}\n\n  Terminal: {}\n  Tools:    {} registered",
                    checks.join("\n"),
                    self.terminal_caps.summary(),
                    tool_count
                ));
            }
            "/config" => {
                let permission_mode = self.session.permission_mode.label();
                let always_allow = if self.session.always_allow_tools.is_empty() {
                    "none".to_string()
                } else {
                    self.session.always_allow_tools.join(", ")
                };
                self.add_system_message(format!(
                    "Configuration:\n  Model:           {}\n  Permission mode: {}\n  Working dir:     {}\n  Always-allow:    {}\n  Terminal:        {}\n  Truecolor:       {}\n  Tmux:            {}\n  SSH:             {}",
                    self.session.model,
                    permission_mode,
                    self.session.working_dir,
                    always_allow,
                    self.terminal_caps.term_program.as_deref().unwrap_or("unknown"),
                    self.terminal_caps.truecolor,
                    self.terminal_caps.in_tmux,
                    self.terminal_caps.in_ssh,
                ));
            }
            "/version" => {
                self.add_system_message(format!(
                    "yode {}\n  Built with:  rustc ({})\n  OS:          {} {}\n  Profile:     {}",
                    env!("CARGO_PKG_VERSION"),
                    option_env!("CARGO_PKG_RUST_VERSION").unwrap_or("unknown"),
                    std::env::consts::OS,
                    std::env::consts::ARCH,
                    if cfg!(debug_assertions) { "debug" } else { "release" },
                ));
            }
            "/history" => {
                let entries = self.history.entries();
                let count = arg.parse::<usize>().unwrap_or(10).min(50);
                let start = entries.len().saturating_sub(count);
                if entries.is_empty() {
                    self.add_system_message("No input history yet.".to_string());
                } else {
                    let mut lines = String::from("Recent input history:\n");
                    for (i, entry) in entries[start..].iter().enumerate() {
                        let preview: String = entry.chars().take(80).collect();
                        let ellipsis = if entry.len() > 80 { "..." } else { "" };
                        lines.push_str(&format!("  {:>3}. {}{}\n", start + i + 1, preview, ellipsis));
                    }
                    self.add_system_message(lines);
                }
            }
            "/time" => {
                let elapsed = self.session_start.elapsed();
                let hours = elapsed.as_secs() / 3600;
                let mins = (elapsed.as_secs() % 3600) / 60;
                let secs = elapsed.as_secs() % 60;
                let turn_info = if let Some(turn_start) = self.turn_started_at {
                    let turn_elapsed = turn_start.elapsed();
                    format!("\n  Current turn:    {}s", turn_elapsed.as_secs())
                } else {
                    String::new()
                };
                self.add_system_message(format!(
                    "Session timing:\n  Session duration: {}h {:02}m {:02}s\n  Messages:        {}\n  Tool calls:      {}{}",
                    hours, mins, secs,
                    self.chat_entries.len(),
                    self.session.tool_call_count,
                    turn_info,
                ));
            }
            _ => {
                // Find closest command by edit distance
                let suggestion = find_closest_command(cmd);
                let msg = if let Some(closest) = suggestion {
                    format!(
                        "Unknown command: {}. Did you mean {}?",
                        cmd, closest
                    )
                } else {
                    format!(
                        "Unknown command: {}. Type /help for available commands.", cmd
                    )
                };
                self.add_system_message(msg);
            }
        }
        true
    }

    /// Handle ! shell command prefix. Returns true if input was a shell command.
    pub(crate) fn handle_shell_command(&mut self, input: &str) -> bool {
        let trimmed = input.trim();
        if !trimmed.starts_with('!') || trimmed.len() <= 1 {
            return false;
        }

        let cmd = &trimmed[1..];

        // Safety check for dangerous commands
        if let Some(pattern) = is_dangerous_command(cmd) {
            self.chat_entries.push(ChatEntry::new(
                ChatRole::User,
                format!("!{}", cmd),
            ));
            self.add_system_message(format!(
                "⚠ Dangerous command detected: '{}'\nCommand blocked for safety. Use the LLM to execute if intended.",
                pattern
            ));
            return true;
        }

        self.chat_entries.push(ChatEntry::new(
            ChatRole::User,
            format!("!{}", cmd),
        ));

        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output();

        let content = match output {
            Ok(o) => {
                let stdout = String::from_utf8_lossy(&o.stdout);
                let stderr = String::from_utf8_lossy(&o.stderr);
                let mut result = String::new();
                if !stdout.is_empty() {
                    result.push_str(&stdout);
                }
                if !stderr.is_empty() {
                    if !result.is_empty() {
                        result.push('\n');
                    }
                    result.push_str(&stderr);
                }
                if result.is_empty() {
                    format!("(exit code: {})", o.status.code().unwrap_or(-1))
                } else {
                    // Truncate very long outputs
                    if result.len() > 10000 {
                        let truncated: String = result.chars().take(10000).collect();
                        format!("{}...\n\n(output truncated, {} total bytes)", truncated, result.len())
                    } else {
                        result
                    }
                }
            }
            Err(e) => format!("Failed to execute: {}", e),
        };

        self.add_system_message(content);
        true
    }

    /// Process @file references in input — attaches file contents.
    pub(crate) fn process_file_references(&self, input: &str) -> String {
        let mut result = input.to_string();
        let mut context_parts: Vec<String> = Vec::new();

        for word in input.split_whitespace() {
            if word.starts_with('@') && word.len() > 1 {
                let file_path = &word[1..];
                match std::fs::read_to_string(file_path) {
                    Ok(content) => {
                        let line_count = content.lines().count();
                        context_parts.push(format!(
                            "\n<file path=\"{}\" lines=\"{}\">\n{}\n</file>",
                            file_path, line_count, content
                        ));
                    }
                    Err(e) => {
                        context_parts.push(format!(
                            "\n<file_error path=\"{}\">{}</file_error>",
                            file_path, e
                        ));
                    }
                }
            }
        }

        if !context_parts.is_empty() {
            result.push_str("\n\n[Attached file context]");
            for part in context_parts {
                result.push_str(&part);
            }
        }
        result
    }

    /// Helper to add a system message.
    fn add_system_message(&mut self, content: String) {
        self.chat_entries.push(ChatEntry::new(ChatRole::System, content));
    }
}

/// Estimate cost based on model with separate input/output pricing (per Mtok).
pub(crate) fn estimate_cost(model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
    let (input_per_mtok, output_per_mtok) = if model.contains("claude-3-opus") || model.contains("claude-opus") {
        (15.0, 75.0)
    } else if model.contains("claude-3-sonnet") || model.contains("claude-3.5") || model.contains("claude-sonnet") {
        (3.0, 15.0)
    } else if model.contains("claude-3-haiku") || model.contains("claude-haiku") {
        (0.25, 1.25)
    } else if model.contains("gpt-4o") {
        (2.5, 10.0)
    } else if model.contains("gpt-4") {
        (30.0, 60.0)
    } else if model.contains("gpt-3.5") {
        (0.5, 1.5)
    } else if model.contains("deepseek") {
        (0.14, 0.28)
    } else {
        (5.0, 15.0)
    };
    (input_tokens as f64 / 1_000_000.0) * input_per_mtok
        + (output_tokens as f64 / 1_000_000.0) * output_per_mtok
}

/// Find the closest slash command by edit distance. Returns None if all are too distant.
fn find_closest_command(input: &str) -> Option<&'static str> {
    let mut best: Option<(&str, usize)> = None;
    for cmd in SLASH_COMMANDS {
        let d = levenshtein(input, cmd.name);
        if best.map_or(true, |(_, bd)| d < bd) {
            best = Some((cmd.name, d));
        }
    }
    // Only suggest if distance is reasonable (at most half the command length)
    best.and_then(|(name, d)| {
        if d <= name.len() / 2 + 1 {
            Some(name)
        } else {
            None
        }
    })
}

/// Simple Levenshtein distance implementation.
fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (m, n) = (a.len(), b.len());
    let mut prev = (0..=n).collect::<Vec<_>>();
    let mut curr = vec![0; n + 1];
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1)
                .min(curr[j - 1] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}
