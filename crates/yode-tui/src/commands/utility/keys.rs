use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct KeysCommand {
    meta: CommandMeta,
}

impl KeysCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "keys",
                description: "Inspect keybindings and config paths",
                aliases: &["keybindings"],
                args: vec![],
                category: CommandCategory::Utility,
                hidden: false,
            },
        }
    }
}

impl Command for KeysCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, ctx: &mut CommandContext) -> CommandResult {
        let home = dirs::home_dir();
        Ok(CommandOutput::Message(render_keybindings_reference(
            &ctx.session.working_dir,
            home.as_deref(),
        )))
    }
}

fn render_keybindings_reference(working_dir: &str, home_dir: Option<&std::path::Path>) -> String {
    let user_config = home_dir
        .map(|home| home.join(".yode").join("config.toml"))
        .unwrap_or_else(|| std::path::PathBuf::from("~/.yode/config.toml"));
    let project_config = std::path::Path::new(working_dir)
        .join(".yode")
        .join("config.toml");
    let local_config = std::path::Path::new(working_dir)
        .join(".yode")
        .join("config.local.toml");

    format!(
        "Keybindings:\n  Command:        /keybindings (/keys)\n  Keymap source:  built-in defaults\n  User config:    {} ({})\n  Project config: {} ({})\n  Local config:   {} ({})\n\n{}",
        user_config.display(),
        path_status(&user_config),
        project_config.display(),
        path_status(&project_config),
        local_config.display(),
        path_status(&local_config),
        concat!(
            "Keyboard shortcuts:\n",
            "\n",
            "  Editing:\n",
            "    Ctrl+A / Home   — Move to line start\n",
            "    Ctrl+E / End    — Move to line end\n",
            "    Ctrl+U          — Clear entire line\n",
            "    Ctrl+K          — Delete to end of line\n",
            "    Ctrl+W          — Delete previous word\n",
            "    Ctrl+J          — Insert newline\n",
            "    Shift+Enter     — Insert newline\n",
            "    Tab             — Autocomplete\n",
            "    Shift+Tab       — Reverse autocomplete\n",
            "\n",
            "  Navigation:\n",
            "    Up/Down         — Browse history (single-line) or navigate (multi-line)\n",
            "    Ctrl+R          — Reverse search history\n",
            "    PageUp/PageDown — Review chat history in the scroll view\n",
            "    Ctrl+End        — Return to the live bottom composer\n",
            "\n",
            "  Session:\n",
            "    Esc / Ctrl+C    — Stop generation\n",
            "    Ctrl+L          — Clear screen\n",
            "    Shift+Tab       — Cycle permission mode: Default → Auto → Plan\n",
            "\n",
            "  Inspector:\n",
            "    Tab             — Next panel\n",
            "    Shift+Tab       — Move focus between body/actions/tabs\n",
            "    PageUp/PageDown — Move through inspector body\n",
            "    Ctrl+Enter      — Run selected action\n",
            "\n",
            "  Special input:\n",
            "    !command        — Execute shell command directly\n",
            "    @file           — Attach file as context\n",
            "    /command        — Slash commands\n",
        )
    )
}

fn path_status(path: &std::path::Path) -> &'static str {
    if path.exists() {
        "available"
    } else {
        "not found"
    }
}

#[cfg(test)]
mod tests {
    use super::render_keybindings_reference;

    #[test]
    fn keybindings_reference_shows_config_paths() {
        let root = std::env::temp_dir().join(format!("yode-keybindings-{}", uuid::Uuid::new_v4()));
        let home = root.join("home");
        let work = root.join("work");
        std::fs::create_dir_all(work.join(".yode")).unwrap();
        std::fs::create_dir_all(home.join(".yode")).unwrap();
        std::fs::write(work.join(".yode").join("config.toml"), "theme = 'dark'").unwrap();

        let rendered = render_keybindings_reference(work.to_str().unwrap(), Some(&home));
        assert!(rendered.contains("Command:        /keybindings (/keys)"));
        assert!(rendered.contains("Keymap source:  built-in defaults"));
        assert!(rendered.contains("Project config:"));
        assert!(rendered.contains("available"));
        assert!(rendered.contains("Ctrl+A / Home"));
        assert!(rendered.contains("Review chat history in the scroll view"));
        assert!(rendered.contains("Default → Auto → Plan"));

        let _ = std::fs::remove_dir_all(root);
    }
}
