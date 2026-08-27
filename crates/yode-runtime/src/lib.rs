pub mod deliberation;
pub mod desktop_events;
pub mod execution_backend;
pub mod multirepo;
pub mod provider_bootstrap;

pub use deliberation::{
    run_best_of_n, run_debate, BestOfNConfig, BestOfNResult, DebateConfig, DebateResult,
    DebateTurn, DeliberationCandidate, DeliberationRunner, JudgeDecision,
};
pub use desktop_events::{
    engine_event_to_desktop_parts, engine_event_to_runtime_parts, run_status_for_event_kind,
    AskUserPayload, BudgetExceededPayload, CancelledPayload, CancellingPayload,
    DesktopEventEnvelope, DesktopEventKind, DesktopEventParts, DesktopEventPayload,
    PendingConfirmationParts, RuntimeEventParts,
};
pub use execution_backend::{
    detect_standard_backends, BackendAvailability, CloudExecutionBackend, DockerExecutionBackend,
    ExecutionBackend, ExecutionBackendCapabilities, ExecutionBackendKind, ExecutionRequest,
    ExecutionResult, LocalExecutionBackend, SshExecutionBackend,
};
pub use multirepo::{
    execute_multi_repo_plan, MultiRepoExecutionReport, MultiRepoPlan, MultiRepoRunRequest,
    MultiRepoRunner, MultiRepoStep, MultiRepoStepResult, RepositoryTarget,
};
pub use provider_bootstrap::{
    bootstrap_provider_registry, bootstrap_provider_registry_with_options, bootstrap_registry_only,
    resolved_provider_id, ProviderBootstrapMetrics, ProviderBootstrapOptions,
    ProviderBootstrapResult, ProviderInventoryEntry, ProviderSourceBreakdown,
};
