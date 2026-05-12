use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use yode_core::config::Config;

pub struct OutputStyleCommand {
    meta: CommandMeta,
}

impl OutputStyleCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "output-style",
                description: "Show or set the assistant output style",
                aliases: &["style"],
                args: vec![ArgDef {
                    name: "style".to_string(),
                    required: false,
                    hint: "[default|explanatory|learning]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "default".to_string(),
                        "explanatory".to_string(),
                        "learning".to_string(),
                    ]),
                }],
                category: CommandCategory::Utility,
                hidden: false,
            },
        }
    }
}

impl Command for OutputStyleCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let style = args.trim().to_ascii_lowercase();
        if style.is_empty() {
            let current = read_current_output_style()?;
            return Ok(CommandOutput::Message(format!(
                "Current output style: {}",
                current
            )));
        }
        if !is_supported_output_style(&style) {
            return Ok(CommandOutput::Message(
                "Invalid output style. Valid values: default, explanatory, learning".to_string(),
            ));
        }

        persist_output_style_to_config(&style)?;
        let refresh_note = apply_output_style_to_active_engine(&style, ctx);
        Ok(CommandOutput::Message(format!(
            "Output style set to: {}. {}",
            style, refresh_note
        )))
    }
}

fn read_current_output_style() -> Result<String, String> {
    Config::load()
        .map(|config| config.ui.output_style)
        .map_err(|err| format!("Failed to load config: {}", err))
}

fn persist_output_style_to_config(style: &str) -> Result<(), String> {
    let mut config = Config::load().map_err(|err| format!("Failed to load config: {}", err))?;
    config.ui.output_style = style.to_string();
    config
        .save()
        .map_err(|err| format!("Failed to save config: {}", err))
}

fn apply_output_style_to_active_engine(style: &str, ctx: &mut CommandContext<'_>) -> String {
    match ctx.engine.try_lock() {
        Ok(mut engine) => {
            engine.set_output_style(style.to_string());
            "Active system prompt refreshed.".to_string()
        }
        Err(_) => "Active engine is busy, the new style will apply on the next turn.".to_string(),
    }
}

fn is_supported_output_style(style: &str) -> bool {
    matches!(style, "default" | "explanatory" | "learning")
}

#[cfg(test)]
mod tests {
    use super::is_supported_output_style;

    #[test]
    fn validates_supported_output_styles() {
        assert!(is_supported_output_style("default"));
        assert!(is_supported_output_style("explanatory"));
        assert!(is_supported_output_style("learning"));
        assert!(!is_supported_output_style("verbose"));
    }
}
