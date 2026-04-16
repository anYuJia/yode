use crate::app::PermissionMode;
use super::responsive::Density;

pub fn permission_mode_badge(mode: PermissionMode, density: Density) -> (String, ratatui::style::Color) {
    let label = mode.label();
    let (icon, color) = match mode {
        PermissionMode::Normal => ("●", ratatui::style::Color::LightGreen),
        PermissionMode::AutoAccept => ("⚡", ratatui::style::Color::Yellow),
        PermissionMode::Plan => ("📋", ratatui::style::Color::LightBlue),
    };
    let text = match density {
        Density::Wide | Density::Medium => format!("{} {} ", icon, label.to_lowercase()),
        Density::Narrow => format!("{}{} ", icon, label.chars().next().unwrap_or('m')),
    };
    (text, color)
}

pub fn queue_badge_label(count: usize, density: Density) -> String {
    match density {
        Density::Wide => format!("{} queued ", count),
        Density::Medium | Density::Narrow => format!("q{} ", count),
    }
}

pub fn task_badge_label(count: usize, density: Density) -> String {
    match density {
        Density::Wide => format!("{} jobs ", count),
        Density::Medium => format!("j{} ", count),
        Density::Narrow => format!("{}j ", count),
    }
}

pub fn runtime_family_badge(label: &str, density: Density) -> String {
    match density {
        Density::Wide => format!("{} ", label),
        Density::Medium | Density::Narrow => {
            let compact = label.chars().take(4).collect::<String>();
            format!("{} ", compact)
        }
    }
}

pub fn budget_badge_label(turn_tool_count: u32, density: Density) -> Option<String> {
    if turn_tool_count >= 25 {
        return Some(match density {
            Density::Wide => "budget warning ".to_string(),
            Density::Medium => "budget! ".to_string(),
            Density::Narrow => "!b ".to_string(),
        });
    }
    if turn_tool_count >= 15 {
        return Some(match density {
            Density::Wide => "budget notice ".to_string(),
            Density::Medium => "budget ".to_string(),
            Density::Narrow => "b ".to_string(),
        });
    }
    None
}
