mod config_ops;
mod wizard_builders;

use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

use self::config_ops::{
    add_provider_to_config, edit_provider_field, persist_default_provider,
    remove_provider_from_config,
};
use self::wizard_builders::{build_add_provider_wizard, build_edit_provider_wizard};

pub struct ProviderCommand {
    meta: CommandMeta,
}

impl ProviderCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "provider",
                description: "Manage LLM providers (list/switch/add/remove/edit)",
                aliases: &[],
                args: vec![
                    ArgDef {
                        name: "subcommand".into(),
                        required: false,
                        hint: "<list|switch|add|remove|edit>".into(),
                        completions: ArgCompletionSource::Static(vec![
                            "list".into(),
                            "switch".into(),
                            "add".into(),
                            "remove".into(),
                            "edit".into(),
                        ]),
                    },
                    ArgDef {
                        name: "name".into(),
                        required: false,
                        hint: "<provider-name>".into(),
                        completions: ArgCompletionSource::Dynamic(|ctx| {
                            let mut names: Vec<String> =
                                ctx.all_provider_models.keys().cloned().collect();
                            names.sort();
                            names
                        }),
                    },
                ],
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
        let parts: Vec<&str> = args.trim().split_whitespace().collect();

        match parts.as_slice() {
            // /provider — show current
            [] => {
                let models = if ctx.provider_models.is_empty() {
                    "(unrestricted)".to_string()
                } else {
                    ctx.provider_models.join(", ")
                };
                Ok(CommandOutput::Messages(vec![
                    format!("Current provider: {}", ctx.provider_name),
                    format!("Current model:    {}", ctx.session.model),
                    format!("Available models: {}", models),
                    String::new(),
                    "Subcommands:".into(),
                    "  /provider list                                — List all providers".into(),
                    "  /provider switch <name>                       — Switch provider (persisted)"
                        .into(),
                    "  /provider add <name> <format> <url> [models]  — Add provider".into(),
                    "  /provider remove <name>                       — Remove provider".into(),
                    "  /provider edit <name>                         — Show config for editing"
                        .into(),
                    "  /provider edit <name> <field> <value>         — Edit a field".into(),
                ]))
            }

            // /provider list
            ["list"] => {
                let mut lines = vec!["Available providers:".to_string()];
                for (name, models) in ctx.all_provider_models.iter() {
                    let marker = if *name == *ctx.provider_name {
                        "*"
                    } else {
                        " "
                    };
                    let model_str = if models.is_empty() {
                        "(unrestricted)".to_string()
                    } else {
                        models.join(", ")
                    };
                    lines.push(format!(" {} {:<15} — {}", marker, name, model_str));
                }
                Ok(CommandOutput::Messages(lines))
            }

            // /provider switch <name>
            ["switch", name] => {
                if let Some(provider) = ctx.provider_registry.get(name) {
                    let new_models = ctx
                        .all_provider_models
                        .get(*name)
                        .cloned()
                        .unwrap_or_default();
                    let new_model = new_models.first().cloned().unwrap_or_default(); // empty if unrestricted
                    if let Ok(mut eng) = ctx.engine.try_lock() {
                        eng.set_provider(provider, name.to_string());
                        if !new_model.is_empty() {
                            eng.set_model(new_model.clone());
                        }
                    }
                    *ctx.provider_name = name.to_string();
                    *ctx.provider_models = new_models;
                    if !new_model.is_empty() {
                        ctx.session.model = new_model.clone();
                    }

                    // Persist to config file
                    let persist_result = persist_default_provider(name, new_model.as_str());

                    let mut messages = vec![format!(
                        "Switched to provider: {}, model: {}",
                        name,
                        if new_model.is_empty() {
                            &ctx.session.model
                        } else {
                            &new_model
                        }
                    )];
                    if let Ok(msg) = persist_result {
                        messages.push(msg);
                    }

                    Ok(CommandOutput::Messages(messages))
                } else {
                    let available: Vec<String> = ctx.all_provider_models.keys().cloned().collect();
                    Err(format!(
                        "Provider '{}' not found. Available: {}",
                        name,
                        available.join(", ")
                    ))
                }
            }

            // /provider add — start interactive wizard (matches setup.rs flow)
            ["add"] | ["add", ..] => {
                // If full args provided, do it directly
                if parts.len() >= 4 {
                    let name = parts[1];
                    let format = parts[2];
                    let base_url = parts[3];
                    if ctx.all_provider_models.contains_key(name) {
                        return Err(format!("Provider '{}' already exists.", name));
                    }
                    if format != "openai" && format != "anthropic" && format != "gemini" {
                        return Err("Format must be 'openai', 'anthropic', or 'gemini'.".into());
                    }
                    let models_owned: Vec<String> = if parts.len() > 4 {
                        parts[4..]
                            .join(" ")
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect()
                    } else {
                        vec![]
                    };
                    match add_provider_to_config(name, format, Some(base_url), &models_owned, None)
                    {
                        Ok(_) => {
                            let model_info = if models_owned.is_empty() {
                                "(unrestricted)".into()
                            } else {
                                models_owned.join(", ")
                            };
                            return Ok(CommandOutput::Messages(vec![
                                format!("✓ Provider '{}' added!", name),
                                format!("  format:   {}", format),
                                format!("  base_url: {}", base_url),
                                format!("  models:   {}", model_info),
                                String::new(),
                                format!(
                                    "Set API key: export {}_API_KEY=<your-key>",
                                    name.to_uppercase().replace("-", "_")
                                ),
                                "Restart yode to activate.".into(),
                            ]));
                        }
                        Err(e) => return Err(format!("Failed to add: {}", e)),
                    }
                }

                // Interactive wizard — matches setup.rs flow:
                // 1. Select provider type (preset or custom)
                // 2. Base URL (with smart default)
                // 3. API Key (required)
                // 4. Provider name (with suggestion)
                // 5. Default model (with recommendation)
                Ok(CommandOutput::StartWizard(build_add_provider_wizard()))
            }

            // /provider remove <name>
            ["remove", name] => {
                if !ctx.all_provider_models.contains_key(*name) {
                    return Err(format!("Provider '{}' not found.", name));
                }
                if *name == ctx.provider_name.as_str() {
                    return Err(format!(
                        "Cannot remove active provider '{}'. Switch first.",
                        name
                    ));
                }
                match remove_provider_from_config(name) {
                    Ok(_) => Ok(CommandOutput::Messages(vec![
                        format!("Provider '{}' removed from config.", name),
                        "Restart yode to take effect.".into(),
                    ])),
                    Err(e) => Err(format!("Failed to remove: {}", e)),
                }
            }

            // /provider edit <name> — interactive edit wizard with current values as defaults
            ["edit", name] => Ok(CommandOutput::StartWizard(build_edit_provider_wizard(
                name,
            )?)),

            // /provider edit <name> format <value>
            ["edit", name, "format", value] => {
                if *value != "openai" && *value != "anthropic" {
                    return Err("Format must be 'openai' or 'anthropic'.".into());
                }
                let msgs = edit_provider_field(name, "format", value)?;
                Ok(CommandOutput::ReloadProvider {
                    name: name.to_string(),
                    messages: msgs,
                })
            }

            // /provider edit <name> base_url <value>
            ["edit", name, "base_url", value] => {
                let msgs = edit_provider_field(name, "base_url", value)?;
                Ok(CommandOutput::ReloadProvider {
                    name: name.to_string(),
                    messages: msgs,
                })
            }

            // /provider edit <name> api_key <value>
            ["edit", name, "api_key", value] => {
                let msgs = edit_provider_field(name, "api_key", value)?;
                Ok(CommandOutput::ReloadProvider {
                    name: name.to_string(),
                    messages: msgs,
                })
            }

            // /provider edit <name> models <model1,model2,...>
            ["edit", name, "models", ..] => {
                let models_str = parts[3..].join(" ");
                let msgs = edit_provider_field(name, "models", &models_str)?;
                Ok(CommandOutput::ReloadProvider {
                    name: name.to_string(),
                    messages: msgs,
                })
            }

            _ => Err("Unknown subcommand. Use /provider for help.".into()),
        }
    }
}
