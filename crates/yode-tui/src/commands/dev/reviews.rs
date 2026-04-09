use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct ReviewsCommand {
    meta: CommandMeta,
}

impl ReviewsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "reviews",
                description: "List or open review artifacts under .yode/reviews",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Development,
                hidden: false,
            },
        }
    }
}

impl Command for ReviewsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult {
        let dir = std::path::PathBuf::from(&ctx.session.working_dir)
            .join(".yode")
            .join("reviews");
        let mut entries = std::fs::read_dir(&dir)
            .ok()
            .into_iter()
            .flat_map(|entries| entries.filter_map(Result::ok))
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
            .collect::<Vec<_>>();
        entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));

        let trimmed = args.trim();
        if trimmed.is_empty() {
            if entries.is_empty() {
                return Ok(CommandOutput::Message(format!(
                    "No review artifacts found in {}.",
                    dir.display()
                )));
            }
            let mut output = format!("Review artifacts in {}:\n", dir.display());
            for (idx, path) in entries.iter().take(12).enumerate() {
                output.push_str(&format!("  {:>2}. {}\n", idx + 1, path.display()));
            }
            output.push_str("\nUse /reviews <index> to open one.");
            return Ok(CommandOutput::Message(output));
        }

        let index = trimmed
            .parse::<usize>()
            .map_err(|_| "Usage: /reviews | /reviews <index>".to_string())?;
        if index == 0 || index > entries.len() {
            return Err(format!("Review artifact index out of range: {}", index));
        }
        let path = &entries[index - 1];
        let content = std::fs::read_to_string(path)
            .map_err(|err| format!("Failed to read {}: {}", path.display(), err))?;
        Ok(CommandOutput::Message(format!(
            "Review artifact {}\nPath: {}\n\n{}",
            index,
            path.display(),
            content
        )))
    }
}
