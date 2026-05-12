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
fn builtin_registry_includes_plan_command() {
    let mut reg = CommandRegistry::new();
    crate::commands::register_all(&mut reg);

    let plan = reg.find("plan").expect("/plan should be registered");
    assert_eq!(plan.meta().category, CommandCategory::Session);
    assert!(!plan.meta().hidden);
}

#[test]
fn builtin_registry_includes_skills_command() {
    let mut reg = CommandRegistry::new();
    crate::commands::register_all(&mut reg);

    let skills = reg.find("skills").expect("/skills should be registered");
    assert_eq!(skills.meta().category, CommandCategory::Tools);
    assert!(!skills.meta().hidden);
    assert!(reg.find("skill").is_some());
}

#[test]
fn builtin_registry_includes_init_command() {
    let mut reg = CommandRegistry::new();
    crate::commands::register_all(&mut reg);

    let init = reg.find("init").expect("/init should be registered");
    assert_eq!(init.meta().category, CommandCategory::Session);
    assert!(!init.meta().hidden);
}

#[test]
fn builtin_registry_includes_rewind_command() {
    let mut reg = CommandRegistry::new();
    crate::commands::register_all(&mut reg);

    let rewind = reg.find("rewind").expect("/rewind should be registered");
    assert_eq!(rewind.meta().category, CommandCategory::Session);
    assert!(!rewind.meta().hidden);
}

#[test]
fn builtin_registry_includes_resume_command() {
    let mut reg = CommandRegistry::new();
    crate::commands::register_all(&mut reg);

    let resume = reg.find("resume").expect("/resume should be registered");
    assert_eq!(resume.meta().category, CommandCategory::Session);
    assert!(!resume.meta().hidden);
}

#[test]
fn builtin_registry_includes_output_style_command() {
    let mut reg = CommandRegistry::new();
    crate::commands::register_all(&mut reg);

    let output_style = reg
        .find("output-style")
        .expect("/output-style should be registered");
    assert_eq!(output_style.meta().category, CommandCategory::Utility);
    assert!(!output_style.meta().hidden);
    assert!(reg.find("style").is_some());
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
    assert!(!cats.contains_key(&CommandCategory::Utility));
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
    assert!(names
        .iter()
        .any(|item| item.name == "model" && !item.is_alias));
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
