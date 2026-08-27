use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Agent roles used by the model router. Roles describe the work being done,
/// not a provider-specific model tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelRole {
    Explore,
    Plan,
    Implement,
    Verify,
    Summarize,
    Vision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelCostClass {
    Economy,
    Standard,
    Premium,
}

/// Provider-independent capabilities used by context management and routing.
///
/// Context/output values are conservative runtime budgets rather than a claim
/// about a provider's absolute API maximum. Provider-specific discovery may
/// override these values at runtime without changing routing call sites.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCapabilities {
    pub context_window: usize,
    pub max_output_tokens: usize,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_reasoning: bool,
    pub supports_parallel_tools: bool,
    pub supports_prompt_cache: bool,
    pub cost_class: ModelCostClass,
    /// Relative coding/reasoning quality hint in the range 0..=100.
    pub quality_score: u8,
    /// Relative latency/throughput hint in the range 0..=100.
    pub speed_score: u8,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            context_window: 128_000,
            max_output_tokens: 4_096,
            supports_tools: true,
            supports_vision: false,
            supports_reasoning: false,
            supports_parallel_tools: false,
            supports_prompt_cache: false,
            cost_class: ModelCostClass::Standard,
            quality_score: 60,
            speed_score: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModelIdentity {
    pub provider: String,
    pub model: String,
}

