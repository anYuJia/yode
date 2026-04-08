use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use std::path::PathBuf;
use yode_core::config::Config;
use yode_core::updater::{Updater, CURRENT_VERSION};

pub struct UpdateCommand {
    meta: CommandMeta,
}

impl UpdateCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "update",
                description: "Check for and install updates",
                aliases: &[],
                args: vec![
                    crate::commands::ArgDef {
                        name: "subcommand".into(),
                        required: false,
                        hint: "<check|status|set>".into(),
                        completions: crate::commands::ArgCompletionSource::Static(vec![
                            "check".into(),
                            "status".into(),
                            "set".into(),
                        ]),
                    },
                    crate::commands::ArgDef {
                        name: "value".into(),
                        required: false,
                        hint: "<true|false>".into(),
                        completions: crate::commands::ArgCompletionSource::Static(vec![
                            "true".into(),
                            "false".into(),
                        ]),
                    },
                ],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for UpdateCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, _ctx: &mut CommandContext) -> CommandResult {
        let parts: Vec<&str> = args.trim().split_whitespace().collect();

        match parts.as_slice() {
            // /update — check for updates
            [] => self.check_for_updates(),

            // /update check — same as /update
            ["check"] => self.check_for_updates(),

            // /update status — show update configuration status
            ["status"] => self.show_status(),

            // /update set <option> <value> — set update config
            ["set", "auto_check", value] => self.set_config("auto_check", value),
            ["set", "auto_download", value] => self.set_config("auto_download", value),

            // /update set — show help
            ["set"] => Ok(CommandOutput::Messages(vec![
                "Update configuration options:".into(),
                "  /update set auto_check <true|false>    — Enable/disable automatic update checks"
                    .into(),
                "  /update set auto_download <true|false> — Enable/disable automatic downloads"
                    .into(),
                String::new(),
                "Current settings:".into(),
            ])),

            _ => Err("Unknown subcommand. Use /update for help.".into()),
        }
    }
}

impl UpdateCommand {
    fn check_for_updates(&self) -> CommandResult {
        // Load config directly
        let config = Config::load().map_err(|e| e.to_string())?;

        if !config.update.auto_check {
            return Ok(CommandOutput::Message(
                "Automatic update check is disabled. Enable with: /update set auto_check true"
                    .into(),
            ));
        }

        // Get config directory
        let config_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".yode");

        let updater = Updater::new(
            config_dir,
            config.update.auto_check,
            config.update.auto_download,
        );

        // Run update check synchronously (blocking for simplicity in command)
        let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
        let result = rt.block_on(updater.check_for_updates());

        match result {
            Ok(Some(update)) => {
                let mut messages = vec![
                    format!("✨ New version available: {}", update.latest_version),
                    format!("   Current version: {}", CURRENT_VERSION),
                    format!("   Published: {}", update.published_at),
                    String::new(),
                    "Release Notes:".into(),
                ];

                // Add first few lines of release notes
                for line in update.release_notes.lines().take(5) {
                    messages.push(format!("  {}", line));
                }
                messages.push(String::new());

                if config.update.auto_download {
                    messages.push("Downloading update...".into());

                    // Download the update
                    match rt.block_on(updater.download_update(&update)) {
                        Ok(path) => {
                            messages.push(format!("✓ Update downloaded to: {:?}", path));
                            messages.push(String::new());
                            messages.push("Restart yode to use the new version.".into());
                        }
                        Err(e) => {
                            messages.push(format!("✗ Download failed: {}", e));
                            messages.push(String::new());
                            messages.push(format!(
                                "Manual update: Download from {}",
                                update.download_url
                            ));
                        }
                    }
                } else {
                    messages.push(format!("Download URL: {}", update.download_url));
                    messages.push(String::new());
                    messages.push("Auto-download is disabled. Enable with:".into());
                    messages.push("  /update set auto_download true".into());
                    messages.push(String::new());
                    messages.push("Or update manually:".into());
                    if cfg!(target_os = "macos") {
                        messages.push("  brew upgrade yode  # if installed via Homebrew".into());
                    }
                    messages.push(format!("  curl -LO {}", update.download_url));
                }

                Ok(CommandOutput::Messages(messages))
            }
            Ok(None) => Ok(CommandOutput::Message(format!(
                "✓ You are on the latest version: {}",
                CURRENT_VERSION
            ))),
            Err(e) => Ok(CommandOutput::Message(format!(
                "✗ Failed to check for updates: {}",
                e
            ))),
        }
    }

    fn show_status(&self) -> CommandResult {
        let config = Config::load().map_err(|e| e.to_string())?;

        Ok(CommandOutput::Messages(vec![
            "Update Configuration:".into(),
            format!("  auto_check:      {}", config.update.auto_check),
            format!("  auto_download:   {}", config.update.auto_download),
            String::new(),
            "Commands:".into(),
            "  /update check                       — Manually check for updates".into(),
            "  /update set auto_check <true|false> — Enable/disable update checks".into(),
            "  /update set auto_download <true|false> — Enable/disable auto downloads".into(),
        ]))
    }

    fn set_config(&self, option: &str, value: &str) -> CommandResult {
        let mut config = Config::load().map_err(|e| e.to_string())?;

        let bool_value = match value {
            "true" => true,
            "false" => false,
            _ => return Err(format!("Invalid value '{}'. Use 'true' or 'false'.", value)),
        };

        match option {
            "auto_check" => {
                config.update.auto_check = bool_value;
            }
            "auto_download" => {
                config.update.auto_download = bool_value;
            }
            _ => return Err(format!("Unknown option '{}'.", option)),
        }

        config.save().map_err(|e| e.to_string())?;

        Ok(CommandOutput::Message(format!(
            "✓ Set update.{} = {}",
            option, bool_value
        )))
    }
}
