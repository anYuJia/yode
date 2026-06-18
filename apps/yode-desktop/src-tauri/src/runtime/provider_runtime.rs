use std::sync::Arc;

use anyhow::Result;

use yode_core::config::Config;
use yode_llm::registry::ProviderRegistry;
use yode_runtime::resolved_provider_id;

use super::DesktopRuntime;
use crate::protocol::{DefaultLlm, DesktopProvider};

impl DesktopRuntime {
    pub fn config_get_providers(&self) -> Result<Vec<DesktopProvider>> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let mut providers = Vec::new();
        for (id, p) in &config.llm.providers {
            let resolved_id = resolved_provider_id(id, p);
            if resolved_id.trim().is_empty() {
                continue;
            }
            let name = match resolved_id.as_str() {
                "anthropic" => "Anthropic Claude".to_string(),
                "openai" => "OpenAI".to_string(),
                "google" | "gemini" => "Google Gemini".to_string(),
                "deepseek" => "DeepSeek (深度求索)".to_string(),
                "doubao" => "豆包".to_string(),
                "ollama" => "Ollama (本地运行)".to_string(),
                _ => resolved_id.to_uppercase(),
            };
            providers.push(DesktopProvider {
                id: resolved_id,
                name,
                format: p.format.clone(),
                enabled: p.enabled.unwrap_or(true),
                api_key: p.api_key.clone().unwrap_or_default(),
                base_url: p.base_url.clone().unwrap_or_default(),
                models: p.models.clone(),
                gradient: p.gradient.clone(),
            });
        }
        let order = [
            "openai",
            "anthropic",
            "gemini",
            "google",
            "deepseek",
            "ollama",
        ];
        providers.sort_by_key(|p| order.iter().position(|&x| x == p.id).unwrap_or(99));
        Ok(providers)
    }

    pub fn config_get_default_llm(&self) -> Result<DefaultLlm> {
        let config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        Ok(DefaultLlm {
            provider: config.llm.default_provider.clone(),
            model: config.llm.default_model.clone(),
        })
    }

    pub fn config_set_default_llm(&self, provider: String, model: String) -> Result<DefaultLlm> {
        let provider = provider.trim().to_string();
        let model = model.trim().to_string();
        if provider.is_empty() || model.is_empty() {
            anyhow::bail!("provider and model cannot be empty");
        }
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        if !config
            .llm
            .providers
            .iter()
            .any(|(id, provider_config)| resolved_provider_id(id, provider_config) == provider)
        {
            anyhow::bail!("Provider '{}' not found", provider);
        }
        let (provider, model) = normalized_provider_model(&config, &provider, &model);
        config.llm.default_provider = provider;
        config.llm.default_model = model;
        config.save()?;
        Ok(DefaultLlm {
            provider: config.llm.default_provider.clone(),
            model: config.llm.default_model.clone(),
        })
    }

    pub fn config_save_providers(&self, providers: Vec<DesktopProvider>) -> Result<()> {
        let mut config = self
            .config
            .lock()
            .map_err(|_| anyhow::anyhow!("config lock poisoned"))?;
        let mut new_providers = std::collections::HashMap::new();
        for p in providers {
            let id = p.id.trim().to_string();
            if id.is_empty() {
                continue;
            }
            new_providers.insert(
                id,
                yode_core::config::ProviderConfig {
                    format: p.format,
                    base_url: if p.base_url.is_empty() {
                        None
                    } else {
                        Some(p.base_url)
                    },
                    api_key: if p.api_key.is_empty() {
                        None
                    } else {
                        Some(p.api_key)
                    },
                    models: p.models,
                    enabled: Some(p.enabled),
                    gradient: p.gradient,
                },
            );
        }
        if !new_providers.iter().any(|(id, provider_config)| {
            resolved_provider_id(id, provider_config) == config.llm.default_provider
        }) {
            if let Some((provider, config_provider)) = new_providers
                .iter()
                .find(|(_, provider)| provider.enabled.unwrap_or(true))
                .or_else(|| new_providers.iter().next())
            {
                config.llm.default_provider = provider.clone();
                config.llm.default_model = config_provider
                    .models
                    .first()
                    .cloned()
                    .unwrap_or_else(|| config.llm.default_model.clone());
            }
        }
        config.llm.providers = new_providers;
        let (provider, model) = normalized_provider_model(
            &config,
            &config.llm.default_provider,
            &config.llm.default_model,
        );
        config.llm.default_provider = provider;
        config.llm.default_model = model;
        config.save()?;

        let new_registry = bootstrap_providers(&config);
        let mut reg_guard = self
            .provider_registry
            .lock()
            .map_err(|_| anyhow::anyhow!("registry lock poisoned"))?;
        *reg_guard = new_registry;

        Ok(())
    }

    pub async fn config_test_provider(&self, p: DesktopProvider) -> Result<()> {
        let api_key = resolve_provider_api_key(&p.id, &p.format, p.api_key.trim());
        let base_url = resolve_provider_base_url(&p.id, &p.format, p.base_url.trim());
        let provider: Arc<dyn yode_llm::provider::LlmProvider> = match p.format.as_str() {
            "anthropic" => Arc::new(yode_llm::providers::anthropic::AnthropicProvider::new(
                &p.id, &api_key, &base_url,
            )),
            "gemini" => {
                let mut provider = yode_llm::providers::gemini::GeminiProvider::new(&api_key);
                if base_url != "https://generativelanguage.googleapis.com/v1beta" {
                    provider = provider.with_base_url(&base_url);
                }
                Arc::new(provider)
            }
            _ => Arc::new(yode_llm::providers::openai::OpenAiProvider::new(
                &p.id, &api_key, &base_url,
            )),
        };

        let _models = provider.list_models().await?;
        Ok(())
    }
}

