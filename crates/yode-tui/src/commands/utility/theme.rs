use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use yode_core::config::Config;

pub struct ThemeCommand {
    meta: CommandMeta,
}

impl ThemeCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "theme",
                description: "Show or set the UI theme (dark/light)",
                aliases: &[],
                args: vec![ArgDef {
                    name: "theme".to_string(),
                    required: false,
                    hint: "[dark|light]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "dark".to_string(),
                        "light".to_string(),
                    ]),
                }],
                category: CommandCategory::Utility,
                hidden: false,
            },
        }
    }
}

impl Command for ThemeCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, _ctx: &mut CommandContext) -> CommandResult {
        let args = args.trim();

        if args.is_empty() {
            let theme = read_current_theme().unwrap_or_else(|_| "dark".to_string());
            Ok(CommandOutput::Message(format!("Current theme: {}", theme)))
        } else {
            let new_theme = args.to_lowercase();
            if !is_supported_theme(&new_theme) {
                return Ok(CommandOutput::Message(
                    "Invalid theme. Valid values: dark, light".to_string(),
                ));
            }

            if let Err(e) = persist_theme_to_config(&new_theme) {
                return Ok(CommandOutput::Message(format!(
                    "Failed to save theme: {}",
                    e
                )));
            }

            Ok(CommandOutput::Message(format!(
                "Theme set to: {}. Restart required for changes to take effect.",
                new_theme
            )))
        }
    }
}

fn read_current_theme() -> Result<String, String> {
    Config::load()
        .map(|config| config.ui.theme)
        .map_err(|e| format!("Failed to load config: {}", e))
}

fn persist_theme_to_config(theme: &str) -> Result<(), String> {
    let mut config = Config::load().map_err(|e| format!("Failed to load config: {}", e))?;
    config.ui.theme = theme.to_string();
    config
        .save()
        .map_err(|e| format!("Failed to save config: {}", e))
}

fn is_supported_theme(theme: &str) -> bool {
    matches!(theme, "dark" | "light")
}
