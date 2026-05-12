use std::path::Path;

use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct InitCommand {
    meta: CommandMeta,
}

impl InitCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "init",
                description: "Create a project YODE.md instruction file",
                aliases: &[],
                args: vec![ArgDef {
                    name: "force".to_string(),
                    required: false,
                    hint: "[--force|force]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "--force".to_string(),
                        "force".to_string(),
                    ]),
                }],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for InitCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext<'_>) -> CommandResult {
        let force = parse_force_flag(args)?;
        let project_root = std::path::PathBuf::from(&ctx.session.working_dir);
        let target = project_root.join("YODE.md");
        if target.exists() && !force {
            return Err("YODE.md already exists. Use `/init --force` to replace it.".to_string());
        }

        let content = render_yode_md(&project_root);
        std::fs::write(&target, content)
            .map_err(|err| format!("Failed to write {}: {}", target.display(), err))?;
        Ok(CommandOutput::Message(format!(
            "Initialized project instructions at {}.",
            target.display()
        )))
    }
}

fn parse_force_flag(args: &str) -> Result<bool, String> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }
    if matches!(trimmed, "--force" | "force" | "-f") {
        return Ok(true);
    }
    Err("Usage: /init [--force]".to_string())
}

fn render_yode_md(project_root: &Path) -> String {
    let project_name = project_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    let project_kind = detect_project_kind(project_root);
    let test_commands = suggested_test_commands(project_root);

    format!(
        "# {project_name}\n\n## Project Overview\n\n- Type: {project_kind}\n- Goal: Maintain and improve this repository while preserving its existing architecture and user-facing behavior.\n\n## Development Commands\n\n{commands}\n\n## Working Guidelines\n\n- Prefer small, focused changes that match the existing architecture.\n- Read nearby code before introducing new abstractions.\n- Run the relevant validation command before committing when the change affects behavior.\n- Do not overwrite unrelated user changes.\n\n## Review Checklist\n\n- Behavior is covered by focused tests or a clear manual verification note.\n- User-facing text and command output are concise and actionable.\n- Long-running context should preserve files, plans, skills, and MCP instructions after compacting.\n",
        commands = test_commands
            .iter()
            .map(|command| format!("- `{}`", command))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn detect_project_kind(project_root: &Path) -> &'static str {
    if project_root.join("Cargo.toml").exists() {
        "Rust workspace or crate"
    } else if project_root.join("package.json").exists() {
        "JavaScript/TypeScript package"
    } else if project_root.join("pyproject.toml").exists() {
        "Python project"
    } else if project_root.join("go.mod").exists() {
        "Go module"
    } else {
        "General software project"
    }
}

fn suggested_test_commands(project_root: &Path) -> Vec<&'static str> {
    let mut commands = Vec::new();
    if project_root.join("Cargo.toml").exists() {
        commands.push("cargo check");
        commands.push("cargo test");
        commands.push("cargo fmt --check");
    }
    if project_root.join("package.json").exists() {
        if project_root.join("pnpm-lock.yaml").exists() {
            commands.push("pnpm test");
        } else if project_root.join("yarn.lock").exists() {
            commands.push("yarn test");
        } else {
            commands.push("npm test");
        }
    }
    if project_root.join("pyproject.toml").exists() {
        commands.push("pytest");
    }
    if project_root.join("go.mod").exists() {
        commands.push("go test ./...");
    }
    if commands.is_empty() {
        commands.push("Run the repository's documented validation command");
    }
    commands
}

#[cfg(test)]
mod tests {
    use super::{parse_force_flag, render_yode_md};

    #[test]
    fn init_template_detects_rust_project() {
        let dir = std::env::temp_dir().join(format!("yode-init-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "[workspace]\n").unwrap();

        let content = render_yode_md(&dir);

        assert!(content.contains("Rust workspace or crate"));
        assert!(content.contains("cargo check"));
        assert!(content.contains("cargo test"));
        assert!(content.contains("cargo fmt --check"));
        assert!(!content.contains("TODO"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn init_template_uses_actionable_fallback_for_unknown_projects() {
        let dir =
            std::env::temp_dir().join(format!("yode-init-generic-test-{}", uuid::Uuid::new_v4()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let content = render_yode_md(&dir);

        assert!(content.contains("General software project"));
        assert!(content.contains("Run the repository's documented validation command"));
        assert!(!content.contains("TODO"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn parse_force_flag_accepts_expected_variants() {
        assert!(!parse_force_flag("").unwrap());
        assert!(parse_force_flag("--force").unwrap());
        assert!(parse_force_flag("force").unwrap());
        assert!(parse_force_flag("-f").unwrap());
        assert!(parse_force_flag("bad").is_err());
    }
}
