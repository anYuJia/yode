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
                        .unwrap_or_default(); // empty if unrestricted
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
                    Ok(CommandOutput::Message(format!(
                        "Switched to provider: {}, model: {}",
                        name,
                        if new_model.is_empty() { &ctx.session.model } else { &new_model }
                    )))
                } else {
                    let available: Vec<String> = ctx.all_provider_models.keys().cloned().collect();
                    Err(format!("Provider '{}' not found. Available: {}", name, available.join(", ")))
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
                    if format != "openai" && format != "anthropic" {
                        return Err("Format must be 'openai' or 'anthropic'.".into());
                    }
                    let models_owned: Vec<String> = if parts.len() > 4 {
                        parts[4..].join(" ").split(',').map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty()).collect()
                    } else {
                        vec![]
                    };
                    match add_provider_to_config(name, format, Some(base_url), &models_owned, None) {
                        Ok(_) => {
                            let model_info = if models_owned.is_empty() { "(unrestricted)".into() } else { models_owned.join(", ") };
                            return Ok(CommandOutput::Messages(vec![
                                format!("✓ Provider '{}' added!", name),
                                format!("  format:   {}", format),
                                format!("  base_url: {}", base_url),
                                format!("  models:   {}", model_info),
                                String::new(),
                                format!("Set API key: export {}_API_KEY=<your-key>", name.to_uppercase().replace("-", "_")),
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
                use crate::app::wizard::{Wizard, WizardStep};

                let wizard = Wizard::new(
                    "添加 LLM 提供商".into(),
                    vec![
                        WizardStep::Select {
                            prompt: "选择提供商类型:".into(),
                            options: vec![
                                "Anthropic (Claude)".into(),
                                "OpenAI (GPT)".into(),
                                "Kimi (Moonshot)".into(),
                                "DeepSeek".into(),
                                "Google Gemini".into(),
                                "自定义 (Custom)".into(),
                            ],
                            default: 0,
                            key: "provider_type".into(),
                        },
                        WizardStep::Input {
                            prompt: "Base URL:".into(),
                            default: Some("https://api.anthropic.com".into()),
                            key: "base_url".into(),
                        },
                        WizardStep::Input {
                            prompt: "API Key:".into(),
                            default: None,
                            key: "api_key".into(),
                        },
                        WizardStep::Input {
                            prompt: "Provider 名称:".into(),
                            default: Some("anthropic".into()),
                            key: "name".into(),
                        },
                        WizardStep::Input {
                            prompt: "默认模型:".into(),
                            default: Some("claude-sonnet-4-20250514".into()),
                            key: "model".into(),
                        },
                    ],
                    Box::new(|answers| {
                        let provider_type = answers.get("provider_type").ok_or("Missing type")?;
                        let name = answers.get("name").ok_or("Missing name")?;
                        let base_url = answers.get("base_url").ok_or("Missing base_url")?;
                        let api_key = answers.get("api_key").ok_or("Missing api_key")?;
                        let model = answers.get("model").cloned().unwrap_or_default();

                        let format = if provider_type.contains("Anthropic") {
                            "anthropic"
                        } else {
                            "openai"
                        };

                        let models = if model.is_empty() { vec![] } else {
                            model.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
                        };

                        add_provider_to_config(name, format, Some(base_url.as_str()), &models, Some(api_key))?;

                        Ok(vec![
                            format!("✓ Provider '{}' 已添加!", name),
                            format!("  format:   {}", format),
                            format!("  base_url: {}", base_url),
                            format!("  model:    {}", if model.is_empty() { "(unrestricted)" } else { &model }),
                            format!("  api_key:  {}...{}", &api_key[..4.min(api_key.len())], &api_key[api_key.len().saturating_sub(4)..]),
                            String::new(),
                            "重启 yode 以激活新提供商。".into(),
                        ])
                    }),
                ).with_step_callback(Box::new(|value, steps| {
                    // After selecting provider type, update defaults for remaining steps
                    let (default_url, name_hint, model_hint) = match value {
                        v if v.contains("Anthropic") => ("https://api.anthropic.com", "anthropic", "claude-sonnet-4-20250514"),
                        v if v.contains("OpenAI") => ("https://api.openai.com/v1", "openai", "gpt-4o"),
                        v if v.contains("Kimi") => ("https://api.moonshot.cn/v1", "kimi", "moonshot-v1-auto"),
                        v if v.contains("DeepSeek") => ("https://api.deepseek.com", "deepseek", "deepseek-chat"),
                        v if v.contains("Gemini") => ("https://generativelanguage.googleapis.com/v1beta/openai/", "gemini", "gemini-2.5-flash"),
                        _ => return,
                    };
                    if let Some(WizardStep::Input { default, .. }) = steps.get_mut(1) {
                        *default = Some(default_url.into());
                    }
                    if let Some(WizardStep::Input { default, .. }) = steps.get_mut(3) {
                        *default = Some(name_hint.into());
                    }
                    if let Some(WizardStep::Input { default, .. }) = steps.get_mut(4) {
                        *default = Some(model_hint.into());
                    }
                }));

                Ok(CommandOutput::StartWizard(wizard))
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

            // /provider edit <name> — interactive edit wizard with current values as defaults
            ["edit", name] => {
                let config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
                let p = config.llm.providers.get(*name)
                    .ok_or_else(|| format!("Provider '{}' not found in config.", name))?;

                use crate::app::wizard::{Wizard, WizardStep};

                let current_format = p.format.clone();
                let current_url = p.base_url.clone().unwrap_or_default();
                let current_models = if p.models.is_empty() { String::new() } else { p.models.join(", ") };
                let provider_name = name.to_string();

                let format_default = if current_format == "anthropic" { 1 } else { 0 };

                let wizard = Wizard::new(
                    format!("Editing provider '{}' (Enter to keep current)", name),
                    vec![
                        WizardStep::Select {
                            prompt: "API format:".into(),
                            options: vec!["openai".into(), "anthropic".into()],
                            default: format_default,
                            key: "format".into(),
                        },
                        WizardStep::Input {
                            prompt: "Base URL:".into(),
                            default: Some(current_url),
                            key: "base_url".into(),
                        },
                        WizardStep::Input {
                            prompt: "Models (comma-separated, empty for unrestricted):".into(),
                            default: Some(current_models),
                            key: "models".into(),
                        },
                    ],
                    Box::new(move |answers| {
                        let format = answers.get("format").ok_or("Missing format")?;
                        let base_url = answers.get("base_url").ok_or("Missing base_url")?;
                        let models_str = answers.get("models").cloned().unwrap_or_default();

                        let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
                        let p = config.llm.providers.get_mut(&provider_name)
                            .ok_or_else(|| format!("Provider '{}' not found.", provider_name))?;

                        p.format = format.clone();
                        p.base_url = if base_url.is_empty() { None } else { Some(base_url.clone()) };
                        p.models = models_str.split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        let model_info: String = if p.models.is_empty() { "(unrestricted)".into() } else { p.models.join(", ") };

                        config.save().map_err(|e| e.to_string())?;
                        Ok(vec![
                            format!("Provider '{}' updated!", provider_name),
                            format!("  format:   {}", format),
                            format!("  base_url: {}", if base_url.is_empty() { "(default)" } else { base_url.as_str() }),
                            format!("  models:   {}", model_info),
                            "Config saved. Restart yode to apply.".into(),
                        ])
                    }),
                );

                Ok(CommandOutput::StartWizard(wizard))
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
fn add_provider_to_config(name: &str, format: &str, base_url: Option<&str>, models: &[String], api_key: Option<&str>) -> Result<(), String> {
    let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    config.llm.providers.insert(name.to_string(), yode_core::config::ProviderConfig {
        format: format.to_string(),
        base_url: base_url.map(|u| u.to_string()),
        api_key: api_key.map(|k| k.to_string()),
        models: models.to_vec(),
    });
    config.llm.default_provider = name.to_string();
    if let Some(first_model) = models.first() {
        config.llm.default_model = first_model.clone();
    }
    config.save().map_err(|e| e.to_string())
}

/// Remove a provider from ~/.yode/config.toml
fn remove_provider_from_config(name: &str) -> Result<(), String> {
    let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    config.llm.providers.remove(name);
    config.save().map_err(|e| e.to_string())
}