impl ModelIdentity {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCandidate {
    pub provider: String,
    pub model: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteRequest {
    pub role: ModelRole,
    #[serde(default)]
    pub requires_vision: bool,
    #[serde(default)]
    pub requires_tools: bool,
    #[serde(default)]
    pub min_context_window: usize,
    #[serde(default)]
    pub prefer_low_cost: bool,
    #[serde(default)]
    pub avoid_model: Option<ModelIdentity>,
}

impl RouteRequest {
    pub fn for_role(role: ModelRole) -> Self {
        Self {
            role,
            requires_vision: role == ModelRole::Vision,
            requires_tools: matches!(role, ModelRole::Implement | ModelRole::Verify),
            min_context_window: 0,
            prefer_low_cost: matches!(role, ModelRole::Explore | ModelRole::Summarize),
            avoid_model: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteDecision {
    pub provider: String,
    pub model: String,
    pub score: i32,
    pub capabilities: ModelCapabilities,
    pub reasons: Vec<String>,
}

/// Central capability registry. Exact provider/model overrides take precedence
/// over built-in conservative family profiles.
#[derive(Debug, Clone, Default)]
pub struct ModelCapabilityRegistry {
    overrides: HashMap<(String, String), ModelCapabilities>,
}

impl ModelCapabilityRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(
        &mut self,
        provider: impl Into<String>,
        model: impl Into<String>,
        capabilities: ModelCapabilities,
    ) {
        self.overrides.insert(
            (provider.into().to_ascii_lowercase(), model.into().to_ascii_lowercase()),
            capabilities,
        );
    }

    pub fn resolve(&self, provider: &str, model: &str) -> ModelCapabilities {
        let provider_key = provider.trim().to_ascii_lowercase();
        let model_key = model.trim().to_ascii_lowercase();
        self.overrides
            .get(&(provider_key, model_key.clone()))
            .or_else(|| self.overrides.get(&("*".to_string(), model_key.clone())))
            .cloned()
            .unwrap_or_else(|| builtin_capabilities(provider, model))
    }

    /// Resolve model-only limits for components that do not know provider yet,
    /// such as the context manager created from a session model string.
    pub fn resolve_model(&self, model: &str) -> ModelCapabilities {
        self.resolve("", model)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModelRouter {
    registry: ModelCapabilityRegistry,
}

impl ModelRouter {
    pub fn new(registry: ModelCapabilityRegistry) -> Self {
        Self { registry }
    }

    pub fn registry(&self) -> &ModelCapabilityRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut ModelCapabilityRegistry {
        &mut self.registry
    }

    pub fn route(&self, request: &RouteRequest, candidates: &[ModelCandidate]) -> Option<RouteDecision> {
        candidates
            .iter()
            .filter(|candidate| candidate.enabled)
            .filter_map(|candidate| self.score_candidate(request, candidate))
            .max_by(|left, right| {
                left.score
                    .cmp(&right.score)
                    .then_with(|| right.provider.cmp(&left.provider))
                    .then_with(|| right.model.cmp(&left.model))
            })
    }

    fn score_candidate(
        &self,
        request: &RouteRequest,
        candidate: &ModelCandidate,
    ) -> Option<RouteDecision> {
        let capabilities = self.registry.resolve(&candidate.provider, &candidate.model);
        if request.requires_vision && !capabilities.supports_vision {
            return None;
        }
        if request.requires_tools && !capabilities.supports_tools {
            return None;
        }
        if capabilities.context_window < request.min_context_window {
            return None;
        }

        let mut score = 0i32;
        let mut reasons = Vec::new();
        match request.role {
            ModelRole::Explore => {
                score += capabilities.speed_score as i32 * 2;
                score += capabilities.quality_score as i32;
                reasons.push("exploration favors speed with sufficient quality".to_string());
            }
            ModelRole::Plan => {
                score += capabilities.quality_score as i32 * 2;
                if capabilities.supports_reasoning {
                    score += 35;
                    reasons.push("reasoning capability helps planning".to_string());
                }
            }
            ModelRole::Implement => {
                score += capabilities.quality_score as i32 * 2;
                score += capabilities.speed_score as i32 / 2;
                if capabilities.supports_parallel_tools {
                    score += 20;
                    reasons.push("parallel tool capability helps implementation".to_string());
                }
            }
            ModelRole::Verify => {
                score += capabilities.quality_score as i32 * 2;
                if capabilities.supports_reasoning {
                    score += 25;
                    reasons.push("reasoning capability helps verification".to_string());
                }
            }
            ModelRole::Summarize => {
                score += capabilities.speed_score as i32 * 2;
                score += capabilities.quality_score as i32 / 2;
            }
            ModelRole::Vision => {
                score += capabilities.quality_score as i32 * 2;
                score += capabilities.speed_score as i32 / 2;
                reasons.push("vision support is required".to_string());
            }
        }

        if request.prefer_low_cost {
            score += match capabilities.cost_class {
                ModelCostClass::Economy => 35,
                ModelCostClass::Standard => 15,
                ModelCostClass::Premium => 0,
            };
            reasons.push("low-cost preference applied".to_string());
        }

        if request
            .avoid_model
            .as_ref()
            .is_some_and(|avoid| {
                avoid.provider.eq_ignore_ascii_case(&candidate.provider)
                    && avoid.model.eq_ignore_ascii_case(&candidate.model)
            })
        {
            // Verification should prefer an independent second opinion when a
            // viable candidate exists, without making the current model invalid.
            score -= 80;
            reasons.push("same model as avoided candidate penalized".to_string());
        }

        Some(RouteDecision {
            provider: candidate.provider.clone(),
            model: candidate.model.clone(),
            score,
            capabilities,
            reasons,
        })
    }
}

pub fn builtin_capabilities(provider: &str, model: &str) -> ModelCapabilities {
    let provider = provider.to_ascii_lowercase();
    let model = model.to_ascii_lowercase();
    let mut caps = ModelCapabilities::default();

    if model.contains("claude") {
        caps.context_window = 200_000;
        caps.max_output_tokens = 8_192;
        caps.supports_tools = true;
        caps.supports_reasoning = model.contains("opus") || model.contains("sonnet");
        caps.supports_parallel_tools = true;
        caps.supports_prompt_cache = true;
        caps.supports_vision = !model.contains("haiku");
        caps.quality_score = if model.contains("opus") { 94 } else { 88 };
        caps.speed_score = if model.contains("haiku") { 92 } else { 68 };
        caps.cost_class = if model.contains("haiku") {
            ModelCostClass::Economy
        } else if model.contains("opus") {
            ModelCostClass::Premium
        } else {
            ModelCostClass::Standard
        };
    } else if model.contains("gpt-4o") || model.contains("gpt-4.1") {
        caps.context_window = 128_000;
        caps.max_output_tokens = 8_192;
        caps.supports_vision = true;
        caps.supports_tools = true;
        caps.supports_parallel_tools = true;
        caps.quality_score = 86;
        caps.speed_score = if model.contains("mini") { 90 } else { 72 };
        caps.cost_class = if model.contains("mini") {
            ModelCostClass::Economy
        } else {
            ModelCostClass::Standard
        };
    } else if model.starts_with("o1") || model.starts_with("o3") || model.starts_with("o4") {
        caps.context_window = 128_000;
        caps.max_output_tokens = 16_384;
        caps.supports_tools = true;
        caps.supports_reasoning = true;
        caps.quality_score = 92;
        caps.speed_score = 48;
        caps.cost_class = ModelCostClass::Premium;
    } else if model.contains("gemini") {
        caps.context_window = 128_000;
        caps.max_output_tokens = 8_192;
        caps.supports_tools = true;
        caps.supports_vision = true;
        caps.supports_reasoning = model.contains("pro");
        caps.supports_parallel_tools = true;
        caps.quality_score = if model.contains("pro") { 88 } else { 78 };
        caps.speed_score = if model.contains("flash") { 92 } else { 66 };
        caps.cost_class = if model.contains("flash") {
            ModelCostClass::Economy
        } else {
            ModelCostClass::Standard
        };
    } else if model.contains("deepseek-reasoner") || model.contains("deepseek-r1") {
        caps.supports_reasoning = true;
        caps.quality_score = 86;
        caps.speed_score = 50;
    } else if model.contains("qwen")
        || model.contains("kimi")
        || model.contains("glm")
        || model.contains("minimax")
        || model.contains("codestral")
    {
        caps.context_window = 128_000;
        caps.max_output_tokens = 8_192;
        caps.supports_tools = true;
        caps.supports_reasoning = model.contains("thinking") || model.contains("reasoner");
        caps.quality_score = 80;
        caps.speed_score = 68;
    }

    if provider == "ollama" {
        // Local model names are user-defined. Keep capability claims conservative
        // unless a provider/model override is registered from runtime discovery.
        caps.supports_vision = false;
        caps.supports_parallel_tools = false;
        caps.supports_prompt_cache = false;
        caps.cost_class = ModelCostClass::Economy;
    }

    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(provider: &str, model: &str) -> ModelCandidate {
        ModelCandidate {
            provider: provider.to_string(),
            model: model.to_string(),
            enabled: true,
        }
    }

    #[test]
    fn exact_override_wins_over_builtin_profile() {
        let mut registry = ModelCapabilityRegistry::new();
        let custom = ModelCapabilities {
            context_window: 42_000,
            supports_vision: true,
            ..ModelCapabilities::default()
        };
        registry.register("custom", "model-a", custom.clone());
        assert_eq!(registry.resolve("custom", "model-a"), custom);
    }

    #[test]
    fn verify_route_prefers_independent_capable_model() {
        let router = ModelRouter::default();
        let mut request = RouteRequest::for_role(ModelRole::Verify);
        request.avoid_model = Some(ModelIdentity::new("openai", "gpt-4o"));
        let decision = router
            .route(
                &request,
                &[
                    candidate("openai", "gpt-4o"),
                    candidate("anthropic", "claude-sonnet-4-20250514"),
                ],
            )
            .unwrap();
        assert_eq!(decision.provider, "anthropic");
    }

    #[test]
    fn vision_route_filters_non_vision_candidates() {
        let router = ModelRouter::default();
        let decision = router
            .route(
                &RouteRequest::for_role(ModelRole::Vision),
                &[
                    candidate("ollama", "qwen2.5-coder"),
                    candidate("openai", "gpt-4o"),
                ],
            )
            .unwrap();
        assert_eq!(decision.model, "gpt-4o");
        assert!(decision.capabilities.supports_vision);
    }

    #[test]
    fn routing_respects_minimum_context() {
        let mut registry = ModelCapabilityRegistry::new();
        registry.register(
            "tiny",
            "tiny-model",
            ModelCapabilities {
                context_window: 8_000,
                ..ModelCapabilities::default()
            },
        );
        let router = ModelRouter::new(registry);
        let mut request = RouteRequest::for_role(ModelRole::Explore);
        request.min_context_window = 32_000;
        assert!(router
            .route(&request, &[candidate("tiny", "tiny-model")])
            .is_none());
    }
}
