use chrono::{Local, NaiveDateTime};
use super::preview::truncate_preview_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemoryDocumentView {
    pub entries: Vec<MemoryEntryView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemoryEntryView {
    pub timestamp: Option<String>,
    pub session_id: Option<String>,
    pub sections: Vec<MemorySectionView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemorySectionView {
    pub title: String,
    pub items: Vec<String>,
}

pub(super) fn parse_memory_document(content: &str) -> Option<MemoryDocumentView> {
    let mut entries = Vec::new();
    let mut current_entry: Option<MemoryEntryView> = None;
    let mut current_section: Option<MemorySectionView> = None;

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            if rest.contains(" session ") {
                flush_memory_section(&mut current_entry, &mut current_section);
                if let Some(entry) = current_entry.take() {
                    entries.push(entry);
                }
                current_entry = Some(parse_memory_entry_header(rest));
            }
            continue;
        }

        let Some(entry) = current_entry.as_mut() else {
            continue;
        };

        if let Some(title) = line.strip_prefix("### ") {
            flush_memory_section(&mut current_entry, &mut current_section);
            current_section = Some(MemorySectionView {
                title: title.trim().to_string(),
                items: Vec::new(),
            });
            continue;
        }

        if let Some(section) = current_section.as_mut() {
            if let Some(item) = line.strip_prefix("- ") {
                section.items.push(item.trim().to_string());
            } else if !line.trim().is_empty()
                && section.title == "Session Stats"
                && !line.starts_with("```")
            {
                section.items.push(line.trim().to_string());
            }
        } else if !line.trim().is_empty() && !line.starts_with('#') {
            let _ = entry;
        }
    }

    flush_memory_section(&mut current_entry, &mut current_section);
    if let Some(entry) = current_entry.take() {
        entries.push(entry);
    }

    if entries.is_empty() || !entries.iter().any(|entry| !entry.sections.is_empty()) {
        None
    } else {
        Some(MemoryDocumentView { entries })
    }
}

pub(super) fn memory_entry_age(timestamp: Option<&str>) -> String {
    let Some(timestamp) = timestamp else {
        return "unknown".to_string();
    };
    let Some(dt) = NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S").ok() else {
        return timestamp.to_string();
    };

    let now = Local::now().naive_local();
    let delta = now - dt;
    let hours = delta.num_hours();
    if hours < 0 {
        return "from the future".to_string();
    }
    if hours < 1 {
        return "less than 1 hour old".to_string();
    }
    if hours < 24 {
        return format!("{} hours old", hours);
    }

    let days = delta.num_days();
    if days == 1 {
        "1 day old".to_string()
    } else {
        format!("{} days old", days)
    }
}

pub(super) fn format_section_items_preview(items: &[String], max_chars: usize) -> String {
    if items.is_empty() {
        return "none".to_string();
    }
    truncate_preview_text(&items.join(" | "), max_chars)
}

fn parse_memory_entry_header(header: &str) -> MemoryEntryView {
    let (timestamp, session_id) =
        if let Some((timestamp, session_id)) = header.split_once(" session ") {
            (
                Some(timestamp.trim().to_string()),
                Some(session_id.trim().to_string()),
            )
        } else {
            (Some(header.trim().to_string()), None)
        };

    MemoryEntryView {
        timestamp,
        session_id,
        sections: Vec::new(),
    }
}

fn flush_memory_section(
    entry: &mut Option<MemoryEntryView>,
    section: &mut Option<MemorySectionView>,
) {
    if let (Some(entry), Some(section)) = (entry.as_mut(), section.take()) {
        entry.sections.push(section);
    }
}
