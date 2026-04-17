use crate::commands::workspace_text::{workspace_bullets, WorkspaceText};
use crate::ui::status_summary::{
    context_window_summary_text, runtime_status_snapshot_from_parts,
    session_runtime_summary_text, tool_runtime_summary_text,
};
use yode_core::engine::EngineRuntimeState;
use yode_tools::RuntimeTask;

pub(super) fn render_section(title: &str, checks: &[String]) -> String {
    if checks.is_empty() {
        return String::new();
    }
    format!("{}\n{}\n", title, checks.join("\n"))
}

pub(super) fn format_artifact_entry(path: &std::path::Path) -> String {
    let metadata = std::fs::metadata(path).ok();
    let size = metadata
        .as_ref()
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let modified = metadata
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|stamp| stamp.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|stamp| stamp.as_secs().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    format!("  - {} ({} bytes, mtime={})", path.display(), size, modified)
}

pub(super) fn doctor_severity_summary(report: &str) -> String {
    let mut ok = 0usize;
    let mut warn = 0usize;
    let mut err = 0usize;
    for line in report.lines() {
        if line.contains("[ok]") {
            ok += 1;
        } else if line.contains("[--]") {
            warn += 1;
        } else if line.contains("[!!]") {
            err += 1;
        }
    }
    format!("ok={} warn={} err={}", ok, warn, err)
}

pub(super) fn doctor_checklist(report_names: &[&str]) -> String {
    report_names
        .iter()
        .map(|name| format!("- {}", name))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn artifact_freshness_summary(paths: &[std::path::PathBuf]) -> String {
    if paths.is_empty() {
        return "none".to_string();
    }
    let mut labels = Vec::new();
    for path in paths.iter().take(4) {
        let modified = std::fs::metadata(path)
            .ok()
            .and_then(|meta| meta.modified().ok())
            .and_then(|stamp| stamp.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|stamp| stamp.as_secs().to_string())
            .unwrap_or_else(|| "unknown".to_string());
        labels.push(format!(
            "{}@{}",
            path.file_name().and_then(|name| name.to_str()).unwrap_or("artifact"),
            modified
        ));
    }
    labels.join(" | ")
}

pub(super) fn render_support_bundle_overview(
    bundle_dir: &std::path::Path,
    project_root: &std::path::Path,
    copied_files: &[std::path::PathBuf],
    report_summaries: &[String],
    runtime: Option<&EngineRuntimeState>,
    tasks: &[RuntimeTask],
) -> String {
    let mut workspace = WorkspaceText::new("Support bundle overview")
        .subtitle(bundle_dir.display().to_string())
        .field("Files", copied_files.len().to_string())
        .field("Freshness", artifact_freshness_summary(copied_files));
    if let Some(state) = runtime {
        let running_tasks = tasks
            .iter()
            .filter(|task| matches!(task.status, yode_tools::RuntimeTaskStatus::Running))
            .count();
        let snapshot = runtime_status_snapshot_from_parts(
            project_root,
            Some(state.clone()),
            running_tasks,
        );
        workspace = workspace
            .field(
                "Runtime",
                session_runtime_summary_text(&snapshot, state.estimated_context_tokens),
            )
            .field(
                "Context",
                context_window_summary_text(Some(state), state.estimated_context_tokens),
            )
            .field("Tools", tool_runtime_summary_text(state))
            .field(
                "Tasks",
                format!("{} total / {} running", tasks.len(), running_tasks),
            );
    }
    workspace
        .section("Report severities", workspace_bullets(report_summaries.to_vec()))
        .render()
}

pub(super) fn support_handoff_template(
    bundle_dir: &std::path::Path,
    report_names: &[&str],
) -> String {
    format!(
        "# Support Handoff\n\n- Bundle: {}\n- Included reports:\n{}\n\n```text\nWhat to inspect first: local-doctor.txt, bundle-overview.txt, runtime-timeline.md, runtime-tasks.md\nIf runtime stalls or hook failures are suspected, inspect hook-failures.md, runtime-timeline.md, and runtime-tasks.md\n```\n",
        bundle_dir.display(),
        doctor_checklist(report_names)
    )
}

pub(super) fn doctor_bundle_navigation_summary(bundle_dir: &std::path::Path) -> String {
    format!(
        "bundle={} | local-doctor.txt | bundle-overview.txt | runtime-timeline.md | runtime-tasks.md | support-handoff.md",
        bundle_dir.display()
    )
}

pub(super) fn doctor_copy_paste_summary(
    bundle_dir: &std::path::Path,
    copied_files: &[std::path::PathBuf],
) -> String {
    format!(
        "Doctor bundle exported to: {}\n  Files: {}",
        bundle_dir.display(),
        copied_files
            .iter()
            .filter_map(|path| path.file_name().and_then(|name| name.to_str()))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::{
        artifact_freshness_summary, doctor_checklist, doctor_copy_paste_summary,
        doctor_severity_summary, render_support_bundle_overview, support_handoff_template,
    };

    #[test]
    fn severity_summary_counts_status_prefixes() {
        let summary = doctor_severity_summary("  [ok] a\n  [--] b\n  [!!] c\n");
        assert_eq!(summary, "ok=1 warn=1 err=1");
    }

    #[test]
    fn checklist_and_handoff_render_report_names() {
        let checklist = doctor_checklist(&["local-doctor.txt", "remote-env.txt"]);
        assert!(checklist.contains("local-doctor.txt"));
        let handoff = support_handoff_template(std::path::Path::new("/tmp/bundle"), &["local-doctor.txt"]);
        assert!(handoff.contains("Support Handoff"));
        assert!(handoff.contains("runtime-tasks.md"));
    }

    #[test]
    fn overview_and_copy_paste_summary_list_files() {
        let dir = std::env::temp_dir().join(format!("yode-support-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("local-doctor.txt");
        std::fs::write(&file, "x").unwrap();
        let freshness = artifact_freshness_summary(std::slice::from_ref(&file));
        assert!(freshness.contains("local-doctor.txt"));
        let overview = render_support_bundle_overview(
            &dir,
            &dir,
            std::slice::from_ref(&file),
            &["local: ok=1 warn=0 err=0".to_string()],
            None,
            &[],
        );
        assert!(overview.contains("Support bundle overview"));
        let summary = doctor_copy_paste_summary(&dir, std::slice::from_ref(&file));
        assert!(summary.contains("local-doctor.txt"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
