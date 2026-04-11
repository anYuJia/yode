mod event_loop;
mod startup;

use std::collections::HashMap;
use std::io;
use std::sync::Arc;

use anyhow::Result;
use crossterm::event::{DisableBracketedPaste, EnableBracketedPaste};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use yode_core::context::AgentContext;
use yode_core::db::Database;
use yode_core::permission::PermissionManager;
use yode_llm::provider::LlmProvider;
use yode_llm::registry::ProviderRegistry;
use yode_llm::types::Message;
use yode_tools::registry::ToolRegistry;

use super::lifecycle::print_exit_summary;

/// Run the TUI application.
pub async fn run(
    provider: Arc<dyn LlmProvider>,
    provider_registry: Arc<ProviderRegistry>,
    tools: Arc<ToolRegistry>,
    permissions: PermissionManager,
    context: AgentContext,
    db: Database,
    restored_messages: Option<Vec<Message>>,
    skill_commands: Vec<(String, String)>,
    all_provider_models: HashMap<String, Vec<String>>,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnableBracketedPaste)?;
    stdout.execute(crossterm::style::Print("\n"))?;

    let mut startup = startup::prepare_runtime(
        provider,
        provider_registry,
        Arc::clone(&tools),
        permissions,
        context,
        db,
        restored_messages,
        skill_commands,
        all_provider_models,
    )
    .await?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::with_options(
        backend,
        ratatui::TerminalOptions {
            viewport: ratatui::Viewport::Inline(4),
        },
    )?;

    let result = event_loop::run_app(
        &mut terminal,
        &mut startup.app,
        startup.engine,
        tools,
        startup.engine_event_tx,
        &mut startup.engine_event_rx,
    )
    .await;

    terminal.clear()?;
    disable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(DisableBracketedPaste)?;

    let area = terminal.get_frame().area();
    crossterm::execute!(stdout, crossterm::cursor::MoveTo(0, area.bottom()))?;
    println!();

    print_exit_summary(&startup.app);

    if let Err(error) = &result {
        eprintln!("Yode error: {:#}", error);
    }
    result
}
