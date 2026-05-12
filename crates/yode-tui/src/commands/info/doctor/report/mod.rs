mod local;
mod remote;
mod remote_workspace;
mod shared;

use self::remote_workspace::{
    build_remote_execution_state, build_remote_workflow_state, remote_command_surface_inventory,
    write_remote_execution_state_artifact, write_remote_execution_stub_inventory,
    write_remote_workflow_capability_artifact,
};
use crate::commands::artifact_nav::export_bundle_root;
use crate::commands::context::CommandContext;
use crate::commands::tools::mcp_workspace::write_browser_access_state_artifact;
use crate::runtime_artifacts::{
    write_hook_failure_artifact, write_media_compact_event_artifact, write_prompt_cache_artifact,
    write_prompt_cache_break_artifact, write_prompt_cache_event_artifact,
    write_prompt_cache_state_artifact, write_runtime_task_inventory_artifact,
    write_runtime_timeline_artifact,
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

pub(super) fn render_restore_doctor(ctx: &mut CommandContext) -> String {
    let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
    crate::commands::session::checkpoint_workspace::render_restore_doctor(&project_root)
}

pub(super) fn render_remote_artifact_index(ctx: &mut CommandContext) -> String {
    remote::render_remote_artifact_index(ctx)
}

pub(super) fn export_doctor_bundle(ctx: &mut CommandContext) -> Result<String, String> {
    let working_dir = std::path::PathBuf::from(&ctx.session.working_dir);
    let bundle_root = export_bundle_root(&working_dir);
    std::fs::create_dir_all(&bundle_root)
        .map_err(|err| format!("Failed to create doctor export dir: {}", err))?;
    let bundle_dir = bundle_root.join(format!(
        "doctor-bundle-{}",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::create_dir_all(&bundle_dir)
        .map_err(|err| format!("Failed to create doctor bundle dir: {}", err))?;

    let reports = [
        ("local-doctor.txt", local::render_doctor_report(ctx)),
        ("remote-env.txt", remote::render_remote_env_check(ctx)),
        (
            "remote-review.txt",
            remote::render_remote_review_prereqs(ctx),
        ),
        (
            "remote-artifacts.txt",
            remote::render_remote_artifact_index(ctx),
        ),
    ];
    let report_names = reports.iter().map(|(name, _)| *name).collect::<Vec<_>>();
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

    let runtime_bundle = ctx
        .engine
        .try_lock()
        .ok()
        .map(|engine| (engine.runtime_state(), engine.runtime_tasks_snapshot()));
    if let Some((state, tasks)) = runtime_bundle.as_ref() {
        if let Some(path) =
            write_runtime_timeline_artifact(&working_dir, &ctx.session.session_id, state, tasks)
        {
            let dest = bundle_dir.join("runtime-timeline.md");
            std::fs::copy(&path, &dest)
                .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
            copied_files.push(dest);
        }
        if let Some(path) = write_runtime_task_inventory_artifact(
            &working_dir,
            &ctx.session.session_id,
            Some(state),
            tasks.clone(),
        ) {
            let dest = bundle_dir.join("runtime-tasks.md");
            std::fs::copy(&path, &dest)
                .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
            copied_files.push(dest);
        }
        if let Some(path) =
            write_hook_failure_artifact(&working_dir, &ctx.session.session_id, state)
        {
            let dest = bundle_dir.join("hook-failures.md");
            std::fs::copy(&path, &dest)
                .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
            copied_files.push(dest);
        }
        if let Some(path) =
            write_prompt_cache_artifact(&working_dir, &ctx.session.session_id, state)
        {
            let dest = bundle_dir.join("prompt-cache.md");
            std::fs::copy(&path, &dest)
                .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
            copied_files.push(dest);
        }
        if let Some(path) =
            write_prompt_cache_state_artifact(&working_dir, &ctx.session.session_id, state)
        {
            let dest = bundle_dir.join("prompt-cache-state.json");
            std::fs::copy(&path, &dest)
                .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
            copied_files.push(dest);
        }
        if let Some(path) =
            write_prompt_cache_event_artifact(&working_dir, &ctx.session.session_id, state)
        {
            let dest = bundle_dir.join("prompt-cache-events.md");
            std::fs::copy(&path, &dest)
                .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
            copied_files.push(dest);
        }
        if let Some(path) =
            write_media_compact_event_artifact(&working_dir, &ctx.session.session_id, state)
        {
            let dest = bundle_dir.join("media-compact-events.md");
            std::fs::copy(&path, &dest)
                .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
            copied_files.push(dest);
        }
        if let Some(path) =
            write_prompt_cache_break_artifact(&working_dir, &ctx.session.session_id, state)
        {
            let dest = bundle_dir.join("prompt-cache-break.json");
            std::fs::copy(&path, &dest)
                .map_err(|err| format!("Failed to copy {}: {}", path, err))?;
            copied_files.push(dest);
        }
        if let Some(path) = state
            .prompt_cache
            .last_prompt_cache_diff_artifact_path
            .as_deref()
        {
            let src = std::path::PathBuf::from(path);
            if src.exists() {
                let dest = bundle_dir.join("prompt-cache-diff.md");
                std::fs::copy(&src, &dest)
                    .map_err(|err| format!("Failed to copy {}: {}", src.display(), err))?;
                copied_files.push(dest);
            }
        }
        for suffix in [
            "post-compact-restore.md",
            "post-compact-restore-state.json",
            "post-compact-restore-diff.md",
        ] {
            if let Some(path) = crate::commands::artifact_nav::latest_artifact_by_suffix(
                &working_dir.join(".yode").join("status"),
                suffix,
            ) {
                let dest = bundle_dir.join(
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("restore-artifact"),
                );
                std::fs::copy(&path, &dest)
                    .map_err(|err| format!("Failed to copy {}: {}", path.display(), err))?;
                copied_files.push(dest);
            }
        }
    }
    copy_mcp_resource_artifacts(&working_dir, &bundle_dir, &mut copied_files)?;
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
        std::fs::copy(&path, &dest).map_err(|err| format!("Failed to copy {}: {}", path, err))?;
        copied_files.push(dest);
    }
    if let Some(path) = write_remote_execution_state_artifact(
        &working_dir,
        &ctx.session.session_id,
        &remote_execution_state,
    ) {
        let dest = bundle_dir.join("remote-execution-state.json");
        std::fs::copy(&path, &dest).map_err(|err| format!("Failed to copy {}: {}", path, err))?;
        copied_files.push(dest);
    }
    let browser_tools_present = ctx.tools.definitions().into_iter().any(|definition| {
        matches!(
            definition.name.as_str(),
            "web_search" | "web_fetch" | "web_browser"
        )
    });
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
        std::fs::copy(&path, &dest).map_err(|err| format!("Failed to copy {}: {}", path, err))?;
        copied_files.push(dest);
    }
    if let Some(path) = write_remote_execution_stub_inventory(&working_dir, &ctx.session.session_id)
    {
        let dest = bundle_dir.join("remote-execution-inventory.md");
        std::fs::copy(&path, &dest).map_err(|err| format!("Failed to copy {}: {}", path, err))?;
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
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".to_string()),
    )
    .map_err(|err| format!("Failed to write {}: {}", manifest_path.display(), err))?;
    copied_files.push(manifest_path.clone());

    let overview_path = bundle_dir.join("bundle-overview.txt");
    std::fs::write(
        &overview_path,
        shared::render_support_bundle_overview(
            &bundle_dir,
            &working_dir,
            &copied_files,
            &severity_summaries,
            runtime_bundle.as_ref().map(|(state, _)| state),
            runtime_bundle
                .as_ref()
                .map(|(_, tasks)| tasks.as_slice())
                .unwrap_or(&[]),
        ),
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

fn copy_mcp_resource_artifacts(
    working_dir: &std::path::Path,
    bundle_dir: &std::path::Path,
    copied_files: &mut Vec<std::path::PathBuf>,
) -> Result<(), String> {
    let candidates = recent_mcp_resource_artifacts(working_dir, 12);
    if candidates.is_empty() {
        return Ok(());
    }
    let dest_dir = bundle_dir.join("mcp-resources");
    std::fs::create_dir_all(&dest_dir)
        .map_err(|err| format!("Failed to create {}: {}", dest_dir.display(), err))?;
    let index_path = write_mcp_resource_bundle_index(&dest_dir, &candidates)?;
    for path in candidates {
        let dest = dest_dir.join(
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("mcp-resource-artifact"),
        );
        std::fs::copy(&path, &dest)
            .map_err(|err| format!("Failed to copy {}: {}", path.display(), err))?;
        copied_files.push(dest);
    }
    if let Some(index_path) = index_path {
        copied_files.push(index_path);
    }
    Ok(())
}

fn write_mcp_resource_bundle_index(
    dest_dir: &std::path::Path,
    candidates: &[std::path::PathBuf],
) -> Result<Option<std::path::PathBuf>, String> {
    let path = dest_dir.join("index.md");
    std::fs::write(
        &path,
        crate::mcp_resource_artifacts::render_mcp_resource_artifact_index(candidates),
    )
    .map_err(|err| format!("Failed to write {}: {}", path.display(), err))?;
    Ok(Some(path))
}

fn recent_mcp_resource_artifacts(
    working_dir: &std::path::Path,
    limit: usize,
) -> Vec<std::path::PathBuf> {
    let dir = working_dir
        .join(".yode")
        .join("status")
        .join("mcp-resources");
    let mut paths = std::fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    paths.sort_by(|left, right| {
        let left_modified = left
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let right_modified = right
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        right_modified
            .cmp(&left_modified)
            .then_with(|| right.file_name().cmp(&left.file_name()))
    });
    paths.into_iter().take(limit).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        copy_mcp_resource_artifacts, recent_mcp_resource_artifacts, write_mcp_resource_bundle_index,
    };

    #[test]
    fn recent_mcp_resource_artifacts_collects_saved_resource_files() {
        let dir = std::env::temp_dir().join(format!(
            "yode-doctor-mcp-resources-{}",
            uuid::Uuid::new_v4()
        ));
        let bundle = dir.join("bundle");
        let resources = dir.join(".yode").join("status").join("mcp-resources");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&resources).unwrap();
        std::fs::create_dir_all(&bundle).unwrap();
        std::fs::write(
            resources.join("a-mcp-resource.md"),
            "# MCP Resource Blob Artifact\n\n- Server: demo\n- URI: mcp://image\n- Blob count: 1\n- Retention: keep newest 120 artifact files\n\n## Blob 1\n\n- Decode warning: invalid base64\n",
        )
        .unwrap();
        std::fs::write(resources.join("a-mcp-resource.b64"), "ZmFrZQ==").unwrap();
        std::fs::write(resources.join("a-mcp-resource.png"), b"fake").unwrap();

        let paths = recent_mcp_resource_artifacts(&dir, 12);
        assert_eq!(paths.len(), 3);

        let mut copied = Vec::new();
        copy_mcp_resource_artifacts(&dir, &bundle, &mut copied).unwrap();
        assert_eq!(copied.len(), 4);
        assert!(bundle
            .join("mcp-resources")
            .join("a-mcp-resource.md")
            .exists());
        assert!(bundle
            .join("mcp-resources")
            .join("a-mcp-resource.b64")
            .exists());
        assert!(bundle
            .join("mcp-resources")
            .join("a-mcp-resource.png")
            .exists());
        let index = std::fs::read_to_string(bundle.join("mcp-resources").join("index.md")).unwrap();
        assert!(index.contains("server=demo"));
        assert!(index.contains("uri=mcp://image"));
        assert!(index.contains("decode_warnings=1"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mcp_resource_bundle_index_handles_decoded_only_files() {
        let dir =
            std::env::temp_dir().join(format!("yode-doctor-mcp-index-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let decoded = dir.join("image.png");
        std::fs::write(&decoded, b"fake").unwrap();

        let index = write_mcp_resource_bundle_index(&dir, &[decoded])
            .unwrap()
            .unwrap();
        let content = std::fs::read_to_string(index).unwrap();
        assert!(content.contains("- Files: 1"));
        assert!(content.contains("- none"));
        assert!(content.contains("/mcp resources cleanup"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
pub(super) fn ssh_context_label(
    ssh_tty: Option<&str>,
    ssh_connection: Option<&str>,
) -> &'static str {
    remote::ssh_context_label(ssh_tty, ssh_connection)
}
