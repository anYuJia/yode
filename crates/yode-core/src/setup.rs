use crate::config::Config;

/// Check if any API key is configured either in ENV or Config
pub fn has_api_keys_configured() -> bool {
    let has_env = std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("ANTHROPIC_API_KEY").is_ok()
        || std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok();

    let has_config = if let Ok(config) = Config::load() {
        config.llm.providers.values().any(|p| p.api_key.is_some())
    } else {
        false
    };

    has_env || has_config
}

/// Check if any API key is configured without blocking the async runtime.
pub async fn has_api_keys_configured_async() -> bool {
    let has_env = std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("ANTHROPIC_API_KEY").is_ok()
        || std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok();

    let has_config = if let Ok(config) = Config::load_async().await {
        config.llm.providers.values().any(|p| p.api_key.is_some())
    } else {
        false
    };

    has_env || has_config
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    #[test]
    fn default_setup_config_parses_without_panicking() {
        let default_str = include_str!("../../../config/default.toml");
        let config: Config = toml::from_str(default_str).unwrap();
        assert!(!config.llm.default_provider.is_empty());
        assert!(!config.llm.default_model.is_empty());
    }
}
