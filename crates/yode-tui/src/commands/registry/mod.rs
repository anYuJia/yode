mod matching;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, HashMap};

use super::context::{CommandContext, CompletionContext};
use super::{ArgCompletionSource, Command, CommandCategory, CommandResult};

use self::matching::{is_boundary_match, levenshtein};

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
            .map(|command| command.as_ref())
            .filter(|command| !command.meta().hidden)
            .collect()
    }

    pub fn by_category(&self) -> BTreeMap<CommandCategory, Vec<&dyn Command>> {
        let mut map = BTreeMap::new();
        for command in &self.commands {
            if command.meta().hidden {
                continue;
            }
            map.entry(command.meta().category)
                .or_insert_with(Vec::new)
                .push(command.as_ref());
        }
        map
    }

    /// Command name completion: prefix match, then substring fallback.
    pub fn complete_command(&self, prefix: &str) -> Vec<CommandSuggestion> {
        let prefix_lower = prefix.to_lowercase();
        let mut scored_results: Vec<(usize, CommandSuggestion)> = Vec::new();

        if prefix_lower.is_empty() {
            for command in &self.commands {
                let meta = command.meta();
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
            for command in &self.commands {
                let meta = command.meta();
                if meta.hidden {
                    continue;
                }
                let name_lower = meta.name.to_lowercase();
                if name_lower == prefix_lower {
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

                if name_lower.starts_with(&prefix_lower) {
                    scored_results.push((
                        1,
                        CommandSuggestion {
                            name: meta.name.to_string(),
                            description: meta.description.to_string(),
                            is_alias: false,
                        },
                    ));
                    continue;
                }

                let mut alias_matched = false;
                for alias in meta.aliases {
                    let alias_lower = alias.to_lowercase();
                    if alias_lower == prefix_lower {
                        scored_results.push((
                            2,
                            CommandSuggestion {
                                name: alias.to_string(),
                                description: meta.description.to_string(),
                                is_alias: true,
                            },
                        ));
                        alias_matched = true;
                        break;
                    }
                    if alias_lower.starts_with(&prefix_lower) {
                        scored_results.push((
                            3,
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

                if is_boundary_match(&name_lower, &prefix_lower) {
                    scored_results.push((
                        4,
                        CommandSuggestion {
                            name: meta.name.to_string(),
                            description: meta.description.to_string(),
                            is_alias: false,
                        },
                    ));
                    continue;
                }

                if name_lower.contains(&prefix_lower) {
                    scored_results.push((
                        5,
                        CommandSuggestion {
                            name: meta.name.to_string(),
                            description: meta.description.to_string(),
                            is_alias: false,
                        },
                    ));
                    continue;
                }

                if prefix_lower.len() >= 2 {
                    let distance = levenshtein(&prefix_lower, &name_lower);
                    if distance <= 2 {
                        scored_results.push((
                            6 + distance,
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

        scored_results.sort_by_key(|(score, suggestion)| (*score, suggestion.name.len()));
        scored_results
            .into_iter()
            .map(|(_, suggestion)| suggestion)
            .collect()
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
            Some(command) => command,
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
            ArgCompletionSource::Static(values) => values.clone(),
            ArgCompletionSource::Dynamic(callback) => callback(ctx),
        };

        if partial.is_empty() {
            all_values
        } else {
            let partial_lower = partial.to_lowercase();
            all_values
                .into_iter()
                .filter(|value| value.to_lowercase().starts_with(&partial_lower))
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
                .map(|arg| arg.hint.as_str())
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
        for command in &self.commands {
            let name = command.meta().name;
            let distance = levenshtein(typo, name);
            let threshold = name.len() / 2 + 1;
            if distance <= threshold
                && (best.is_none() || distance < best.as_ref().unwrap().0)
            {
                best = Some((distance, name.to_string()));
            }
        }
        best.map(|(_, name)| name)
    }
}
