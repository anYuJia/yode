use crate::app::wizard::{Wizard, WizardCompletion, WizardStep};
use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

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
                    hint: "list | default | model name".into(),
                    completions: ArgCompletionSource::Dynamic(|ctx| model_completions(
                        ctx.provider_name,
                        ctx.provider_models,
                    )),
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
        let trimmed = args.trim();
        if trimmed.is_empty() {
            return Ok(CommandOutput::StartWizard(build_model_picker_wizard(
                ctx.provider_name,
                &ctx.session.model,
                ctx.provider_models,
            )));
        }

        if trimmed == "list" {
            return Ok(CommandOutput::Message(render_model_inventory(
                ctx.provider_name,
                &ctx.session.model,
                ctx.provider_models,
            )));
        }

        let resolved_model = resolve_model_request(
            trimmed,
            ctx.provider_name,
            ctx.provider_models,
            &ctx.session.model,
        )?;

        if !ctx.provider_models.is_empty()
            && !ctx.provider_models.contains(&resolved_model)
            && resolved_model != ctx.session.model
        {
            return Err(format!(
                "Model '{}' not available for provider '{}'. Available:\n  {}",
                resolved_model,
                ctx.provider_name,
                ctx.provider_models.join("\n  ")
            ));
        }

        if let Ok(mut eng) = ctx.engine.try_lock() {
            eng.set_model(resolved_model.clone());
        }
        ctx.session.model = resolved_model.clone();
        Ok(CommandOutput::Message(format!(
            "Switched to model: {}\nUse `/effort` to tune reasoning depth if needed.",
            resolved_model
        )))
    }
}

