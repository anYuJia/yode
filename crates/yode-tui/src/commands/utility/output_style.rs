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
            let configured = read_current_output_style()?;
            let active = read_active_output_style(ctx);
            return Ok(CommandOutput::Message(render_output_style_status(
                &configured,
                active.as_deref(),
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

fn read_active_output_style(ctx: &mut CommandContext<'_>) -> Option<String> {
    ctx.engine
        .try_lock()
        .ok()
        .map(|engine| engine.context().output_style.clone())
}

fn render_output_style_status(configured: &str, active: Option<&str>) -> String {
    match active {
        Some(active) if active == configured => {
            format!("Current output style: {}.", configured)
        }
        Some(active) => format!(
            "Current output style: {}. Active session: {}.",
            configured, active
        ),
        None => format!(
            "Current output style: {}. Active session unavailable because the engine is busy.",
            configured
        ),
    }
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
    use super::{is_supported_output_style, render_output_style_status};

    #[test]
    fn validates_supported_output_styles() {
        assert!(is_supported_output_style("default"));
        assert!(is_supported_output_style("explanatory"));
        assert!(is_supported_output_style("learning"));
        assert!(!is_supported_output_style("verbose"));
    }

    #[test]
    fn output_style_status_surfaces_active_session_drift() {
        assert_eq!(
            render_output_style_status("learning", Some("learning")),
            "Current output style: learning."
        );
        assert_eq!(
            render_output_style_status("learning", Some("default")),
            "Current output style: learning. Active session: default."
        );
        assert!(render_output_style_status("learning", None).contains("engine is busy"));
    }
}
