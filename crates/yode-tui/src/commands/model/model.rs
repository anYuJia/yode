use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use crate::commands::context::CommandContext;

pub struct ModelCommand {
    meta: CommandMeta,
}

impl ModelCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "model",
                description: "Show or switch the current model",
                aliases: &["m"],
                args: vec![ArgDef {
                    name: "model".into(),
                    required: false,
                    hint: "model name".into(),
                    completions: ArgCompletionSource::Dynamic(|ctx| {
                        ctx.provider_models.to_vec()
                    }),
                }],
                category: CommandCategory::Model,
                hidden: false,
            },
        }
    }
}

impl Command for ModelCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        if args.is_empty() {
            // Show current model + available models
            let models_list = if ctx.provider_models.is_empty() {
                "  (unrestricted)".to_string()
            } else {
                ctx.provider_models
                    .iter()
                    .map(|m| {
                        if *m == ctx.session.model {
                            format!("  * {} (current)", m)
                        } else {
                            format!("    {}", m)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            Ok(CommandOutput::Message(format!(
                "Current model: {}\nProvider: {}\nAvailable models:\n{}",
                ctx.session.model, ctx.provider_name, models_list
            )))
        } else {
            // Switch model
            let new_model = args.to_string();
            if !ctx.provider_models.is_empty() && !ctx.provider_models.contains(&new_model) {
                Ok(CommandOutput::Message(format!(
                    "Model '{}' is not available for provider '{}'. Available models:\n  {}",
                    new_model,
                    ctx.provider_name,
                    ctx.provider_models.join("\n  ")
                )))
            } else {
                if let Ok(mut eng) = ctx.engine.try_lock() {
                    eng.set_model(new_model.clone());
                }
                Ok(CommandOutput::Message(format!(
                    "Switched to model: {}",
                    new_model
                )))
            }
        }
    }
}
