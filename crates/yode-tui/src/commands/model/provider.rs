mod config_ops;
mod wizard_builders;

use crate::app::wizard::{Wizard, WizardCompletion, WizardStep};
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
                description: "Show or switch the current provider",
                aliases: &[],
                args: vec![
                    ArgDef {
                        name: "provider".into(),
                        required: false,
                        hint: "<provider-name|add|remove|edit>".into(),
                        completions: ArgCompletionSource::Dynamic(|ctx| provider_completions(
                            ctx.all_provider_models,
                        )),
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
        let trimmed = args.trim();
        let parts: Vec<&str> = trimmed.split_whitespace().collect();

        match parts.as_slice() {
            [] | ["list"] => Ok(CommandOutput::StartWizard(build_provider_picker_wizard(
                ctx.provider_name,
                &ctx.session.model,
                ctx.all_provider_models,
            ))),

            [name] if !matches!(*name, "add" | "remove" | "edit" | "switch") => {
                switch_provider_in_context(name, ctx)
            }

            ["switch", name] => switch_provider_in_context(name, ctx),

            ["add"] | ["add", ..] => {
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

                Ok(CommandOutput::StartWizard(build_add_provider_wizard()))
            }

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

            ["edit", name] => Ok(CommandOutput::StartWizard(build_edit_provider_wizard(name)?)),

            ["edit", name, "format", value] => {
                if *value != "openai" && *value != "anthropic" && *value != "gemini" {
                    return Err("Format must be 'openai', 'anthropic', or 'gemini'.".into());
                }
                let msgs = edit_provider_field(name, "format", value)?;
                Ok(CommandOutput::ReloadProvider {
                    name: name.to_string(),
                    messages: msgs,
                })
            }

            ["edit", name, "base_url", value] => {
                let msgs = edit_provider_field(name, "base_url", value)?;
                Ok(CommandOutput::ReloadProvider {
                    name: name.to_string(),
                    messages: msgs,
                })
            }

            ["edit", name, "api_key", value] => {
                let msgs = edit_provider_field(name, "api_key", value)?;
                Ok(CommandOutput::ReloadProvider {
                    name: name.to_string(),
                    messages: msgs,
                })
            }

            ["edit", name, "models", ..] => {
                let models_str = parts[3..].join(" ");
                let msgs = edit_provider_field(name, "models", &models_str)?;
                Ok(CommandOutput::ReloadProvider {
                    name: name.to_string(),
                    messages: msgs,
                })
            }

            _ => Err("Usage: /provider | /provider <name> | /provider add | /provider remove <name> | /provider edit <name> [field value]".into()),
        }
    }
}

fn switch_provider_in_context(name: &str, ctx: &mut CommandContext) -> CommandResult {
    let provider = ctx
        .provider_registry
        .get(name)
        .ok_or_else(|| {
            let mut available: Vec<String> = ctx.all_provider_models.keys().cloned().collect();
            available.sort();
            format!(
                "Provider '{}' not found. Available: {}",
                name,
                available.join(", ")
            )
        })?;

    let new_models = ctx
        .all_provider_models
        .get(name)
        .cloned()
        .unwrap_or_default();
    let new_model = new_models
        .first()
        .cloned()
        .unwrap_or_else(|| ctx.session.model.clone());

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

    let persist_result = persist_default_provider(name, new_model.as_str());
    let mut messages = vec![format!(
        "Switched to provider: {}, model: {}",
        name, new_model
    )];
    if let Ok(msg) = persist_result {
        messages.push(msg);
    }
    Ok(CommandOutput::Messages(messages))
}

