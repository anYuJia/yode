mod app;
mod configuration;
mod mcp;
mod provider;
mod session;
mod settings;
mod terminal;
mod turn;
mod update;
mod worktree;

pub fn invoke_handler() -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app::app_get_bootstrap,
        session::sessions_list,
        session::sessions_create,
        session::sessions_messages,
        session::sessions_clear_messages,
        session::sessions_rename,
        session::sessions_export_markdown,
        session::sessions_compact_local,
        session::sessions_compact_engine,
        app::project_folder_pick,
        app::runtime_state_get,
        app::edit_diff_artifact_read,
        turn::turn_send_message,
        turn::permission_respond,
        turn::ask_user_respond,
        turn::turn_cancel,
        turn::permission_mode_set,
        app::general_settings_apply,
        app::open_target,
        app::import_ai_sessions,
        app::license_notices,
        configuration::configuration_state_get,
        configuration::configuration_update,
        configuration::configuration_open_file,
        configuration::workspace_diagnose,
        configuration::workspace_reinstall,
        configuration::desktop_setting_get,
        configuration::desktop_setting_set,
        configuration::personalization_state_get,
        configuration::personalization_reset_memories,
        mcp::mcp_servers_state,
        mcp::mcp_servers_save,
        mcp::mcp_server_test,
        mcp::mcp_servers_reload,
        settings::browser_clear_data,
        settings::browser_settings_get,
        settings::browser_settings_apply,
        settings::hooks_settings_get,
        settings::hooks_settings_apply,
        worktree::git_settings_get,
        worktree::git_settings_apply,
        worktree::git_current_branch,
        worktree::worktrees_list,
        worktree::worktrees_prune_idle,
        worktree::worktree_delete,
        settings::computer_use_open_accessibility,
        settings::computer_use_open_chrome,
        settings::computer_use_pick_application,
        settings::computer_use_settings_get,
        settings::computer_use_settings_apply,
        terminal::terminal_run,
        terminal::terminal_open,
        terminal::terminal_write,
        terminal::terminal_resize,
        terminal::terminal_close,
        session::sessions_delete,
        session::sessions_update_llm,
        provider::config_get_providers,
        provider::config_save_providers,
        provider::config_get_default_llm,
        provider::config_set_default_llm,
        provider::config_test_provider,
        update::check_for_updates,
        update::download_update,
        update::has_pending_update,
        update::apply_downloaded_update
    ]
}
