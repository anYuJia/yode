use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};
use crate::commands::context::CommandContext;

pub struct ProviderCommand {
    meta: CommandMeta,
}

impl ProviderCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "/provider",
                description: "Show or switch the current provider",
                aliases: &[],
                args: vec![ArgDef {
                    name: "provider".into(),
                    required: false,
                    hint: "provider name".into(),
                    completions: ArgCompletionSource::Dynamic(|ctx| {
                        let mut names: Vec<String> =
                            ctx.all_provider_models.keys().cloned().collect();
                        names.sort();
                        names
                    }),
                }],
                category: CommandCategory::Model,
                hidden: false,
            },
        }
    }
}

impl Command for ProviderCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        if args.is_empty() {
            Ok(CommandOutput::Message(format!(
                "Current provider: {}\nUse /provider <name> to switch, /providers to list all.",
                ctx.provider_name
            )))
        } else {
            let new_provider = args.to_string();
            if let Some(provider) = ctx.provider_registry.get(&new_provider) {
                let new_models = ctx
                    .all_provider_models
                    .get(&new_provider)
                    .cloned()
                    .unwrap_or_default();
                let new_model = new_models
                    .first()
                    .cloned()
                    .unwrap_or_else(|| ctx.session.model.clone());
                if let Ok(mut eng) = ctx.engine.try_lock() {
                    eng.set_provider(provider, new_provider.clone());
                    eng.set_model(new_model.clone());
                }
                *ctx.provider_name = new_provider.clone();
                *ctx.provider_models = new_models;
                Ok(CommandOutput::Message(format!(
                    "Switched to provider: {}, model: {}",
                    new_provider, new_model
                )))
            } else {
                let available: Vec<String> =
                    ctx.all_provider_models.keys().cloned().collect();
                Ok(CommandOutput::Message(format!(
                    "Provider '{}' not found. Available: {}",
                    new_provider,
                    available.join(", ")
                )))
            }
        }
    }
}
