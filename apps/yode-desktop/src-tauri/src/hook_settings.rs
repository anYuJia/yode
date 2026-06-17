use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;
use yode_core::hooks::{HookDefinition, HookManager};

use crate::desktop_settings_store::{desktop_bool_setting, read_desktop_settings};
use crate::protocol::{DesktopHookEntry, HooksSettings};

pub(super) fn hooks_settings_from_desktop_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> Result<HooksSettings> {
    Ok(normalize_hooks_settings(HooksSettings {
        enabled: desktop_bool_setting(settings, "yode-hooks-enabled", true),
        hooks: desktop_hook_list_setting(settings, "yode-hooks-list"),
    }))
}

pub(super) fn normalize_hooks_settings(mut settings: HooksSettings) -> HooksSettings {
    settings.hooks = settings
        .hooks
        .into_iter()
        .map(normalize_desktop_hook_entry)
        .collect();
    settings
}

pub(super) fn validate_hooks_settings(settings: &HooksSettings) -> Result<()> {
    for hook in &settings.hooks {
        if hook.name.trim().is_empty() {
            anyhow::bail!("钩子名称不能为空。");
        }
        if hook.command.trim().is_empty() {
            anyhow::bail!("钩子指令不能为空。");
        }
        if hook.events.is_empty() {
            anyhow::bail!("钩子至少需要一个触发事件。");
        }
        if hook.timeout_secs == 0 {
            anyhow::bail!("钩子超时时间必须大于 0。");
        }
    }
    Ok(())
}

pub(super) fn build_desktop_hook_manager(workspace_path: &Path) -> Result<Option<HookManager>> {
    let settings = read_desktop_settings()?;
    let hooks_settings = hooks_settings_from_desktop_settings(&settings)?;
    if !hooks_settings.enabled {
        return Ok(None);
    }

    let mut manager = HookManager::new(workspace_path.to_path_buf());
    for hook in hooks_settings.hooks {
        if hook.disabled {
            continue;
        }
        manager.register(HookDefinition {
            command: hook.command,
            events: hook.events,
            tool_filter: hook.tool_filter,
            timeout_secs: hook.timeout_secs,
            can_block: hook.can_block,
        });
    }
    Ok(Some(manager))
}

fn normalize_desktop_hook_entry(mut hook: DesktopHookEntry) -> DesktopHookEntry {
    hook.name = hook.name.trim().to_string();
    hook.command = hook.command.trim().to_string();
    hook.events = hook
        .events
        .into_iter()
        .map(|event| event.trim().to_string())
        .filter(|event| !event.is_empty())
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    hook.events.retain(|event| seen.insert(event.clone()));
    hook.timeout_secs = hook.timeout_secs.max(1);
    hook.tool_filter = hook.tool_filter.and_then(|tools| {
        let filtered = tools
            .into_iter()
            .map(|tool| tool.trim().to_string())
            .filter(|tool| !tool.is_empty())
            .collect::<Vec<_>>();
        if filtered.is_empty() {
            None
        } else {
            Some(filtered)
        }
    });
    hook
}

fn desktop_hook_list_setting(
    settings: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Vec<DesktopHookEntry> {
    settings
        .get(key)
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| serde_json::from_value::<DesktopHookEntry>(item.clone()).ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(name: &str, command: &str, events: Vec<&str>) -> DesktopHookEntry {
        DesktopHookEntry {
            name: name.to_string(),
            command: command.to_string(),
            events: events.into_iter().map(str::to_string).collect(),
            tool_filter: Some(vec![" bash ".to_string(), "".to_string()]),
            timeout_secs: 0,
            can_block: true,
            disabled: false,
        }
    }

    #[test]
    fn hook_settings_normalize_entries() {
        let settings = normalize_hooks_settings(HooksSettings {
            enabled: true,
            hooks: vec![hook(
                " test ",
                " echo hi ",
                vec!["pre_tool", "pre_tool", " "],
            )],
        });
        let hook = &settings.hooks[0];

        assert_eq!(hook.name, "test");
        assert_eq!(hook.command, "echo hi");
        assert_eq!(hook.events, vec!["pre_tool"]);
        assert_eq!(hook.tool_filter.as_deref(), Some(&["bash".to_string()][..]));
        assert_eq!(hook.timeout_secs, 1);
    }

    #[test]
    fn hook_settings_validation_rejects_missing_command() {
        let settings = HooksSettings {
            enabled: true,
            hooks: vec![hook("test", " ", vec!["pre_tool"])],
        };

        assert!(validate_hooks_settings(&settings).is_err());
    }
}
