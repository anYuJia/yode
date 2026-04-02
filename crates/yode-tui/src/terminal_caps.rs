/// Terminal capability detection.
///
/// Detects terminal type, color support, and environment (tmux, SSH)
/// to adjust rendering behavior accordingly.

/// Detected terminal capabilities.
#[derive(Debug, Clone)]
pub struct TerminalCaps {
    /// Terminal program name (e.g. "iTerm.app", "Apple_Terminal", "vscode")
    pub term_program: Option<String>,
    /// TERM environment variable
    pub term: Option<String>,
    /// Whether truecolor (24-bit) is supported
    pub truecolor: bool,
    /// Whether running inside tmux
    pub in_tmux: bool,
    /// Whether running over SSH
    pub in_ssh: bool,
    /// Whether running inside a VS Code terminal
    pub in_vscode: bool,
}

impl TerminalCaps {
    /// Detect terminal capabilities from environment variables.
    pub fn detect() -> Self {
        let term_program = std::env::var("TERM_PROGRAM").ok();
        let term = std::env::var("TERM").ok();
        let colorterm = std::env::var("COLORTERM").ok();

        let truecolor = colorterm
            .as_deref()
            .map(|v| v == "truecolor" || v == "24bit")
            .unwrap_or(false)
            || term_program.as_deref() == Some("iTerm.app")
            || term_program.as_deref() == Some("WezTerm")
            || std::env::var("WT_SESSION").is_ok(); // Windows Terminal

        let in_tmux = std::env::var("TMUX").is_ok();
        let in_ssh = std::env::var("SSH_TTY").is_ok() || std::env::var("SSH_CONNECTION").is_ok();
        let in_vscode = term_program.as_deref() == Some("vscode");

        Self {
            term_program,
            term,
            truecolor,
            in_tmux,
            in_ssh,
            in_vscode,
        }
    }

    /// Whether to use simplified colors (256-color fallback).
    pub fn use_simple_colors(&self) -> bool {
        !self.truecolor || self.in_ssh
    }

    /// Get a summary string for diagnostics (e.g. /doctor command).
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(ref prog) = self.term_program {
            parts.push(format!("term={}", prog));
        }
        if self.truecolor {
            parts.push("truecolor".to_string());
        } else {
            parts.push("256color".to_string());
        }
        if self.in_tmux {
            parts.push("tmux".to_string());
        }
        if self.in_ssh {
            parts.push("ssh".to_string());
        }
        if self.in_vscode {
            parts.push("vscode".to_string());
        }
        parts.join(", ")
    }
}
