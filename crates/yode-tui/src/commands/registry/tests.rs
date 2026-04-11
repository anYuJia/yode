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
fn test_complete_command_exact_match_beats_prefix_neighbor() {
    let mut reg = CommandRegistry::new();
    reg.register(Box::new(DummyCommand::new(
        "review",
        "Review changes",
        &["rev"],
        CommandCategory::Development,
    )));
    reg.register(Box::new(DummyCommand::new(
        "reviews",
        "Review artifacts",
        &[],
        CommandCategory::Development,
    )));

    let results = reg.complete_command("review");
    assert_eq!(results[0].name, "review");
}

#[test]
fn test_boundary_match_prefers_segment_start() {
    assert!(matching::is_boundary_match("theme-pack", "pack"));
    assert!(!matching::is_boundary_match("themepack", "pack"));
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
    assert!(cats.get(&CommandCategory::Utility).is_none());
}

#[test]
fn test_visible_command_names_include_aliases_but_skip_hidden() {
    let mut reg = CommandRegistry::new();
    reg.register(Box::new(DummyCommand::new(
        "model",
        "Switch model",
        &["m"],
        CommandCategory::Model,
    )));
    reg.register(Box::new(DummyCommand::hidden("debug")));

    let names = reg.visible_command_names();
    assert!(names.iter().any(|item| item.name == "model" && !item.is_alias));
    assert!(names.iter().any(|item| item.name == "m" && item.is_alias));
    assert!(!names.iter().any(|item| item.name == "debug"));
}

#[test]
fn test_levenshtein() {
    assert_eq!(matching::levenshtein("", ""), 0);
    assert_eq!(matching::levenshtein("abc", "abc"), 0);
    assert_eq!(matching::levenshtein("kitten", "sitting"), 3);
    assert_eq!(matching::levenshtein("", "hello"), 5);
    assert_eq!(matching::levenshtein("hello", ""), 5);
}
