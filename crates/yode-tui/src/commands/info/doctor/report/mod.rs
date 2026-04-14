mod local;
mod remote;
mod remote_workspace;
mod shared;

use crate::commands::context::CommandContext;
use crate::runtime_artifacts::{write_hook_failure_artifact, write_runtime_timeline_artifact};
use crate::commands::tools::mcp_workspace::write_browser_access_state_artifact;
use self::remote_workspace::{
    build_remote_execution_state, build_remote_workflow_state, remote_command_surface_inventory,
    write_remote_execution_state_artifact, write_remote_execution_stub_inventory,
    write_remote_workflow_capability_artifact,
};

pub(super) fn render_doctor_report(ctx: &mut CommandContext) -> String {
    local::render_doctor_report(ctx)
}

pub(super) fn render_remote_env_check(ctx: &mut CommandContext) -> String {
    remote::render_remote_env_check(ctx)
}

pub(super) fn render_remote_review_prereqs(ctx: &mut CommandContext) -> String {
    remote::render_remote_review_prereqs(ctx)
}

pub(super) fn render_remote_control_doctor(ctx: &mut CommandContext) -> String {
    let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
    crate::commands::dev::remote_control_workspace::render_remote_control_doctor(&project_root)
}

pub(super) fn render_remote_artifact_index(ctx: &mut CommandContext) -> String {
    remote::render_remote_artifact_index(ctx)
}

pub(super) fn export_doctor_bundle(ctx: &mut CommandContext) -> Result<String, String> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let bundle_dir = cwd.join(format!(
        "doctor-bundle-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::create_dir_all(&bundle_dir)
        .map_err(|err| format!("Failed to create doctor bundle dir: {}", err))?;

    let reports = [
        ("local-doctor.txt", local::render_doctor_report(ctx)),
        ("remote-env.txt", remote::render_remote_env_check(ctx)),
        ("remote-review.txt", remote::render_remote_review_prereqs(ctx)),
        ("remote-artifacts.txt", remote::render_remote_artifact_index(ctx)),
    ];
    let report_names = reports
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>();
    let mut copied_files = Vec::new();
    let mut severity_summaries = Vec::new();

    for (name, body) in reports {
        let path = bundle_dir.join(name);
        std::fs::write(&path, body)
            .map_err(|err| format!("Failed to write {}: {}", path.display(), err))?;
        severity_summaries.push(format!(
            "{}: {}",
            name,
            shared::doctor_severity_summary(&std::fs::read_to_string(&path).unwrap_or_default())
        ));
        copied_files.push(path);
    }

    let working_dir = std::path::PathBuf::from(&ctx.session.working_dir);
    if let Some((state, tasks)) = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| (engine.runtime_state(), engine.runtime_tasks_snapshot()))
    {
        if let Some(path) = write_runtime_timeline_artifact(
            &working_dir,
            &ctx.session.session_id,
            &state,
            &tasks,
        ) {
            let dest = bundle_dir.join("runtime-timeline.md");
            std::fs::copy(&path, &dest)
                .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
            copied_files.push(dest);
        }
        if let Some(path) = write_hook_failure_artifact(
            &working_dir,
            &ctx.session.session_id,
            &state,
        ) {
            let dest = bundle_dir.join("hook-failures.md");
            std::fs::copy(&path, &dest)
                .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
            copied_files.push(dest);
        }
    }
    let remote_state = build_remote_workflow_state(ctx);
    let remote_execution_state = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| build_remote_execution_state(&working_dir, Some(&engine.runtime_state())))
        .unwrap_or_default();
    if let Some(path) = write_remote_workflow_capability_artifact(
        &working_dir,
        &ctx.session.session_id,
        &remote_state,
        &remote_command_surface_inventory(),
    ) {
        let dest = bundle_dir.join("remote-workflow-capability.json");
        std::fs::copy(&path, &dest)
            .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
        copied_files.push(dest);
    }
    if let Some(path) = write_remote_execution_state_artifact(
        &working_dir,
        &ctx.session.session_id,
        &remote_execution_state,
    ) {
        let dest = bundle_dir.join("remote-execution-state.json");
        std::fs::copy(&path, &dest)
            .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
        copied_files.push(dest);
    }
    let browser_tools_present = ctx
        .tools
        .definitions()
        .into_iter()
        .any(|definition| matches!(definition.name.as_str(), "web_search" | "web_fetch" | "web_browser"));
    if let Some(path) = write_browser_access_state_artifact(
        &working_dir,
        &ctx.session.session_id,
        browser_tools_present,
        yode_core::config::Config::load()
            .ok()
            .map(|cfg| cfg.mcp.servers.len())
            .unwrap_or(0),
    ) {
        let dest = bundle_dir.join("browser-access-state.json");
        std::fs::copy(&path, &dest)
            .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
        copied_files.push(dest);
    }
    if let Some(path) = write_remote_execution_stub_inventory(&working_dir, &ctx.session.session_id)
    {
        let dest = bundle_dir.join("remote-execution-inventory.md");
        std::fs::copy(&path, &dest)
            .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
        copied_files.push(dest);
    }
    if let Some(bundle) =
        crate::commands::dev::remote_control_workspace::export_remote_control_bundle(&working_dir)
            .map_err(|err| format!("Failed to export remote control bundle: {}", err))?
    {
        let dest = bundle_dir.join("remote-control-bundle.txt");
        std::fs::write(&dest, bundle.display().to_string())
            .map_err(|err| format!("Failed to write {}: {}", dest.display(), err))?;
        copied_files.push(dest);
    }

    let manifest_path = bundle_dir.join("bundle-manifest.json");
    let manifest = serde_json::json!({
        "bundle_dir": bundle_dir.display().to_string(),
        "file_count": copied_files.len(),
        "files": copied_files
            .iter()
            .map(|path| {
                serde_json::json!({
                    "name": path.file_name().and_then(|name| name.to_str()).unwrap_or("file"),
                    "path": path.display().to_string(),
                })
            })
            .collect::<Vec<_>>(),
    });
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".to_string()))
        .map_err(|err| format!("Failed to write {}: {}", manifest_path.display(), err))?;
    copied_files.push(manifest_path.clone());

    let overview_path = bundle_dir.join("bundle-overview.txt");
    std::fs::write(
        &overview_path,
        shared::render_support_bundle_overview(&bundle_dir, &copied_files, &severity_summaries),
    )
    .map_err(|err| format!("Failed to write {}: {}", overview_path.display(), err))?;
    copied_files.push(overview_path.clone());

    let handoff_path = bundle_dir.join("support-handoff.md");
    std::fs::write(
        &handoff_path,
        shared::support_handoff_template(&bundle_dir, &report_names),
    )
    .map_err(|err| format!("Failed to write {}: {}", handoff_path.display(), err))?;
    copied_files.push(handoff_path.clone());

    Ok(format!(
        "{}\n  Navigation: {}",
        shared::doctor_copy_paste_summary(&bundle_dir, &copied_files),
        shared::doctor_bundle_navigation_summary(&bundle_dir)
    ))
}

#[cfg(test)]
pub(super) fn ssh_context_label(
    ssh_tty: Option<&str>,
    ssh_connection: Option<&str>,
) -> &'static str {
    remote::ssh_context_label(ssh_tty, ssh_connection)
}
