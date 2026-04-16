use crate::app::wizard::{Wizard, WizardCompletion, WizardStep};

pub(crate) fn build_edit_provider_wizard(name: &str) -> Result<Wizard, String> {
    let config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    let provider = config
        .llm
        .providers
        .get(name)
        .ok_or_else(|| format!("Provider '{}' not found in config.", name))?;

    let current_format = provider.format.clone();
    let current_url = provider.base_url.clone().unwrap_or_default();
    let current_api_key = provider.api_key.clone().unwrap_or_default();
    let current_models = if provider.models.is_empty() {
        String::new()
    } else {
        provider.models.join(", ")
    };
    let model_picker_options = editable_model_options(name, &provider.models, &current_models);
    let provider_name = name.to_string();

    let format_default = match current_format.as_str() {
        "anthropic" => 1,
        "gemini" => 2,
        _ => 0,
    };

    let masked_key = if current_api_key.len() > 8 {
        format!(
            "{}...{}",
            &current_api_key[..4],
            &current_api_key[current_api_key.len() - 4..]
        )
    } else if !current_api_key.is_empty() {
        "****".to_string()
    } else {
        String::new()
    };

    Ok(Wizard::new(
        format!("Editing provider '{}' (Enter to keep current)", name),
        vec![
            WizardStep::Select {
                prompt: "API format:".into(),
                options: vec!["openai".into(), "anthropic".into(), "gemini".into()],
                default: format_default,
                key: "format".into(),
            },
            WizardStep::Input {
                prompt: "Base URL:".into(),
                default: Some(current_url),
                key: "base_url".into(),
            },
            WizardStep::Input {
                prompt: format!(
                    "API Key (current: {}): ",
                    if masked_key.is_empty() {
                        "not set"
                    } else {
                        &masked_key
                    }
                ),
                default: Some(current_api_key),
                key: "api_key".into(),
            },
            WizardStep::Select {
                prompt: "Select a model preset (fills the next field, you can still edit it):"
                    .into(),
                options: model_picker_options.clone(),
                default: editable_model_default_index(&model_picker_options, &current_models),
                key: "model_picker".into(),
            },
            WizardStep::Input {
                prompt: "Models (comma-separated, empty for unrestricted):".into(),
                default: Some(current_models.clone()),
                key: "models".into(),
            },
        ],
        Box::new(move |answers| {
            let format = answers.get("format").ok_or("Missing format")?;
            let base_url = answers.get("base_url").ok_or("Missing base_url")?;
            let api_key = answers.get("api_key").cloned().unwrap_or_default();
            let models_str = answers.get("models").cloned().unwrap_or_default();

            let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
            let provider = config
                .llm
                .providers
                .get_mut(&provider_name)
                .ok_or_else(|| format!("Provider '{}' not found.", provider_name))?;

            provider.format = format.clone();
            provider.base_url = if base_url.is_empty() {
                None
            } else {
                Some(base_url.clone())
            };
            provider.api_key = if api_key.is_empty() {
                None
            } else {
                Some(api_key.clone())
            };
            provider.models = models_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let model_info: String = if provider.models.is_empty() {
                "(unrestricted)".into()
            } else {
                provider.models.join(", ")
            };
            let key_display = if api_key.is_empty() {
                "(not set)".to_string()
            } else if api_key.len() > 8 {
                format!("{}...{}", &api_key[..4], &api_key[api_key.len() - 4..])
            } else {
                "****".to_string()
            };

            config.save().map_err(|e| e.to_string())?;
            Ok(WizardCompletion::messages(vec![
                format!("Provider '{}' updated!", provider_name),
                format!("  format:   {}", format),
                format!(
                    "  base_url: {}",
                    if base_url.is_empty() {
                        "(default)"
                    } else {
                        base_url.as_str()
                    }
                ),
                format!("  api_key:  {}", key_display),
                format!("  models:   {}", model_info),
                "✓ Applied immediately.".into(),
            ]))
        }),
    )
    .with_step_callback(Box::new(move |value, steps| {
        if !model_picker_options.iter().any(|item| item == value) {
            return;
        }
        if let Some(WizardStep::Input { default, .. }) = steps.get_mut(4) {
            *default = Some(match value.trim() {
                "(keep current list)" => default.clone().unwrap_or_default(),
                "(unrestricted)" => String::new(),
                other => other.to_string(),
            });
        }
    }))
    .with_reload_provider(name.to_string()))
}

fn editable_model_options(
    provider_name: &str,
    current_models: &[String],
    current_models_joined: &str,
) -> Vec<String> {
    let mut options = Vec::<String>::new();
    if !current_models_joined.trim().is_empty() {
        options.push("(keep current list)".to_string());
    }
    for model in current_models {
        if !options.contains(model) {
            options.push(model.clone());
        }
    }
    if let Some(info) = yode_llm::find_provider_info(provider_name) {
        for model in info.default_models {
            let model = model.to_string();
            if !options.contains(&model) {
                options.push(model);
            }
        }
    }
    options.push("(unrestricted)".to_string());
    options
}

fn editable_model_default_index(options: &[String], current_models_joined: &str) -> usize {
    if !current_models_joined.trim().is_empty() {
        options
            .iter()
            .position(|item| item == "(keep current list)")
            .unwrap_or(0)
    } else {
        options
            .iter()
            .position(|item| item == "(unrestricted)")
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::editable_model_options;

    #[test]
    fn edit_picker_options_include_known_and_current_models() {
        let options = editable_model_options(
            "anthropic",
            &["claude-sonnet-4-20250514".to_string()],
            "claude-sonnet-4-20250514",
        );
        assert!(options.iter().any(|item| item == "(keep current list)"));
        assert!(options
            .iter()
            .any(|item| item.contains("claude-opus")));
    }
}