fn render_model_inventory(provider_name: &str, current_model: &str, provider_models: &[String]) -> String {
    let models_list = if provider_models.is_empty() {
        "  (unrestricted; use `/model <name>` to set any model)".to_string()
    } else {
        provider_models
            .iter()
            .map(|model| {
                if model == current_model {
                    format!("  * {} (current)", model)
                } else {
                    format!("    {}", model)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    format!(
        "Current model: {}\nProvider: {}\nAvailable models:\n{}\n\nUse `/model` to open the picker or `/model <name>` to switch directly.",
        current_model, provider_name, models_list
    )
}

fn build_model_picker_wizard(
    provider_name: &str,
    current_model: &str,
    provider_models: &[String],
) -> Wizard {
    let provider_name_owned = provider_name.to_string();
    let current_model_owned = current_model.to_string();
    let choices = build_model_choices(current_model, provider_models);
    let options = choices
        .iter()
        .map(|choice| choice.display.clone())
        .collect::<Vec<_>>();
    let default = choices
        .iter()
        .position(|choice| choice.value == current_model)
        .unwrap_or(0);
    let choice_map = choices
        .iter()
        .map(|choice| (choice.display.clone(), choice.clone()))
        .collect::<std::collections::HashMap<_, _>>();

    Wizard::new(
        format!("Select model for provider '{}'", provider_name),
        vec![WizardStep::Select {
            prompt: "Choose a model:".into(),
            options,
            default,
            key: "model".into(),
        }],
        Box::new(move |answers| {
            let picked = answers.get("model").ok_or("Missing model selection")?;
            let choice = choice_map
                .get(picked)
                .cloned()
                .ok_or_else(|| "Unknown model selection".to_string())?;
            if choice.value == "__add_model__" {
                return Ok(WizardCompletion::next(build_add_model_wizard(
                    &provider_name_owned,
                    &current_model_owned,
                )));
            }
            Ok(WizardCompletion::apply_model(
                vec![format!("Switched to model: {}", choice.value)],
                choice.value,
            ))
        }),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ModelChoice {
    display: String,
    value: String,
}

fn build_model_choices(current_model: &str, provider_models: &[String]) -> Vec<ModelChoice> {
    let mut choices = Vec::<ModelChoice>::new();
    let default_model = load_config_default_model()
        .filter(|model| provider_models.contains(model) || model == current_model);

    for model in provider_models {
        let mut display = model.clone();
        if model == current_model {
            display.push_str(" [current]");
        } else if default_model.as_deref() == Some(model.as_str()) {
            display.push_str(" [default]");
        }
        choices.push(ModelChoice {
            display,
            value: model.clone(),
        });
    }

    if !provider_models.iter().any(|model| model == current_model) {
        choices.insert(
            0,
            ModelChoice {
                display: format!("{} [current]", current_model),
                value: current_model.to_string(),
            },
        );
    }

    choices.push(ModelChoice {
        display: "Add model…".to_string(),
        value: "__add_model__".to_string(),
    });

    choices
}

fn model_completions(provider_name: &str, provider_models: &[String]) -> Vec<String> {
    let mut values = vec!["list".to_string(), "default".to_string()];
    values.extend(provider_models.iter().cloned());
    if provider_name == "anthropic" {
        values.extend(
            ["best", "sonnet", "opus", "haiku"]
                .into_iter()
                .map(str::to_string),
        );
    }
    values.push("add".to_string());
    values.sort();
    values.dedup();
    values
}

fn resolve_model_request(
    raw: &str,
    provider_name: &str,
    provider_models: &[String],
    current_model: &str,
) -> Result<String, String> {
    let value = raw.trim();
    if value.eq_ignore_ascii_case("add") {
        return Err("Use `/model` and choose `Add model…` from the picker.".to_string());
    }
    if value.eq_ignore_ascii_case("default") {
        return load_config_default_model()
            .ok_or_else(|| "No default model configured.".to_string());
    }

    if provider_name == "anthropic" {
        if let Some(model) = resolve_anthropic_alias(value, provider_models, current_model) {
            return Ok(model);
        }
    }

    Ok(value.to_string())
}

fn resolve_anthropic_alias(
    raw: &str,
    provider_models: &[String],
    current_model: &str,
) -> Option<String> {
    let normalized = raw.trim().to_ascii_lowercase();
    let source_models = if provider_models.is_empty() {
        yode_llm::find_provider_info("anthropic")
            .map(|info| info.default_models.iter().map(|item| item.to_string()).collect::<Vec<_>>())
            .unwrap_or_else(|| vec![current_model.to_string()])
    } else {
        provider_models.to_vec()
    };

    match normalized.as_str() {
        "sonnet" => source_models
            .iter()
            .find(|model| model.to_ascii_lowercase().contains("sonnet"))
            .cloned(),
        "opus" => source_models
            .iter()
            .find(|model| model.to_ascii_lowercase().contains("opus"))
            .cloned(),
        "haiku" => source_models
            .iter()
            .find(|model| model.to_ascii_lowercase().contains("haiku"))
            .cloned(),
        "best" => source_models
            .iter()
            .find(|model| model.to_ascii_lowercase().contains("opus"))
            .cloned()
            .or_else(|| source_models.first().cloned()),
        _ => None,
    }
}

fn load_config_default_model() -> Option<String> {
    yode_core::config::Config::load()
        .ok()
        .map(|config| config.llm.default_model)
        .filter(|model| !model.trim().is_empty())
}

fn build_add_model_wizard(provider_name: &str, current_model: &str) -> Wizard {
    let provider_name = provider_name.to_string();
    let current_model = current_model.to_string();
    let reload_provider_name = provider_name.clone();
    Wizard::new(
        format!("Add models to provider '{}'", provider_name),
        vec![WizardStep::Input {
            prompt:
                "Models (comma-separated; first one becomes default/current for this provider):"
                    .into(),
            default: Some(current_model.clone()),
            key: "models".into(),
        }],
        Box::new(move |answers| {
            let models_str = answers.get("models").cloned().unwrap_or_default();
            let models = parse_model_csv(&models_str);
            if models.is_empty() {
                return Err("Please enter at least one model.".to_string());
            }

            let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
            if !config.llm.providers.contains_key(&provider_name) {
                let info = yode_llm::find_provider_info(&provider_name)
                    .ok_or_else(|| format!("Provider '{}' not found.", provider_name))?;
                config.llm.providers.insert(
                    provider_name.clone(),
                    yode_core::config::ProviderConfig {
                        format: info.format.to_string(),
                        base_url: (!info.default_base_url.is_empty())
                            .then(|| info.default_base_url.to_string()),
                        api_key: None,
                        models: Vec::new(),
                    },
                );
            }
            let provider = config
                .llm
                .providers
                .get_mut(&provider_name)
                .ok_or_else(|| format!("Provider '{}' not found.", provider_name))?;
            for model in &models {
                if !provider.models.contains(model) {
                    provider.models.push(model.clone());
                }
            }
            let first_model = models.first().cloned().unwrap_or_else(|| current_model.clone());
            config.llm.default_model = first_model.clone();
            config.save().map_err(|e| e.to_string())?;

            Ok(WizardCompletion::apply_model(
                vec![
                    format!("Added models to provider '{}': {}", provider_name, models.join(", ")),
                    format!("Default/current model: {}", first_model),
                ],
                first_model,
            ))
        }),
    )
    .with_reload_provider(reload_provider_name)
}

fn parse_model_csv(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        build_model_choices, model_completions, parse_model_csv, resolve_anthropic_alias,
        resolve_model_request,
    };

    #[test]
    fn model_choices_mark_current_and_default() {
        let choices = build_model_choices(
            "claude-sonnet-4-20250514",
            &[
                "claude-sonnet-4-20250514".to_string(),
                "claude-opus-4-20250514".to_string(),
            ],
        );
        assert!(choices.iter().any(|choice| choice.display.contains("[current]")));
        assert!(choices.iter().any(|choice| choice.display == "Add model…"));
    }

    #[test]
    fn anthropic_aliases_resolve_from_available_models() {
        let models = vec![
            "claude-sonnet-4-20250514".to_string(),
            "claude-opus-4-20250514".to_string(),
            "claude-haiku-4-20250414".to_string(),
        ];
        assert_eq!(
            resolve_anthropic_alias("sonnet", &models, "claude-sonnet-4-20250514").as_deref(),
            Some("claude-sonnet-4-20250514")
        );
        assert_eq!(
            resolve_anthropic_alias("best", &models, "claude-sonnet-4-20250514").as_deref(),
            Some("claude-opus-4-20250514")
        );
    }

    #[test]
    fn completions_include_picker_and_alias_entries() {
        let completions = model_completions("anthropic", &["claude-sonnet-4-20250514".into()]);
        assert!(completions.iter().any(|item| item == "list"));
        assert!(completions.iter().any(|item| item == "default"));
        assert!(completions.iter().any(|item| item == "sonnet"));
        assert!(completions.iter().any(|item| item == "add"));
    }

    #[test]
    fn direct_model_request_passes_through_plain_names() {
        let resolved = resolve_model_request(
            "claude-sonnet-4-20250514",
            "anthropic",
            &["claude-sonnet-4-20250514".into()],
            "claude-sonnet-4-20250514",
        )
        .unwrap();
        assert_eq!(resolved, "claude-sonnet-4-20250514");
    }

    #[test]
    fn parse_model_csv_supports_multiple_models() {
        assert_eq!(
            parse_model_csv("a, b ,c"),
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }
}
