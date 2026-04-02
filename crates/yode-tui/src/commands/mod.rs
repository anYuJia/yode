pub mod context;
pub mod registry;

pub mod session;
pub mod model;
pub mod tools;
pub mod info;
pub mod dev;
pub mod utility;

use context::{CommandContext, CompletionContext};

/// How a command argument gets its completions.
pub enum ArgCompletionSource {
    None,
    Static(Vec<String>),
    Dynamic(fn(&CompletionContext) -> Vec<String>),
}

pub struct ArgDef {
    pub name: String,
    pub required: bool,
    pub hint: String,
    pub completions: ArgCompletionSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CommandCategory {
    Session,
    Model,
    Tools,
    Info,
    Development,
    Utility,
}

impl CommandCategory {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Session => "Session",
            Self::Model => "Model & Provider",
            Self::Tools => "Tools",
            Self::Info => "Information",
            Self::Development => "Development",
            Self::Utility => "Utility",
        }
    }
}

pub struct CommandMeta {
    pub name: &'static str,
    pub description: &'static str,
    pub aliases: &'static [&'static str],
    pub args: Vec<ArgDef>,
    pub category: CommandCategory,
    pub hidden: bool,
}

pub enum CommandOutput {
    Message(String),
    Messages(Vec<String>),
    Silent,
}

pub type CommandResult = Result<CommandOutput, String>;

/// The trait every slash command implements. Synchronous (not async).
pub trait Command: Send + Sync {
    fn meta(&self) -> &CommandMeta;
    fn execute(&self, args: &str, ctx: &mut CommandContext) -> CommandResult;
}

use registry::CommandRegistry;

/// Register all built-in commands. Body commented out until command types exist.
pub fn register_all(_registry: &mut CommandRegistry) {
    // Will be filled in as commands are created in Tasks 4-9
}
