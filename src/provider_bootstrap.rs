use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};

use yode_core::config::{Config, ProviderConfig};
use yode_llm::providers::anthropic::AnthropicProvider;
use yode_llm::providers::gemini::GeminiProvider;
use yode_llm::providers::openai::OpenAiProvider;
use yode_llm::registry::{ProviderInfo, ProviderRegistry};

pub(crate) struct ProviderBootstrapResult {
    pub provider_registry: Arc<ProviderRegistry>,
    pub provider_name: String,
    pub provider_models: Vec<String>,
    pub all_provider_models: std::collections::HashMap<String, Vec<String>>,
    pub provider: Arc<dyn yode_llm::provider::LlmProvider>,
    pub model: String,
    pub metrics: ProviderBootstrapMetrics,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ProviderBootstrapMetrics {
    pub configured_registered: usize,
    pub env_detected_registered: usize,
    pub total_registered: usize,
    pub capability_summary: String,
    pub source_breakdown: ProviderSourceBreakdown,
    pub provider_details: Vec<ProviderInventoryEntry>,
    pub duration_ms: u64,
}

impl ProviderBootstrapMetrics {
    pub(crate) fn summary(&self) -> String {
        format!(
            "providers[configured={} env_detected={} total={} capabilities=\"{}\" sources=\"{}\" duration={}ms]",
            self.configured_registered,
            self.env_detected_registered,
            self.total_registered,
            self.capability_summary,
            self.source_breakdown.summary(),
            self.duration_ms
        )
    }
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub(crate) struct ProviderSourceBreakdown {
    pub configured_env_override: usize,
    pub configured_inline: usize,
    pub configured_fallback_env: usize,
    pub env_detected: usize,
    pub none_required: usize,
    pub base_url_env_override: usize,
    pub base_url_config_override: usize,
    pub base_url_default: usize,
}

impl ProviderSourceBreakdown {
    fn summary(&self) -> String {
        format!(
            "cfg_env={} cfg_inline={} cfg_fallback={} env_detected={} none={} base_env={} base_config={} base_default={}",
            self.configured_env_override,
            self.configured_inline,
            self.configured_fallback_env,
            self.env_detected,
            self.none_required,
            self.base_url_env_override,
            self.base_url_config_override,
            self.base_url_default,
        )
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct ProviderInventoryEntry {
    pub name: String,
    pub format: String,
    pub model_count: usize,
    pub registration_source: String,
    pub api_key_source: String,
    pub base_url_source: String,
    pub base_url: String,
}

#[derive(Debug, Clone)]
enum ApiKeySource {
    EnvOverride(String),
    ConfigInline,
    FallbackEnv(String),
    NoneRequired,
}

impl ApiKeySource {
    fn label(&self) -> String {
        match self {
            Self::EnvOverride(key) => format!("env_override:{}", key),
            Self::ConfigInline => "config:inline".to_string(),
            Self::FallbackEnv(key) => format!("env_fallback:{}", key),
            Self::NoneRequired => "none_required".to_string(),
        }
    }

    fn apply(&self, breakdown: &mut ProviderSourceBreakdown) {
        match self {
            Self::EnvOverride(_) => breakdown.configured_env_override += 1,
            Self::ConfigInline => breakdown.configured_inline += 1,
            Self::FallbackEnv(_) => breakdown.configured_fallback_env += 1,
            Self::NoneRequired => breakdown.none_required += 1,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum BaseUrlSource {
    EnvOverride,
    ConfigOverride,
    Default,
}

impl BaseUrlSource {
    fn label(self) -> &'static str {
        match self {
            Self::EnvOverride => "env_override",
            Self::ConfigOverride => "config_override",
            Self::Default => "default",
        }
    }

    fn apply(self, breakdown: &mut ProviderSourceBreakdown) {
        match self {
            Self::EnvOverride => breakdown.base_url_env_override += 1,
            Self::ConfigOverride => breakdown.base_url_config_override += 1,
            Self::Default => breakdown.base_url_default += 1,
        }
    }
}

pub(crate) fn bootstrap_provider_registry(
    cli_provider: Option<String>,
    cli_model: Option<String>,
    config: &Config,
) -> Result<ProviderBootstrapResult> {
    let started_at = Instant::now();
    let provider_registry = ProviderRegistry::new();
    let mut configured_registered = 0usize;
    let mut env_detected_registered = 0usize;
    let mut source_breakdown = ProviderSourceBreakdown::default();
    let mut provider_details = Vec::new();

    for (name, p_config) in &config.llm.providers {
        let known = yode_llm::find_provider_info(name);
        let (api_key, api_key_source) = match resolve_configured_api_key(name, p_config, known) {
            Some(resolved) => resolved,
            None => {
                tracing::warn!(
                    "Provider '{}' is configured but missing an API key, skipping.",
                    name
                );
                continue;
            }
        };
        let (base_url, base_url_source) = resolve_base_url(name, p_config, known);

        register_provider(&provider_registry, name, &p_config.format, &api_key, &base_url);
        configured_registered += 1;
        api_key_source.apply(&mut source_breakdown);
        base_url_source.apply(&mut source_breakdown);
        provider_details.push(ProviderInventoryEntry {
            name: name.clone(),
            format: p_config.format.clone(),
            model_count: p_config.models.len(),
            registration_source: "configured".to_string(),
            api_key_source: api_key_source.label(),
            base_url_source: base_url_source.label().to_string(),
            base_url,
        });
    }

    for info in yode_llm::detect_available_providers() {
        if provider_registry.contains(info.name) {
            continue;
        }
        let Some((api_key, env_key)) = info
            .env_keys
            .iter()
            .find_map(|key| std::env::var(key).ok().map(|value| (value, (*key).to_string())))
        else {
            continue;
        };
        register_provider(
            &provider_registry,
            info.name,
            info.format,
            &api_key,
            info.default_base_url,
        );
        env_detected_registered += 1;
        source_breakdown.env_detected += 1;
        source_breakdown.base_url_default += 1;
        provider_details.push(ProviderInventoryEntry {
            name: info.name.to_string(),
            format: info.format.to_string(),
            model_count: info.default_models.len(),
            registration_source: "env_detected".to_string(),
            api_key_source: format!("env_detected:{}", env_key),
            base_url_source: BaseUrlSource::Default.label().to_string(),
            base_url: info.default_base_url.to_string(),
        });
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
    let model =
        {
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
    let capability_summary = summarize_provider_capabilities(&provider_details);
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
        metrics: ProviderBootstrapMetrics {
            configured_registered,
            env_detected_registered,
            total_registered: configured_registered + env_detected_registered,
            capability_summary,
            source_breakdown,
            provider_details,
            duration_ms: started_at.elapsed().as_millis() as u64,
        },
    })
}

fn default_base_url(format: &str, known: Option<&'static ProviderInfo>) -> &'static str {
    known.map(|provider| provider.default_base_url).unwrap_or(match format {
        "openai" => "https://api.openai.com/v1",
        "anthropic" => "https://api.anthropic.com",
        "gemini" => "https://generativelanguage.googleapis.com/v1beta",
        _ => "https://api.openai.com/v1",
    })
}

fn resolve_configured_api_key(
    name: &str,
    p_config: &ProviderConfig,
    known: Option<&'static ProviderInfo>,
) -> Option<(String, ApiKeySource)> {
    let env_prefix = name.to_uppercase().replace('-', "_");
    let override_key = format!("{}_API_KEY", env_prefix);
    if let Ok(api_key) = std::env::var(&override_key) {
        return Some((api_key, ApiKeySource::EnvOverride(override_key)));
    }
    if let Some(api_key) = p_config.api_key.clone() {
        return Some((api_key, ApiKeySource::ConfigInline));
    }
    if let Some((api_key, key)) = known
        .and_then(provider_env_key_from_info)
        .or_else(|| provider_env_key_from_format(&p_config.format))
    {
        return Some((api_key, ApiKeySource::FallbackEnv(key)));
    }
    if name == "ollama" || p_config.format == "ollama" {
        return Some((String::new(), ApiKeySource::NoneRequired));
    }
    None
}

fn resolve_base_url(
    name: &str,
    p_config: &ProviderConfig,
    known: Option<&'static ProviderInfo>,
) -> (String, BaseUrlSource) {
    let default_base = default_base_url(&p_config.format, known).to_string();
    let env_prefix = name.to_uppercase().replace('-', "_");
    let override_key = format!("{}_BASE_URL", env_prefix);
    if let Ok(base_url) = std::env::var(&override_key) {
        return if base_url.is_empty() {
            (default_base, BaseUrlSource::Default)
        } else {
            (base_url, BaseUrlSource::EnvOverride)
        };
    }
    if let Some(base_url) = p_config.base_url.clone() {
        return if base_url.is_empty() {
            (default_base, BaseUrlSource::Default)
        } else {
            (base_url, BaseUrlSource::ConfigOverride)
        };
    }
    (default_base, BaseUrlSource::Default)
}

fn provider_env_key_from_info(info: &'static ProviderInfo) -> Option<(String, String)> {
    info.env_keys
        .iter()
        .find_map(|key| std::env::var(key).ok().map(|value| (value, (*key).to_string())))
}

fn provider_env_key_from_format(format: &str) -> Option<(String, String)> {
    match format {
        "openai" => std::env::var("OPENAI_API_KEY")
            .ok()
            .map(|value| (value, "OPENAI_API_KEY".to_string())),
        "anthropic" => std::env::var("ANTHROPIC_API_KEY")
            .or_else(|_| std::env::var("ANTHROPIC_AUTH_TOKEN"))
            .ok()
            .map(|value| {
                let key = if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                    "ANTHROPIC_API_KEY"
                } else {
                    "ANTHROPIC_AUTH_TOKEN"
                };
                (value, key.to_string())
            }),
        "gemini" => std::env::var("GOOGLE_API_KEY")
            .or_else(|_| std::env::var("GEMINI_API_KEY"))
            .ok()
            .map(|value| {
                let key = if std::env::var("GOOGLE_API_KEY").is_ok() {
                    "GOOGLE_API_KEY"
                } else {
                    "GEMINI_API_KEY"
                };
                (value, key.to_string())
            }),
        _ => None,
    }
}

fn register_provider(
    provider_registry: &ProviderRegistry,
    name: &str,
    format: &str,
    api_key: &str,
    base_url: &str,
) {
    match format {
        "anthropic" => {
            provider_registry.register(Arc::new(AnthropicProvider::new(name, api_key, base_url)));
        }
        "gemini" => {
            let mut provider = GeminiProvider::new(api_key);
            if base_url != "https://generativelanguage.googleapis.com/v1beta" {
                provider = provider.with_base_url(base_url);
            }
            provider_registry.register(Arc::new(provider));
        }
        _ => {
            provider_registry.register(Arc::new(OpenAiProvider::new(name, api_key, base_url)));
        }
    }
}

fn summarize_provider_capabilities(entries: &[ProviderInventoryEntry]) -> String {
    let mut summary = entries
        .iter()
        .map(|entry| {
            format!(
                "{}:{} models={} source={} base={}",
                entry.name,
                entry.format,
                entry.model_count,
                entry.registration_source,
                entry.base_url_source
            )
        })
        .collect::<Vec<_>>();
    summary.sort();
    summary.join(" | ")
}
