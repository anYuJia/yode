mod artifacts;
mod session_restore;
mod startup_profile;
mod startup_summary;
mod tooling;

pub(crate) use artifacts::{
    write_mcp_startup_failure_artifact, write_permission_policy_artifact,
    write_provider_inventory_artifact, write_startup_profile_artifact,
    write_startup_bundle_manifest_artifact, write_tooling_inventory_artifact,
};
pub(crate) use session_restore::{
    configure_permissions, ensure_session_exists, restore_or_create_context, shutdown_mcp_clients,
};
pub(crate) use startup_profile::StartupProfiler;
pub(crate) use startup_summary::{
    append_startup_segments, build_startup_resume_segment, parse_startup_summary_segment,
    tooling_phase_summary,
};
pub(crate) use tooling::{init_logging, setup_tooling};
