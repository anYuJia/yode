use super::super::App;

pub(in crate::app) fn reload_provider_from_config(name: &str, app: &mut App) {
    let config = match yode_core::config::Config::load() {
        Ok(c) => c,
        Err(_) => return,
    };
    let p_config = match config.llm.providers.get(name) {
        Some(c) => c,
        None => return,
    };

    let env_prefix = name.to_uppercase().replace("-", "_");
    let api_key = std::env::var(format!("{}_API_KEY", env_prefix))
        .ok()
        .or_else(|| p_config.api_key.clone())
        .or_else(|| {
            if p_config.format == "openai" {
                std::env::var("OPENAI_API_KEY").ok()
            } else {
                std::env::var("ANTHROPIC_API_KEY")
                    .or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
                    .ok()
            }
        });

    let api_key = match api_key {
        Some(k) if !k.is_empty() => k,
        _ => return,
    };

    let default_base = if p_config.format == "openai" {
        "https://api.openai.com/v1"
    } else {
        "https://api.anthropic.com"
    };
    let base_url = std::env::var(format!("{}_BASE_URL", env_prefix))
        .ok()
        .or_else(|| p_config.base_url.clone())
        .unwrap_or_else(|| default_base.to_string());

    let provider: std::sync::Arc<dyn yode_llm::provider::LlmProvider> =
        if p_config.format == "openai" {
            std::sync::Arc::new(yode_llm::providers::openai::OpenAiProvider::new(
                name, api_key, base_url,
            ))
        } else {
            std::sync::Arc::new(yode_llm::providers::anthropic::AnthropicProvider::new(
                name, api_key, base_url,
            ))
        };

    app.provider_registry.register(provider.clone());

    if let Some(p_cfg) = config.llm.providers.get(name) {
        app.all_provider_models
            .insert(name.to_string(), p_cfg.models.clone());
    }

    if app.provider_name == name {
        app.provider_models = p_config.models.clone();
        if let Some(ref engine) = app.engine {
            if let Ok(mut eng) = engine.try_lock() {
                eng.set_provider(provider, name.to_string());
            }
        }
    }
}
