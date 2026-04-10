use std::sync::Arc;

use anyhow::{Context, Result};

use yode_core::config::Config;
use yode_llm::providers::anthropic::AnthropicProvider;
use yode_llm::providers::gemini::GeminiProvider;
use yode_llm::providers::openai::OpenAiProvider;
use yode_llm::registry::ProviderRegistry;

pub(crate) struct ProviderBootstrapResult {
    pub provider_registry: Arc<ProviderRegistry>,
    pub provider_name: String,
    pub provider_models: Vec<String>,
    pub all_provider_models: std::collections::HashMap<String, Vec<String>>,
    pub provider: Arc<dyn yode_llm::provider::LlmProvider>,
    pub model: String,
}

pub(crate) fn bootstrap_provider_registry(
    cli_provider: Option<String>,
    cli_model: Option<String>,
    config: &Config,
) -> Result<ProviderBootstrapResult> {
    let provider_registry = ProviderRegistry::new();

    for (name, p_config) in &config.llm.providers {
        let env_prefix = name.to_uppercase().replace("-", "_");
        let api_key = match std::env::var(format!("{}_API_KEY", env_prefix))
            .ok()
            .or_else(|| p_config.api_key.clone())
            .or_else(|| {
                if let Some(info) = yode_llm::find_provider_info(name) {
                    info.env_keys.iter().find_map(|k| std::env::var(k).ok())
                } else if p_config.format == "openai" {
                    std::env::var("OPENAI_API_KEY").ok()
                } else if p_config.format == "anthropic" {
                    std::env::var("ANTHROPIC_API_KEY")
                        .or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
                        .ok()
                } else if p_config.format == "gemini" {
                    std::env::var("GOOGLE_API_KEY")
                        .or_else(|_| std::env::var("GEMINI_API_KEY"))
                        .ok()
                } else {
                    None
                }
            }) {
            Some(k) => k,
            None => {
                if name == "ollama" || p_config.format == "ollama" {
                    String::new()
                } else {
                    tracing::warn!(
                        "Provider '{}' is configured but missing an API key, skipping.",
                        name
                    );
                    continue;
                }
            }
        };

        let known = yode_llm::find_provider_info(name);
        let default_base =
            known
                .map(|k| k.default_base_url)
                .unwrap_or(match p_config.format.as_str() {
                    "openai" => "https://api.openai.com/v1",
                    "anthropic" => "https://api.anthropic.com",
                    "gemini" => "https://generativelanguage.googleapis.com/v1beta",
                    _ => "https://api.openai.com/v1",
                });

        let base_url = match std::env::var(format!("{}_BASE_URL", env_prefix))
            .ok()
            .or_else(|| p_config.base_url.clone())
        {
            Some(u) => {
                if u.is_empty() {
                    default_base.to_string()
                } else {
                    u
                }
            }
            None => default_base.to_string(),
        };

        match p_config.format.as_str() {
            "anthropic" => {
                provider_registry
                    .register(Arc::new(AnthropicProvider::new(name, &api_key, &base_url)));
            }
            "gemini" => {
                let mut p = GeminiProvider::new(&api_key);
                if base_url != "https://generativelanguage.googleapis.com/v1beta" {
                    p = p.with_base_url(&base_url);
                }
                provider_registry.register(Arc::new(p));
            }
            _ => {
                provider_registry
                    .register(Arc::new(OpenAiProvider::new(name, &api_key, &base_url)));
            }
        }
    }

    for info in yode_llm::detect_available_providers() {
        if provider_registry.contains(info.name) {
            continue;
        }
        let api_key = info
            .env_keys
            .iter()
            .find_map(|k| std::env::var(k).ok())
            .unwrap_or_default();
        match info.format {
            "anthropic" => {
                provider_registry.register(Arc::new(AnthropicProvider::new(
                    info.name,
                    &api_key,
                    info.default_base_url,
                )));
            }
            "gemini" => {
                provider_registry.register(Arc::new(GeminiProvider::new(&api_key)));
            }
            _ => {
                provider_registry.register(Arc::new(OpenAiProvider::new(
                    info.name,
                    &api_key,
                    info.default_base_url,
                )));
            }
        }
    }

    let provider_name = cli_provider.unwrap_or_else(|| config.llm.default_provider.clone());

    let all_provider_models: std::collections::HashMap<String, Vec<String>> = config
        .llm
        .providers
        .iter()
        .filter(|(name, _)| provider_registry.get(name).is_some())
        .map(|(name, p_config)| (name.clone(), p_config.models.clone()))
        .collect();

    let provider_models = all_provider_models
        .get(&provider_name)
        .cloned()
        .unwrap_or_default();
    let model = {
        let requested = cli_model.unwrap_or_else(|| config.llm.default_model.clone());
        if !provider_models.is_empty() && !provider_models.contains(&requested) {
            let first = provider_models[0].clone();
            tracing::warn!(
                "Model '{}' not in provider '{}' model list, using '{}' instead. Available: {:?}",
                requested, provider_name, first, provider_models
            );
            eprintln!(
                "⚠ Model '{}' not available for provider '{}', using '{}' instead.",
                requested, provider_name, first
            );
            first
        } else {
            requested
        }
    };

    let provider_registry = Arc::new(provider_registry);
    let provider = provider_registry.get(&provider_name).context(format!(
        "Provider '{}' not available. Set the appropriate API key environment variable.\n\
         - OpenAI: OPENAI_API_KEY\n\
         - Anthropic: ANTHROPIC_API_KEY or ANTHROPIC_AUTH_TOKEN",
        provider_name
    ))?;

    Ok(ProviderBootstrapResult {
        provider_registry,
        provider_name,
        provider_models,
        all_provider_models,
        provider,
        model,
    })
}