fn provider_completions(
    all_provider_models: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<String> {
    let mut values = vec!["add".to_string(), "remove".to_string(), "edit".to_string()];
    values.extend(all_provider_models.keys().cloned());
    values.sort();
    values.dedup();
    values
}

fn build_provider_picker_wizard(
    current_provider: &str,
    current_model: &str,
    all_provider_models: &std::collections::HashMap<String, Vec<String>>,
) -> Wizard {
    let choices = build_provider_choices(current_provider, current_model, all_provider_models);
    let options = choices
        .iter()
        .map(|choice| choice.display.clone())
        .collect::<Vec<_>>();
    let default = choices
        .iter()
        .position(|choice| choice.name == current_provider)
        .unwrap_or(0);
    let choice_map = choices
        .iter()
        .map(|choice| (choice.display.clone(), (choice.name.clone(), choice.model.clone())))
        .collect::<std::collections::HashMap<_, _>>();

    Wizard::new(
        "Select provider".into(),
        vec![WizardStep::Select {
            prompt: "Choose a provider:".into(),
            options,
            default,
            key: "provider".into(),
        }],
        Box::new(move |answers| {
            let picked = answers
                .get("provider")
                .ok_or("Missing provider selection")?;
            let (provider, model) = choice_map
                .get(picked)
                .cloned()
                .ok_or_else(|| "Unknown provider selection".to_string())?;
            Ok(WizardCompletion::apply_provider_and_model(
                vec![format!(
                    "Switched to provider: {}, model: {}",
                    provider, model
                )],
                provider,
                model,
            ))
        }),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderChoice {
    display: String,
    name: String,
    model: String,
}

fn build_provider_choices(
    current_provider: &str,
    current_model: &str,
    all_provider_models: &std::collections::HashMap<String, Vec<String>>,
) -> Vec<ProviderChoice> {
    let default_provider = load_default_provider_name();
    let default_model = load_default_model_name();
    let mut names = all_provider_models.keys().cloned().collect::<Vec<_>>();
    names.sort();

    names.into_iter()
        .map(|name| {
            let models = all_provider_models.get(&name).cloned().unwrap_or_default();
            let chosen_model = if name == current_provider {
                current_model.to_string()
            } else if models.is_empty() {
                default_model.clone().unwrap_or_else(|| "(unrestricted)".to_string())
            } else {
                models
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "(unrestricted)".to_string())
            };
            let summary = if models.is_empty() {
                "(unrestricted)".to_string()
            } else {
                models.join(", ")
            };
            let mut display = format!("{} — {}", name, summary);
            if name == current_provider {
                display.push_str(" [current]");
            } else if default_provider.as_deref() == Some(name.as_str()) {
                display.push_str(" [default]");
            }
            ProviderChoice {
                display,
                name,
                model: chosen_model,
            }
        })
        .collect()
}

fn load_default_provider_name() -> Option<String> {
    yode_core::config::Config::load()
        .ok()
        .map(|config| config.llm.default_provider)
        .filter(|provider| !provider.trim().is_empty())
}

fn load_default_model_name() -> Option<String> {
    yode_core::config::Config::load()
        .ok()
        .map(|config| config.llm.default_model)
        .filter(|model| !model.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::{build_provider_choices, provider_completions};
    use std::collections::HashMap;

    #[test]
    fn provider_completions_prefer_direct_provider_names() {
        let mut providers = HashMap::new();
        providers.insert("anthropic".to_string(), vec![]);
        providers.insert("pyu".to_string(), vec!["glm-5".to_string()]);
        let completions = provider_completions(&providers);
        assert!(completions.iter().any(|item| item == "anthropic"));
        assert!(!completions.iter().any(|item| item == "list"));
        assert!(!completions.iter().any(|item| item == "switch"));
    }

    #[test]
    fn provider_choices_mark_current_provider() {
        let mut providers = HashMap::new();
        providers.insert("anthropic".to_string(), vec![]);
        providers.insert("pyu".to_string(), vec!["glm-5".to_string()]);
        let choices = build_provider_choices("pyu", "glm-5", &providers);
        assert!(choices
            .iter()
            .any(|choice| choice.display.contains("[current]")));
    }
}
