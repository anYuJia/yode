use std::path::{Path, PathBuf};

use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};
use crate::display_text::compact_path_tail;
use yode_core::engine::EngineRuntimeState;

pub struct FilesCommand {
    meta: CommandMeta,
}

impl FilesCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "files",
                description: "Show files in context and restore sources",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Info,
                hidden: false,
            },
        }
    }
}

impl Command for FilesCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let project_root = PathBuf::from(&ctx.session.working_dir);
        let runtime = ctx
            .engine
            .try_lock()
            .ok()
            .map(|engine| engine.runtime_state());
        Ok(CommandOutput::Message(render_files_view(
            &FilesView::from_runtime(runtime.as_ref(), &project_root),
        )))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RestoreSourceView {
    label: String,
    path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FilesView {
    runtime_available: bool,
    current_context_files: Vec<String>,
    restore_sources: Vec<RestoreSourceView>,
}

impl FilesView {
    fn from_runtime(runtime: Option<&EngineRuntimeState>, project_root: &Path) -> Self {
        let mut restore_sources = Vec::new();
        match runtime {
            Some(state) => {
                add_source(
                    &mut restore_sources,
                    "live memory",
                    Some(state.live_session_memory_path.as_str()),
                );
                add_source(
                    &mut restore_sources,
                    "latest memory update",
                    state.last_session_memory_update_path.as_deref(),
                );
                add_source(
                    &mut restore_sources,
                    "compact memory",
                    state.last_compaction_session_memory_path.as_deref(),
                );
                add_source(
                    &mut restore_sources,
                    "compact transcript",
                    state.last_compaction_transcript_path.as_deref(),
                );
                if let Some(boundary) = &state.last_compact_boundary {
                    for path in &boundary.artifact_paths {
                        add_source(
                            &mut restore_sources,
                            "compact boundary artifact",
                            Some(path),
                        );
                    }
                }
                Self {
                    runtime_available: true,
                    current_context_files: state.read_file_history.clone(),
                    restore_sources,
                }
            }
            None => {
                let live = yode_core::session_memory::live_session_memory_path(project_root);
                let session = yode_core::session_memory::session_memory_path(project_root);
                add_source(&mut restore_sources, "live memory", live.to_str());
                add_source(&mut restore_sources, "compact memory", session.to_str());
                Self {
                    runtime_available: false,
                    current_context_files: Vec::new(),
                    restore_sources,
                }
            }
        }
    }
}

fn render_files_view(view: &FilesView) -> String {
    let mut lines = vec!["Files in context:".to_string()];
    if !view.runtime_available {
        lines.push("  Runtime: engine busy; current read history is unavailable.".to_string());
    }
    if view.current_context_files.is_empty() {
        lines.push("  Current context: none recorded yet".to_string());
    } else {
        lines.push(format!(
            "  Current context: {} recent read file(s)",
            view.current_context_files.len()
        ));
        for (idx, file) in view.current_context_files.iter().enumerate() {
            lines.push(format!("    {}. {}", idx + 1, file));
        }
    }

    lines.push(String::new());
    lines.push("Restore sources:".to_string());
    if view.restore_sources.is_empty() {
        lines.push("  none".to_string());
    } else {
        for source in &view.restore_sources {
            lines.push(format!(
                "  - {}: {} ({})",
                source.label,
                compact_path_tail(&source.path),
                path_status(&source.path)
            ));
        }
    }

    lines.push(String::new());
    lines.push("Recovery: use `read_file` to re-open exact file content, or `/memory latest` for the latest compact transcript.".to_string());
    lines.join("\n")
}

fn add_source(sources: &mut Vec<RestoreSourceView>, label: &str, path: Option<&str>) {
    let Some(path) = path.map(str::trim).filter(|path| !path.is_empty()) else {
        return;
    };
    if sources.iter().any(|source| source.path == path) {
        return;
    }
    sources.push(RestoreSourceView {
        label: label.to_string(),
        path: path.to_string(),
    });
}

fn path_status(path: &str) -> &'static str {
    if Path::new(path).exists() {
        "available"
    } else {
        "not written yet"
    }
}

#[cfg(test)]
mod tests {
    use super::{render_files_view, FilesView, RestoreSourceView};

    #[test]
    fn render_files_view_lists_recent_files_and_restore_sources() {
        let view = FilesView {
            runtime_available: true,
            current_context_files: vec![
                "src/lib.rs (120 lines)".to_string(),
                "README.md (40 lines)".to_string(),
            ],
            restore_sources: vec![RestoreSourceView {
                label: "compact transcript".to_string(),
                path: "/tmp/yode/.yode/transcripts/latest.md".to_string(),
            }],
        };
        let rendered = render_files_view(&view);
        assert!(rendered.contains("2 recent read file(s)"));
        assert!(rendered.contains("src/lib.rs (120 lines)"));
        assert!(rendered.contains("compact transcript"));
        assert!(rendered.contains("/memory latest"));
    }

    #[test]
    fn render_files_view_reports_busy_runtime() {
        let view = FilesView {
            runtime_available: false,
            current_context_files: Vec::new(),
            restore_sources: Vec::new(),
        };
        let rendered = render_files_view(&view);
        assert!(rendered.contains("engine busy"));
        assert!(rendered.contains("Current context: none recorded yet"));
    }
}
