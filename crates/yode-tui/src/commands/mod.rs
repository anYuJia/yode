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

/// Register all built-in commands.
pub fn register_all(registry: &mut CommandRegistry) {
    // Session
    registry.register(Box::new(session::ClearCommand::new()));
    registry.register(Box::new(session::CompactCommand::new()));
    registry.register(Box::new(session::SessionsCommand::new()));
    registry.register(Box::new(session::ExitCommand::new()));
    // Model
    registry.register(Box::new(model::ModelCommand::new()));
    registry.register(Box::new(model::ProviderCommand::new()));
    registry.register(Box::new(model::ProvidersCommand::new()));
    registry.register(Box::new(model::EffortCommand::new()));
    // Tools
    registry.register(Box::new(tools::ToolsCommand::new()));
    registry.register(Box::new(tools::PermissionsCommand::new()));
    // Info
    registry.register(Box::new(info::HelpCommand::new()));
    registry.register(Box::new(info::StatusCommand::new()));
    registry.register(Box::new(info::CostCommand::new()));
    registry.register(Box::new(info::VersionCommand::new()));
    registry.register(Box::new(info::ConfigCommand::new()));
    registry.register(Box::new(info::ContextCommand::new()));
    registry.register(Box::new(info::DoctorCommand::new()));
    // Dev
    registry.register(Box::new(dev::DiffCommand::new()));
    registry.register(Box::new(dev::BugCommand::new()));
    // Utility
    registry.register(Box::new(utility::CopyCommand::new()));
    registry.register(Box::new(utility::KeysCommand::new()));
    registry.register(Box::new(utility::HistoryCommand::new()));
    registry.register(Box::new(utility::TimeCommand::new()));
}
