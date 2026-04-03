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
                let lines = vec![
                    format!("Current provider: {}", ctx.provider_name),
                    format!("Current model: {}", ctx.session.model),
                    String::new(),
                    "Subcommands:".into(),
                    "  /provider list               — List all providers".into(),
                    "  /provider switch <name>       — Switch to a provider".into(),
                    "  /provider add <name>          — Add a new provider".into(),
                    "  /provider remove <name>       — Remove a provider".into(),
                    "  /provider edit <name>         — Edit provider config".into(),
                ];
                Ok(CommandOutput::Messages(lines))
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
                    let new_models = ctx
                        .all_provider_models
                        .get(*name)
                        .cloned()
                        .unwrap_or_default();
                    let new_model = new_models
                        .first()
                        .cloned()
                        .unwrap_or_else(|| ctx.session.model.clone());
                    if let Ok(mut eng) = ctx.engine.try_lock() {
                        eng.set_provider(provider, name.to_string());
                        eng.set_model(new_model.clone());
                    }
                    *ctx.provider_name = name.to_string();
                    *ctx.provider_models = new_models;
                    Ok(CommandOutput::Message(format!(
                        "Switched to provider: {}, model: {}",
                        name, new_model
                    )))
                } else {
                    let available: Vec<String> =
                        ctx.all_provider_models.keys().cloned().collect();
                    Err(format!(
                        "Provider '{}' not found. Available: {}",
                        name,
                        available.join(", ")
                    ))
                }
            }

            // /provider add <name>
            ["add", name] => {
                // Check if already exists
                if ctx.all_provider_models.contains_key(*name) {
                    return Err(format!("Provider '{}' already exists. Use /provider edit {} to modify.", name, name));
                }

                // Add to config with openai format as default
                match add_provider_to_config(name, "openai", None, &[]) {
                    Ok(_) => Ok(CommandOutput::Messages(vec![
                        format!("Provider '{}' added to config.", name),
                        format!("Config saved to ~/.yode/config.toml"),
                        String::new(),
                        format!("Next steps:"),
                        format!("  1. Set API key: export {}_API_KEY=<your-key>", name.to_uppercase().replace("-", "_")),
                        format!("  2. Optionally edit: /provider edit {}", name),
                        format!("  3. Restart yode to activate"),
                    ])),
                    Err(e) => Err(format!("Failed to add provider: {}", e)),
                }
            }

            // /provider add <name> <format>
            ["add", name, format] => {
                if ctx.all_provider_models.contains_key(*name) {
                    return Err(format!("Provider '{}' already exists.", name));
                }
                if *format != "openai" && *format != "anthropic" {
                    return Err("Format must be 'openai' or 'anthropic'.".into());
                }

                match add_provider_to_config(name, format, None, &[]) {
                    Ok(_) => Ok(CommandOutput::Messages(vec![
                        format!("Provider '{}' added (format: {}).", name, format),
                        format!("Config saved to ~/.yode/config.toml"),
                        String::new(),
                        format!("Set API key: export {}_API_KEY=<your-key>", name.to_uppercase().replace("-", "_")),
                        format!("Restart yode to activate."),
                    ])),
                    Err(e) => Err(format!("Failed to add provider: {}", e)),
                }
            }

            // /provider add <name> <format> <base_url>
            ["add", name, format, base_url] => {
                if ctx.all_provider_models.contains_key(*name) {
                    return Err(format!("Provider '{}' already exists.", name));
                }
                if *format != "openai" && *format != "anthropic" {
                    return Err("Format must be 'openai' or 'anthropic'.".into());
                }

                match add_provider_to_config(name, format, Some(base_url), &[]) {
                    Ok(_) => Ok(CommandOutput::Messages(vec![
                        format!("Provider '{}' added (format: {}, url: {}).", name, format, base_url),
                        format!("Config saved to ~/.yode/config.toml"),
                        String::new(),
                        format!("Set API key: export {}_API_KEY=<your-key>", name.to_uppercase().replace("-", "_")),
                        format!("Restart yode to activate."),
                    ])),
                    Err(e) => Err(format!("Failed to add provider: {}", e)),
                }
            }

            // /provider remove <name>
            ["remove", name] => {
                if !ctx.all_provider_models.contains_key(*name) {
                    return Err(format!("Provider '{}' not found.", name));
                }
                if *name == ctx.provider_name.as_str() {
                    return Err(format!("Cannot remove the active provider '{}'. Switch first.", name));
                }

                match remove_provider_from_config(name) {
                    Ok(_) => Ok(CommandOutput::Messages(vec![
                        format!("Provider '{}' removed from config.", name),
                        format!("Restart yode to take effect."),
                    ])),
                    Err(e) => Err(format!("Failed to remove provider: {}", e)),
                }
            }

            // /provider edit <name>
            ["edit", name] => {
                if !ctx.all_provider_models.contains_key(*name) {
                    return Err(format!("Provider '{}' not found.", name));
                }

                let config_path = dirs::home_dir()
                    .map(|h| h.join(".yode").join("config.toml"))
                    .unwrap_or_default();

                if !config_path.exists() {
                    return Err("Config file not found at ~/.yode/config.toml".into());
                }

                // Try to open in $EDITOR
                let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".into());
                match std::process::Command::new(&editor).arg(&config_path).status() {
                    Ok(status) if status.success() => {
                        Ok(CommandOutput::Messages(vec![
                            format!("Config edited. Restart yode to apply changes."),
                            format!("Provider section: [llm.providers.{}]", name),
                        ]))
                    }
                    Ok(_) => Err("Editor exited with error.".into()),
                    Err(e) => {
                        // Fallback: show current config for the provider
                        match yode_core::config::Config::load() {
                            Ok(config) => {
                                if let Some(p) = config.llm.providers.get(*name) {
                                    Ok(CommandOutput::Messages(vec![
                                        format!("Could not open editor ({}). Current config for '{}':", e, name),
                                        format!("  format:   {}", p.format),
                                        format!("  base_url: {}", p.base_url.as_deref().unwrap_or("(default)")),
                                        format!("  api_key:  {}", if p.api_key.is_some() { "(set in config)" } else { "(from env)" }),
                                        format!("  models:   {}", if p.models.is_empty() { "(unrestricted)".into() } else { p.models.join(", ") }),
                                        String::new(),
                                        format!("Edit manually: ~/.yode/config.toml → [llm.providers.{}]", name),
                                    ]))
                                } else {
                                    Err(format!("Provider '{}' not found in config.", name))
                                }
                            }
                            Err(e) => Err(format!("Failed to load config: {}", e)),
                        }
                    }
                }
            }

            _ => Err(format!(
                "Unknown subcommand. Usage:\n  /provider list\n  /provider switch <name>\n  /provider add <name> [format] [base_url]\n  /provider remove <name>\n  /provider edit <name>"
            )),
        }
    }
}

/// Add a provider to ~/.yode/config.toml
fn add_provider_to_config(name: &str, format: &str, base_url: Option<&str>, models: &[&str]) -> Result<(), String> {
    let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;

    let provider_config = yode_core::config::ProviderConfig {
        format: format.to_string(),
        base_url: base_url.map(|u| u.to_string()),
        api_key: None,
        models: models.iter().map(|m| m.to_string()).collect(),
    };

    config.llm.providers.insert(name.to_string(), provider_config);
    config.save().map_err(|e| e.to_string())
}

/// Remove a provider from ~/.yode/config.toml
fn remove_provider_from_config(name: &str) -> Result<(), String> {
    let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    config.llm.providers.remove(name);
    config.save().map_err(|e| e.to_string())
}
