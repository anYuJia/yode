pub(super) fn edit_provider_field(
    name: &str,
    field: &str,
    value: &str,
) -> Result<Vec<String>, String> {
    let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    let p = config
        .llm
        .providers
        .get_mut(name)
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
            p.models = value
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        _ => {
            return Err(format!(
                "Unknown field '{}'. Valid: format, base_url, api_key, models",
                field
            ))
        }
    }

    config.save().map_err(|e| e.to_string())?;

    Ok(vec![
        format!("Updated {}.{} = {}", name, field, value),
        "✓ Applied immediately.".into(),
    ])
}

pub(super) fn add_provider_to_config(
    name: &str,
    format: &str,
    base_url: Option<&str>,
    models: &[String],
    api_key: Option<&str>,
) -> Result<(), String> {
    let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    config.llm.providers.insert(
        name.to_string(),
        yode_core::config::ProviderConfig {
            format: format.to_string(),
            base_url: base_url.map(|u| u.to_string()),
            api_key: api_key.map(|k| k.to_string()),
            models: models.to_vec(),
        },
    );
    config.llm.default_provider = name.to_string();
    if let Some(first_model) = models.first() {
        config.llm.default_model = first_model.clone();
    }
    config.save().map_err(|e| e.to_string())
}

pub(super) fn remove_provider_from_config(name: &str) -> Result<(), String> {
    let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    config.llm.providers.remove(name);
    config.save().map_err(|e| e.to_string())
}

pub(super) fn persist_default_provider(name: &str, model: &str) -> Result<String, String> {
    let mut config = yode_core::config::Config::load().map_err(|e| e.to_string())?;
    config.llm.default_provider = name.to_string();
    if !model.is_empty() {
        config.llm.default_model = model.to_string();
    }
    config.save().map_err(|e| e.to_string())?;
    Ok("✓ Config saved (will persist after restart)".to_string())
}