fn resolve_provider_api_key(id: &str, format: &str, configured: &str) -> String {
    if !configured.is_empty() {
        return configured.to_string();
    }

    let env_prefix = id.to_uppercase().replace('-', "_");
    let mut candidates = vec![format!("{}_API_KEY", env_prefix)];
    candidates.extend(match (id, format) {
        ("anthropic", _) | (_, "anthropic") => vec![
            "ANTHROPIC_API_KEY".to_string(),
            "ANTHROPIC_AUTH_TOKEN".to_string(),
        ],
        ("gemini", _) | ("google", _) | (_, "gemini") => {
            vec!["GOOGLE_API_KEY".to_string(), "GEMINI_API_KEY".to_string()]
        }
        ("deepseek", _) => vec!["DEEPSEEK_API_KEY".to_string()],
        ("openai", _) => vec!["OPENAI_API_KEY".to_string()],
        _ => Vec::new(),
    });

    candidates
        .into_iter()
        .find_map(|key| std::env::var(key).ok())
        .unwrap_or_default()
}

fn resolve_provider_base_url(id: &str, format: &str, configured: &str) -> String {
    let env_prefix = id.to_uppercase().replace('-', "_");
    let override_base = format!("{}_BASE_URL", env_prefix);
    if let Ok(url) = std::env::var(override_base) {
        return url;
    }
    if !configured.is_empty() {
        return configured.to_string();
    }
    match format {
        "anthropic" => "https://api.anthropic.com".to_string(),
        "gemini" => "https://generativelanguage.googleapis.com/v1beta".to_string(),
        _ => "https://api.openai.com/v1".to_string(),
    }
}

pub(super) fn normalized_provider_model(
    config: &Config,
    provider: &str,
    model: &str,
) -> (String, String) {
    let provider = provider.trim();
    let model = model.trim();

    let configured_provider = config
        .llm
        .providers
        .iter()
        .find(|(id, provider_config)| {
            resolved_provider_id(id, provider_config) == provider
                && provider_config.enabled.unwrap_or(true)
        })
        .map(|(_, provider_config)| provider_config);

    if let Some(provider_config) = configured_provider {
        if provider_config.models.is_empty()
            || provider_config
                .models
                .iter()
                .any(|candidate| candidate == model)
        {
            return (provider.to_string(), model.to_string());
        }
        if let Some(first_model) = provider_config.models.first() {
            return (provider.to_string(), first_model.clone());
        }
    }

    if let Some(default_provider) = config
        .llm
        .providers
        .get(&config.llm.default_provider)
        .filter(|provider_config| provider_config.enabled.unwrap_or(true))
    {
        let fallback_model = default_provider
            .models
            .first()
            .cloned()
            .unwrap_or_else(|| config.llm.default_model.clone());
        return (config.llm.default_provider.clone(), fallback_model);
    }

    if let Some((fallback_provider, fallback_config)) =
        config.llm.providers.iter().find(|(id, provider_config)| {
            !resolved_provider_id(id, provider_config).trim().is_empty()
                && provider_config.enabled.unwrap_or(true)
        })
    {
        let fallback_model = fallback_config
            .models
            .first()
            .cloned()
            .unwrap_or_else(|| config.llm.default_model.clone());
        return (
            resolved_provider_id(fallback_provider, fallback_config),
            fallback_model,
        );
    }

    (
        config.llm.default_provider.clone(),
        config.llm.default_model.clone(),
    )
}

pub(super) fn bootstrap_providers(config: &Config) -> Arc<ProviderRegistry> {
    yode_runtime::bootstrap_registry_only(config)
}
