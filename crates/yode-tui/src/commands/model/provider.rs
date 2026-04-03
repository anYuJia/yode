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
                    "  /provider switch <name>                       — Switch provider".into(),
                    "  /provider add <name> <format> <url> [models]  — Add provider".into(),
                    "  /provider remove <name>                       — Remove provider".into(),
                    "  /provider edit <name>                         — Show config for editing".into(),
                    "  /provider edit <name> <field> <value>         — Edit a field".into(),
                ]))
            }

            // /provider list
            ["list"] => {
                let mut lines = vec!["Available providers:".to_string()];
                for (name, models) in ctx.all_provider_models.iter() {
                    let marker = if *name == *ctx.provider_name { "*" } else { " " };
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
                    let new_models = ctx.all_provider_models
                        .get(*name).cloned().unwrap_or_default();
                    let new_model = new_models.first().cloned()
                        .unwrap_or_else(|| ctx.session.model.clone());
                    if let Ok(mut eng) = ctx.engine.try_lock() {
                        eng.set_provider(provider, name.to_string());
                        eng.set_model(new_model.clone());
                    }
                    *ctx.provider_name = name.to_string();
                    *ctx.provider_models = new_models;
                    Ok(CommandOutput::Message(format!(
                        "Switched to provider: {}, model: {}", name, new_model
                    )))
                } else {
                    let available: Vec<String> = ctx.all_provider_models.keys().cloned().collect();
                    Err(format!("Provider '{}' not found. Available: {}", name, available.join(", ")))
                }
            }

            // /provider add <name> <format> <base_url>
            // /provider add <name> <format> <base_url> <models>
            ["add", name, format, base_url, ..] => {
                if ctx.all_provider_models.contains_key(*name) {
                    return Err(format!("Provider '{}' already exists. Use /provider edit {}.", name, name));
                }
                if *format != "openai" && *format != "anthropic" {
                    return Err("Format must be 'openai' or 'anthropic'.".into());
                }
                let models_owned: Vec<String> = if parts.len() > 4 {
                    parts[4..].join(" ").split(',').map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()).collect()
                } else {
                    vec![]
                };
                match add_provider_to_config(name, format, Some(base_url), &models_owned) {
                    Ok(_) => {
                        let model_info = if models_owned.is_empty() {
                            "(unrestricted)".to_string()
                        } else {
                            models_owned.join(", ")
                        };
                        Ok(CommandOutput::Messages(vec![
                            format!("Provider '{}' added.", name),
                            format!("  format:   {}", format),
                            format!("  base_url: {}", base_url),
                            format!("  models:   {}", model_info),
                            String::new(),
                            format!("Set API key: export {}_API_KEY=<your-key>", name.to_uppercase().replace("-", "_")),
                            "Restart yode to activate.".into(),
                        ]))
                    }
                    Err(e) => Err(format!("Failed to add provider: {}", e)),
                }
            }

            // /provider add — missing required params
            ["add"] | ["add", _] | ["add", _, _] => {
                Err("Usage: /provider add <name> <openai|anthropic> <base_url> [model1,model2,...]".into())
            }

            // /provider remove <name>
            ["remove", name] => {
                if !ctx.all_provider_models.contains_key(*name) {
                    return Err(format!("Provider '{}' not found.", name));
                }
                if *name == ctx.provider_name.as_str() {
                    return Err(format!("Cannot remove active provider '{}'. Switch first.", name));
                }
                match remove_provider_from_config(name) {
                    Ok(_) => Ok(CommandOutput::Messages(vec![
                        format!("Provider '{}' removed from config.", name),
                        "Restart yode to take effect.".into(),
                    ])),
                    Err(e) => Err(format!("Failed to remove: {}", e)),
                }
            }

            // /provider edit <name> — show current config
            ["edit", name] => {
                show_provider_config(name)
            }

            // /provider edit <name> format <value>
            ["edit", name, "format", value] => {
                if *value != "openai" && *value != "anthropic" {
                    return Err("Format must be 'openai' or 'anthropic'.".into());
                }
                edit_provider_field(name, "format", value)
            }

            // /provider edit <name> base_url <value>
            ["edit", name, "base_url", value] => {
                edit_provider_field(name, "base_url", value)
            }

            // /provider edit <name> api_key <value>
            ["edit", name, "api_key", value] => {
                edit_provider_field(name, "api_key", value)
            }

            // /provider edit <name> models <model1,model2,...>
            ["edit", name, "models", ..] => {
                let models_str = parts[3..].join(" ");
                edit_provider_field(name, "models", &models_str)
            }

            _ => Err(
                "Unknown subcommand. Use /provider for help.".into()
            ),
        }
    }
}

/// Show current config for a provider
fn show_provider_config(name: &str) -> CommandResult {
    let config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    let p = config.llm.providers.get(name)
        .ok_or_else(|| format!("Provider '{}' not found in config.", name))?;

    let models_str = if p.models.is_empty() {
        "(unrestricted)".to_string()
    } else {
        p.models.join(", ")
    };

    Ok(CommandOutput::Messages(vec![
        format!("Provider '{}' config:", name),
        format!("  format:   {}", p.format),
        format!("  base_url: {}", p.base_url.as_deref().unwrap_or("(default)")),
        format!("  api_key:  {}", if p.api_key.is_some() { "(set in config)" } else { "(from env)" }),
        format!("  models:   {}", models_str),
        String::new(),
        "Edit fields:".into(),
        format!("  /provider edit {} format <openai|anthropic>", name),
        format!("  /provider edit {} base_url <url>", name),
        format!("  /provider edit {} api_key <key>", name),
        format!("  /provider edit {} models <model1,model2,...>", name),
    ]))
}

/// Edit a single field of a provider config
fn edit_provider_field(name: &str, field: &str, value: &str) -> CommandResult {
    let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    let p = config.llm.providers.get_mut(name)
        .ok_or_else(|| format!("Provider '{}' not found in config.", name))?;

    match field {
        "format" => {
            p.format = value.to_string();
        }
        "base_url" => {
            p.base_url = Some(value.to_string());
        }
        "api_key" => {
            p.api_key = Some(value.to_string());
        }
        "models" => {
            p.models = value.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        _ => return Err(format!("Unknown field '{}'. Valid: format, base_url, api_key, models", field)),
    }

    config.save().map_err(|e| e.to_string())?;

    Ok(CommandOutput::Messages(vec![
        format!("Updated {}.{} = {}", name, field, value),
        "Config saved. Restart yode to apply.".into(),
    ]))
}

/// Add a provider to ~/.yode/config.toml
fn add_provider_to_config(name: &str, format: &str, base_url: Option<&str>, models: &[String]) -> Result<(), String> {
    let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    config.llm.providers.insert(name.to_string(), yode_core::config::ProviderConfig {
        format: format.to_string(),
        base_url: base_url.map(|u| u.to_string()),
        api_key: None,
        models: models.to_vec(),
    });
    config.save().map_err(|e| e.to_string())
}

/// Remove a provider from ~/.yode/config.toml
fn remove_provider_from_config(name: &str) -> Result<(), String> {
    let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    config.llm.providers.remove(name);
    config.save().map_err(|e| e.to_string())
}
