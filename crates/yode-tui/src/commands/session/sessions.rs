use std::path::PathBuf;

use yode_core::db::Database;

use crate::commands::context::CommandContext;
use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

pub struct SessionsCommand {
    meta: CommandMeta,
}

impl SessionsCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "sessions",
                description: "List recent sessions",
                aliases: &[],
                args: vec![],
                category: CommandCategory::Session,
                hidden: false,
            },
        }
    }
}

impl Command for SessionsCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
        let db_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".yode")
            .join("sessions.db");
        match Database::open(&db_path) {
            Ok(db) => match db.list_sessions(10) {
                Ok(sessions) if sessions.is_empty() => Ok(CommandOutput::Message(
                    "No saved sessions found.".to_string(),
                )),
                Ok(sessions) => {
                    let mut lines = String::from("Recent sessions:\n");
                    for s in &sessions {
                        let id_short = &s.id[..s.id.len().min(8)];
                        let age = chrono::Utc::now().signed_duration_since(s.updated_at);
                        let age_str = if age.num_days() > 0 {
                            format!("{}d ago", age.num_days())
                        } else if age.num_hours() > 0 {
                            format!("{}h ago", age.num_hours())
                        } else {
                            format!("{}m ago", age.num_minutes().max(1))
                        };
                        let name = s.name.as_deref().unwrap_or("-");
                        lines.push_str(&format!(
                            "  {}  {:<12} {:<8} {}\n",
                            id_short, s.model, age_str, name
                        ));
                    }
                    lines.push_str("\nResume with: yode --resume <session-id>");
                    Ok(CommandOutput::Message(lines))
                }
                Err(e) => Err(format!("Failed to list sessions: {}", e)),
            },
            Err(e) => Err(format!("Failed to open session database: {}", e)),
        }
    }
}
