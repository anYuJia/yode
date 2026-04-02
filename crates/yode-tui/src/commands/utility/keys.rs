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
                description: "Show keyboard shortcut reference",
                aliases: &[],
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

    fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
        let keys = concat!(
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
            "    PageUp/PageDown — Scroll chat\n",
            "    Ctrl+End        — Scroll to bottom\n",
            "\n",
            "  Session:\n",
            "    Esc / Ctrl+C    — Stop generation\n",
            "    Ctrl+L          — Clear screen\n",
            "    Shift+Tab       — Cycle permission mode (when no popup)\n",
            "\n",
            "  Special input:\n",
            "    !command        — Execute shell command directly\n",
            "    @file           — Attach file as context\n",
            "    /command        — Slash commands\n",
        );
        Ok(CommandOutput::Message(keys.to_string()))
    }
}
