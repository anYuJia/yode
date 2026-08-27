use crate::runtime;
use serde_json::{json, Value};
use yode_llm::capabilities::{
    ModelCandidate, ModelCapabilityRegistry, ModelIdentity, ModelRole, ModelRouter, RouteRequest,
};

#[tauri::command]
pub async fn agent_intelligence_snapshot(
    runtime: tauri::State<'_, runtime::DesktopRuntime>,
) -> Result<Value, String> {
    let bootstrap = runtime.bootstrap().map_err(|error| error.to_string())?;
    let default_llm = runtime
        .config_get_default_llm()
        .map_err(|error| error.to_string())?;
    let providers = runtime.config_get_providers().map_err(|error| error.to_string())?;

    let registry = ModelCapabilityRegistry::default();
    let router = ModelRouter::new(registry.clone());
    let current_capabilities = registry.resolve(&default_llm.provider, &default_llm.model);
    let mut candidates = Vec::new();
    for provider in providers.iter().filter(|provider| provider.enabled) {
        if provider.models.is_empty() {
            if provider.id == default_llm.provider {
                candidates.push(ModelCandidate {
                    provider: provider.id.clone(),
                    model: default_llm.model.clone(),
                    enabled: true,
                });
            }
        } else {
            for model in &provider.models {
                candidates.push(ModelCandidate {
                    provider: provider.id.clone(),
                    model: model.clone(),
                    enabled: true,
                });
            }
        }
    }
    if candidates.is_empty() {
        candidates.push(ModelCandidate {
            provider: default_llm.provider.clone(),
            model: default_llm.model.clone(),
            enabled: true,
        });
    }

    let roles = [
        ModelRole::Explore,
        ModelRole::Plan,
        ModelRole::Implement,
        ModelRole::Verify,
        ModelRole::Summarize,
        ModelRole::Vision,
    ];
    let routes = roles
        .into_iter()
        .map(|role| {
            let mut request = RouteRequest::for_role(role);
            if role == ModelRole::Verify {
                request.avoid_model = Some(ModelIdentity::new(
                    default_llm.provider.clone(),
                    default_llm.model.clone(),
                ));
            }
            let decision = router.route(&request, &candidates);
            json!({"role": role, "decision": decision})
        })
        .collect::<Vec<_>>();

    let workspace = std::path::PathBuf::from(&bootstrap.workspace_path);
    let sandbox = yode_tools::sandbox::prepare_shell("true", &workspace, false)
        .map(|prepared| serde_json::to_value(prepared.info).unwrap_or_else(|_| json!({})))
        .unwrap_or_else(|error| json!({"error": error.to_string(), "sandboxed": false}));
    let backends = yode_runtime::detect_standard_backends().await;
    let learning = yode_core::learning::LearningStore::for_workspace(&workspace)
        .summary()
        .unwrap_or_default();

    Ok(json!({
        "current": {
            "provider": default_llm.provider,
            "model": default_llm.model,
            "capabilities": current_capabilities,
        },
        "routes": routes,
        "sandbox": sandbox,
        "executionBackends": backends,
        "learning": learning,
        "workspace": bootstrap.workspace_path,
        "runtime": {
            "multiRepo": true,
            "bestOfN": true,
            "debate": true,
            "githubDelivery": true,
            "mandatoryVerification": true,
            "semanticRepositoryIndex": true,
            "parallelDag": true,
            "realBrowser": true
        }
    }))
}
