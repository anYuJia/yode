pub mod context;
pub mod registry;

pub mod dev;
pub mod info;
pub mod model;
pub mod session;
pub mod tools;
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
    /// Start an interactive wizard (multi-step input flow)
    StartWizard(crate::app::wizard::Wizard),
    /// Provider config changed — hot-reload this provider from disk
    ReloadProvider {
        name: String,
        messages: Vec<String>,
    },
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
    registry.register(Box::new(session::RenameCommand::new()));
    // Model
    registry.register(Box::new(model::ModelCommand::new()));
    registry.register(Box::new(model::ProviderCommand::new()));
    registry.register(Box::new(model::EffortCommand::new()));
    // Tools
    registry.register(Box::new(tools::ToolsCommand::new()));
    registry.register(Box::new(tools::McpCommand::new()));
    registry.register(Box::new(tools::PermissionsCommand::new()));
    registry.register(Box::new(tools::WorkflowsCommand::new()));
    // Info
    registry.register(Box::new(info::HelpCommand::new()));
    registry.register(Box::new(info::BriefCommand::new()));
    registry.register(Box::new(info::StatusCommand::new()));
    registry.register(Box::new(info::CostCommand::new()));
    registry.register(Box::new(info::VersionCommand::new()));
    registry.register(Box::new(info::ConfigCommand::new()));
    registry.register(Box::new(info::ContextCommand::new()));
    registry.register(Box::new(info::DiagnosticsCommand::new()));
    registry.register(Box::new(info::HooksCommand::new()));
    registry.register(Box::new(info::MemoryCommand::new()));
    registry.register(Box::new(info::TasksCommand::new()));
    registry.register(Box::new(info::DoctorCommand::new()));
    registry.register(Box::new(info::UpdateCommand::new()));
    // Dev
    registry.register(Box::new(dev::BenchCommand::new()));
    registry.register(Box::new(dev::DiffCommand::new()));
    registry.register(Box::new(dev::BugCommand::new()));
    registry.register(Box::new(dev::CoordinateCommand::new()));
    registry.register(Box::new(dev::ReviewCommand::new()));
    registry.register(Box::new(dev::ReviewsCommand::new()));
    registry.register(Box::new(dev::PipelineCommand::new()));
    registry.register(Box::new(dev::ShipCommand::new()));
    // Utility
    registry.register(Box::new(utility::CopyCommand::new()));
    registry.register(Box::new(utility::KeysCommand::new()));
    registry.register(Box::new(utility::HistoryCommand::new()));
    registry.register(Box::new(utility::TimeCommand::new()));
    registry.register(Box::new(utility::ThemeCommand::new()));
    registry.register(Box::new(utility::ExportCommand::new()));
}
