use crate::app::wizard::{Wizard, WizardStep};

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
            WizardStep::Input {
                prompt: "Models (comma-separated, empty for unrestricted):".into(),
                default: Some(current_models),
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
            Ok(vec![
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
            ])
        }),
    )
    .with_reload_provider(name.to_string()))
}
