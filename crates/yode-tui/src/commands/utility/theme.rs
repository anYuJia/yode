use std::fs;
use std::path::PathBuf;

use crate::commands::context::CommandContext;
use crate::commands::{
    ArgCompletionSource, ArgDef, Command, CommandCategory, CommandMeta, CommandOutput,
    CommandResult,
};

pub struct ThemeCommand {
    meta: CommandMeta,
}

impl ThemeCommand {
    pub fn new() -> Self {
        Self {
            meta: CommandMeta {
                name: "theme",
                description: "Show or set the UI theme (dark/light)",
                aliases: &[],
                args: vec![ArgDef {
                    name: "theme".to_string(),
                    required: false,
                    hint: "[dark|light]".to_string(),
                    completions: ArgCompletionSource::Static(vec![
                        "dark".to_string(),
                        "light".to_string(),
                    ]),
                }],
                category: CommandCategory::Utility,
                hidden: false,
            },
        }
    }
}

impl Command for ThemeCommand {
    fn meta(&self) -> &CommandMeta {
        &self.meta
    }

    fn execute(&self, args: &str, _ctx: &mut CommandContext) -> CommandResult {
        let args = args.trim();

        if args.is_empty() {
            // Show current theme - read from config file
            let theme = read_current_theme().unwrap_or_else(|_| "dark".to_string());
            Ok(CommandOutput::Message(format!("Current theme: {}", theme)))
        } else {
            // Set theme
            let new_theme = args.to_lowercase();
            if new_theme != "dark" && new_theme != "light" {
                return Ok(CommandOutput::Message(
                    "Invalid theme. Valid values: dark, light".to_string(),
                ));
            }

            // Persist to config file
            if let Err(e) = persist_theme_to_config(&new_theme) {
                return Ok(CommandOutput::Message(format!(
                    "Failed to save theme: {}",
                    e
                )));
            }

            Ok(CommandOutput::Message(format!(
                "Theme set to: {}. Restart required for changes to take effect.",
                new_theme
            )))
        }
    }
}

/// Read current theme from config.toml
fn read_current_theme() -> Result<String, String> {
    let config_path = get_config_path()?;
    let content =
        fs::read_to_string(&config_path).map_err(|e| format!("Failed to read config: {}", e))?;

    // Simple parse: look for theme = "..." under [ui] section
    let mut in_ui_section = false;
    for line in content.lines() {
        let line = line.trim();
        if line == "[ui]" {
            in_ui_section = true;
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_ui_section = false;
            continue;
        }
        if in_ui_section && line.starts_with("theme") {
            if let Some(value) = line.split('"').nth(1) {
                return Ok(value.to_string());
            }
        }
    }

    Ok("dark".to_string())
}

/// Persist theme change to config.toml
fn persist_theme_to_config(theme: &str) -> Result<(), String> {
    let config_path = get_config_path()?;

    // Read existing config
    let content =
        fs::read_to_string(&config_path).map_err(|e| format!("Failed to read config: {}", e))?;

    // Check if [ui] section and theme key exist
    let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let mut in_ui_section = false;
    let mut theme_found = false;
    let mut ui_section_found = false;

    for i in 0..lines.len() {
        let line = lines[i].trim();
        if line == "[ui]" {
            in_ui_section = true;
            ui_section_found = true;
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_ui_section = false;
            continue;
        }
        if in_ui_section && line.starts_with("theme") {
            lines[i] = format!("theme = \"{}\"", theme);
            theme_found = true;
            break;
        }
    }

    // If theme not found but [ui] exists, add it under [ui]
    if !theme_found && ui_section_found {
        for i in 0..lines.len() {
            if lines[i].trim() == "[ui]" {
                // Find the end of [ui] section
                let mut insert_pos = i + 1;
                for j in (i + 1)..lines.len() {
                    let line = lines[j].trim();
                    if line.starts_with('[') && line.ends_with(']') {
                        break;
                    }
                    if line.is_empty() || line.starts_with('#') {
                        insert_pos = j + 1;
                        continue;
                    }
                    insert_pos = j + 1;
                }
                lines.insert(insert_pos, format!("theme = \"{}\"", theme));
                break;
            }
        }
    }

    // If no [ui] section, add it at the end
    if !ui_section_found {
        lines.push(String::new());
        lines.push(format!("[ui]"));
        lines.push(format!("theme = \"{}\"", theme));
    }

    // Write back
    let new_content = lines.join("\n");
    fs::write(&config_path, new_content).map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}

/// Get the config file path
fn get_config_path() -> Result<PathBuf, String> {
    let config_dir = dirs::config_dir()
        .ok_or("Cannot determine config directory")?
        .join("yode");
    Ok(config_dir.join("config.toml"))
}
