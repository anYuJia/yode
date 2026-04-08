use std::collections::{BTreeMap, HashMap};

use super::context::{CommandContext, CompletionContext};
use super::{ArgCompletionSource, Command, CommandCategory, CommandResult};

pub struct CommandSuggestion {
    pub name: String,
    pub description: String,
    pub is_alias: bool,
}

pub struct CommandRegistry {
    commands: Vec<Box<dyn Command>>,
    name_index: HashMap<String, usize>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            name_index: HashMap::new(),
        }
    }

    pub fn register(&mut self, cmd: Box<dyn Command>) {
        let idx = self.commands.len();
        let meta = cmd.meta();
        self.name_index.insert(meta.name.to_string(), idx);
        for alias in meta.aliases {
            self.name_index.insert(alias.to_string(), idx);
        }
        self.commands.push(cmd);
    }

    pub fn find(&self, name: &str) -> Option<&dyn Command> {
        self.name_index
            .get(name)
            .map(|&idx| self.commands[idx].as_ref())
    }

    pub fn visible_commands(&self) -> Vec<&dyn Command> {
        self.commands
            .iter()
            .map(|c| c.as_ref())
            .filter(|c| !c.meta().hidden)
            .collect()
    }

    pub fn by_category(&self) -> BTreeMap<CommandCategory, Vec<&dyn Command>> {
        let mut map = BTreeMap::new();
        for cmd in &self.commands {
            if cmd.meta().hidden {
                continue;
            }
            map.entry(cmd.meta().category)
                .or_insert_with(Vec::new)
                .push(cmd.as_ref());
        }
        map
    }

    /// Command name completion: prefix match, then substring fallback.
    pub fn complete_command(&self, prefix: &str) -> Vec<CommandSuggestion> {
        let prefix_lower = prefix.to_lowercase();
        let mut scored_results: Vec<(usize, CommandSuggestion)> = Vec::new();

        if prefix_lower.is_empty() {
            // If empty, just return all non-hidden commands sorted by name length
            for cmd in &self.commands {
                let meta = cmd.meta();
                if !meta.hidden {
                    scored_results.push((
                        0,
                        CommandSuggestion {
                            name: meta.name.to_string(),
                            description: meta.description.to_string(),
                            is_alias: false,
                        },
                    ));
                }
            }
        } else {
            for cmd in &self.commands {
                let meta = cmd.meta();
                if meta.hidden {
                    continue;
                }
                let name_lower = meta.name.to_lowercase();

                // Score 0: exact prefix
                if name_lower.starts_with(&prefix_lower) {
                    scored_results.push((
                        0,
                        CommandSuggestion {
                            name: meta.name.to_string(),
                            description: meta.description.to_string(),
                            is_alias: false,
                        },
                    ));
                    continue;
                }

                // Score 1: alias prefix
                let mut alias_matched = false;
                for alias in meta.aliases {
                    if alias.to_lowercase().starts_with(&prefix_lower) {
                        scored_results.push((
                            1,
                            CommandSuggestion {
                                name: alias.to_string(),
                                description: meta.description.to_string(),
                                is_alias: true,
                            },
                        ));
                        alias_matched = true;
                        break;
                    }
                }
                if alias_matched {
                    continue;
                }

                // Score 2: substring
                if name_lower.contains(&prefix_lower) {
                    scored_results.push((
                        2,
                        CommandSuggestion {
                            name: meta.name.to_string(),
                            description: meta.description.to_string(),
                            is_alias: false,
                        },
                    ));
                    continue;
                }

                // Score 3: fuzzy (levenshtein <= 2) if prefix is long enough
                if prefix_lower.len() >= 2 {
                    let dist = levenshtein(&prefix_lower, &name_lower);
                    if dist <= 2 {
                        scored_results.push((
                            3 + dist,
                            CommandSuggestion {
                                name: meta.name.to_string(),
                                description: meta.description.to_string(),
                                is_alias: false,
                            },
                        ));
                    }
                }
            }
        }

        // Sort by score first, then by name length to prefer shorter/exact commands
        scored_results.sort_by_key(|(score, s)| (*score, s.name.len()));
        scored_results.into_iter().map(|(_, s)| s).collect()
    }

    /// Argument completion: determine position from args_so_far, delegate to ArgDef.
    pub fn complete_args(
        &self,
        cmd_name: &str,
        args_so_far: &[&str],
        partial: &str,
        ctx: &CompletionContext,
    ) -> Vec<String> {
        let cmd = match self.find(cmd_name) {
            Some(c) => c,
            None => return Vec::new(),
        };
        let meta = cmd.meta();
        let arg_index = args_so_far.len();
        if arg_index >= meta.args.len() {
            return Vec::new();
        }

        let arg_def = &meta.args[arg_index];
        let all_values = match &arg_def.completions {
            ArgCompletionSource::None => return Vec::new(),
            ArgCompletionSource::Static(vals) => vals.clone(),
            ArgCompletionSource::Dynamic(f) => f(ctx),
        };

        if partial.is_empty() {
            all_values
        } else {
            let partial_lower = partial.to_lowercase();
            all_values
                .into_iter()
                .filter(|v| v.to_lowercase().starts_with(&partial_lower))
                .collect()
        }
    }

    /// Get args hint string for a command.
    pub fn args_hint(&self, cmd_name: &str) -> Option<String> {
        let cmd = self.find(cmd_name)?;
        let meta = cmd.meta();
        if meta.args.is_empty() {
            return None;
        }
        Some(
            meta.args
                .iter()
                .map(|a| a.hint.as_str())
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    /// Execute a command by name, returning None if the command is not found.
    pub fn execute_command(
        &self,
        name: &str,
        args: &str,
        ctx: &mut CommandContext,
    ) -> Option<CommandResult> {
        let cmd = self.find(name)?;
        Some(cmd.execute(args, ctx))
    }

    /// Edit-distance suggestion for typos.
    pub fn suggest_similar(&self, typo: &str) -> Option<String> {
        let mut best: Option<(usize, String)> = None;
        for cmd in &self.commands {
            let name = cmd.meta().name;
            let dist = levenshtein(typo, name);
            let threshold = name.len() / 2 + 1;
            if dist <= threshold {
                if best.is_none() || dist < best.as_ref().unwrap().0 {
                    best = Some((dist, name.to_string()));
                }
            }
        }
        best.map(|(_, name)| name)
    }
}

fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut dp = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() {
        dp[i][0] = i;
    }
    for j in 0..=b.len() {
        dp[0][j] = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[i][j] = (dp[i - 1][j] + 1)
                .min(dp[i][j - 1] + 1)
                .min(dp[i - 1][j - 1] + cost);
        }
    }
    dp[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::context::CommandContext;
    use crate::commands::{Command, CommandCategory, CommandMeta, CommandOutput, CommandResult};

    struct DummyCommand {
        meta: CommandMeta,
    }

    impl DummyCommand {
        fn new(
            name: &'static str,
            description: &'static str,
            aliases: &'static [&'static str],
            category: CommandCategory,
        ) -> Self {
            Self {
                meta: CommandMeta {
                    name,
                    description,
                    aliases,
                    args: Vec::new(),
                    category,
                    hidden: false,
                },
            }
        }

        fn hidden(name: &'static str) -> Self {
            Self {
                meta: CommandMeta {
                    name,
                    description: "",
                    aliases: &[],
                    args: Vec::new(),
                    category: CommandCategory::Utility,
                    hidden: true,
                },
            }
        }
    }

    impl Command for DummyCommand {
        fn meta(&self) -> &CommandMeta {
            &self.meta
        }

        fn execute(&self, _args: &str, _ctx: &mut CommandContext) -> CommandResult {
            Ok(CommandOutput::Silent)
        }
    }

    #[test]
    fn test_register_and_find() {
        let mut reg = CommandRegistry::new();
        reg.register(Box::new(DummyCommand::new(
            "model",
            "Switch model",
            &["m"],
            CommandCategory::Model,
        )));

        assert!(reg.find("model").is_some());
        assert!(reg.find("m").is_some());
        assert!(reg.find("unknown").is_none());
    }

    #[test]
    fn test_complete_command_prefix() {
        let mut reg = CommandRegistry::new();
        reg.register(Box::new(DummyCommand::new(
            "model",
            "Switch model",
            &[],
            CommandCategory::Model,
        )));
        reg.register(Box::new(DummyCommand::new(
            "memory",
            "Memory info",
            &[],
            CommandCategory::Info,
        )));

        let results = reg.complete_command("mo");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "model");
    }

    #[test]
    fn test_complete_command_substring_fallback() {
        let mut reg = CommandRegistry::new();
        reg.register(Box::new(DummyCommand::new(
            "compact",
            "Compact history",
            &[],
            CommandCategory::Utility,
        )));

        let results = reg.complete_command("pac");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "compact");
    }

    #[test]
    fn test_suggest_similar_typo() {
        let mut reg = CommandRegistry::new();
        reg.register(Box::new(DummyCommand::new(
            "model",
            "Switch model",
            &[],
            CommandCategory::Model,
        )));

        assert_eq!(reg.suggest_similar("modle"), Some("model".to_string()));
        assert_eq!(reg.suggest_similar("zzzzz"), None);
    }

    #[test]
    fn test_by_category() {
        let mut reg = CommandRegistry::new();
        reg.register(Box::new(DummyCommand::new(
            "model",
            "Switch model",
            &[],
            CommandCategory::Model,
        )));
        reg.register(Box::new(DummyCommand::new(
            "info",
            "Show info",
            &[],
            CommandCategory::Info,
        )));
        reg.register(Box::new(DummyCommand::new(
            "provider",
            "Switch provider",
            &[],
            CommandCategory::Model,
        )));
        reg.register(Box::new(DummyCommand::hidden("debug")));

        let cats = reg.by_category();
        assert_eq!(cats.get(&CommandCategory::Model).unwrap().len(), 2);
        assert_eq!(cats.get(&CommandCategory::Info).unwrap().len(), 1);
        assert!(cats.get(&CommandCategory::Utility).is_none()); // hidden command excluded
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("", "hello"), 5);
        assert_eq!(levenshtein("hello", ""), 5);
    }
}
