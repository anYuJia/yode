use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct DoctorCommand {
    meta: CommandMeta,
}

impl DoctorCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "doctor",
                description: "Run environment health check",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for DoctorCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let mut checks = Vec::new();

        // 1. Check providers
        if ctx.all_provider_models.is_empty() {
            checks.push("  [!!] No LLM providers configured. Run /provider add to set one up.".to_string());
        } else {
            let names: Vec<_> = ctx.all_provider_models.keys().cloned().collect();
            checks.push(format!("  [ok] LLM providers configured: {}", names.join(", ")));
        }

        // 2. Check git
        let git_v = std::process::Command::new("git").arg("--version").output();
        match git_v {
            Ok(o) if o.status.success() => {
                let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                checks.push(format!("  [ok] git available: {}", v));
            }
            _ => checks.push("  [!!] git not found or failed".to_string()),
        }

        // 3. Check optional runtimes (Node, Python, Go, Rust)
        let runtimes = [
            ("node", "--version"),
            ("python3", "--version"),
            ("go", "version"),
            ("cargo", "--version"),
        ];

        for (cmd, arg) in runtimes {
            let output = std::process::Command::new(cmd).arg(arg).output();
            match output {
                Ok(o) if o.status.success() => {
                    let v = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    checks.push(format!("  [ok] {} available: {}", cmd, v));
                }
                _ => checks.push(format!("  [--] {} not found (optional)", cmd)),
            }
        }

        // 4. Check terminal capabilities
        checks.push(if ctx.terminal_caps.truecolor {
            "  [ok] Truecolor support enabled".to_string()
        } else {
            "  [--] No truecolor (using 256 colors)".to_string()
        });
        
        if ctx.terminal_caps.in_tmux { checks.push("  [--] Running inside tmux".to_string()); }
        if ctx.terminal_caps.in_ssh { checks.push("  [--] Running over SSH".to_string()); }

        // 5. Check Yode internals
        let tool_count = ctx.tools.definitions().len();
        checks.push(format!("  [ok] {} tools registered", tool_count));
        
        let config_path = dirs::home_dir().map(|h| h.join(".yode/config.toml"));
        if let Some(p) = config_path {
            if p.exists() {
                checks.push(format!("  [ok] Config file: {:?}", p));
            } else {
                checks.push("  [!!] Config file missing".to_string());
            }
        }

        Ok(CommandOutput::Message(format!(
            "Yode Environment Health Check:\n\n{}\n\n  Platform: {} {}\n  Version:  v{}\n  Session:  {}",
            checks.join("\n"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            env!("CARGO_PKG_VERSION"),
            &ctx.session.session_id[..8],
        )))
    }
}
