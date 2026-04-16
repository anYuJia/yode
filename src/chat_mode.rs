use std::sync::Arc;
use std::io::Write;

use crate::app_bootstrap::shutdown_mcp_clients;
use anyhow::Result;
use yode_core::config::Config;
use yode_core::context::AgentContext;
use yode_core::db::Database;
use yode_core::engine::{AgentEngine, ConfirmResponse, EngineEvent};
use yode_core::hooks::{HookDefinition, HookManager};
use yode_core::permission::PermissionManager;
use yode_llm::provider::LlmProvider;
use yode_llm::types::Message;
use yode_tools::registry::ToolRegistry;

pub(crate) async fn run_noninteractive_chat(
    chat_message: &str,
    provider: Arc<dyn LlmProvider>,
    tool_registry: Arc<ToolRegistry>,
    permissions: PermissionManager,
    context: AgentContext,
    db: Database,
    restored_messages: Option<Vec<Message>>,
    config: &Config,
    mcp_clients: Vec<yode_mcp::McpClient>,
) -> Result<()> {
    let mut engine = AgentEngine::new(provider, tool_registry, permissions, context);
    engine.set_database(db);

    if let Some(budget) = config.cost.max_budget_usd {
        if budget > 0.0 {
            engine.cost_tracker_mut().set_budget_limit(budget);
        }
    }

    if !config.hooks.hooks.is_empty() {
        let mut hook_manager = HookManager::new(std::env::current_dir().unwrap_or_default());
        for hook in &config.hooks.hooks {
            hook_manager.register(HookDefinition {
                command: hook.command.clone(),
                events: hook.events.clone(),
                tool_filter: hook.tool_filter.clone(),
                timeout_secs: hook.timeout_secs,
                can_block: hook.can_block,
            });
        }
        engine.set_hook_manager(hook_manager);
    }

    if let Some(messages) = restored_messages {
        engine.restore_messages(messages);
    }
    engine
        .initialize_session_hooks(if engine.context().is_resumed {
            "resume"
        } else {
            "startup"
        })
        .await;

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (confirm_tx, confirm_rx) = tokio::sync::mpsc::unbounded_channel();

    let chat_message = chat_message.to_string();
    let engine_handle = tokio::spawn(async move {
        let result = engine
            .run_turn_streaming(
                &chat_message,
                yode_core::context::QuerySource::User,
                event_tx,
                confirm_rx,
                None,
            )
            .await;
        engine.finalize_session_hooks("chat_exit").await;
        result
    });

    let mut full_text = String::new();
    let mut retry_line_active = false;
    let mut saw_final_error = false;
    while let Some(event) = event_rx.recv().await {
        if retry_line_active && !matches!(event, EngineEvent::Retrying { .. }) {
            eprintln!();
            retry_line_active = false;
        }
        match event {
            EngineEvent::TextDelta(delta) => {
                print!("{}", delta);
                full_text.push_str(&delta);
            }
            EngineEvent::TextComplete(_) => {}
            EngineEvent::ToolCallStart {
                name, arguments, ..
            } => {
                eprintln!(
                    "\x1b[90m⚡ {}({})\x1b[0m",
                    name,
                    truncate_str(&arguments, 80)
                );
            }
            EngineEvent::ToolConfirmRequired { id, name, .. } => {
                eprintln!("\x1b[33m🔑 自动确认工具: {}\x1b[0m", name);
                let _ = confirm_tx.send(ConfirmResponse::Allow);
                let _ = id;
            }
            EngineEvent::ToolResult { name, result, .. } => {
                if result.is_error {
                    eprintln!(
                        "\x1b[31m✗ {} 失败: {}\x1b[0m",
                        name,
                        truncate_str(&result.content, 200)
                    );
                } else {
                    eprintln!(
                        "\x1b[90m✓ {} 完成 ({} 字节)\x1b[0m",
                        name,
                        result.content.len()
                    );
                }
            }
            EngineEvent::Error(error) => {
                eprintln!("\x1b[31m错误: {}\x1b[0m", error);
                saw_final_error = true;
            }
            EngineEvent::Retrying {
                error_message,
                attempt,
                max_attempts,
                delay_secs,
            } => {
                eprint!(
                    "\r\x1b[33m↻ {} · retrying in {}s ({}/{})\x1b[0m",
                    error_message, delay_secs, attempt, max_attempts
                );
                let _ = std::io::stderr().flush();
                retry_line_active = true;
            }
            EngineEvent::SessionMemoryUpdated {
                path,
                generated_summary,
            } => {
                eprintln!(
                    "\x1b[90m🧠 Session memory updated ({}) -> {}\x1b[0m",
                    if generated_summary {
                        "summary"
                    } else {
                        "snapshot"
                    },
                    path
                );
            }
            EngineEvent::Done => break,
            _ => {}
        }
    }

    if retry_line_active {
        eprintln!();
    }

    if !full_text.is_empty() && !full_text.ends_with('\n') {
        println!();
    }

    if let Err(err) = engine_handle.await? {
        if saw_final_error {
            shutdown_mcp_clients(mcp_clients).await;
            return Ok(());
        }
        eprintln!("\x1b[31m引擎错误: {}\x1b[0m", err);
    }

    shutdown_mcp_clients(mcp_clients).await;
    Ok(())
}

fn truncate_str(input: &str, max: usize) -> String {
    let input = input.replace('\n', " ");
    if input.len() > max {
        format!("{}...", &input[..max])
    } else {
        input
    }
}
