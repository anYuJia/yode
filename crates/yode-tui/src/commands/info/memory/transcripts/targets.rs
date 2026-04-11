use std::fs;
use std::path::{Path, PathBuf};

use super::*;

pub(in crate::commands::info::memory) fn resolve_transcript_target(
    dir: &Path,
    target: &str,
) -> Option<PathBuf> {
    let entries = sorted_transcript_entries(dir);
    if let Some(path) = resolve_latest_alias(dir, target) {
        return Some(path);
    }
    if let Ok(index) = target.parse::<usize>() {
        if index == 0 {
            return None;
        }
        return entries.get(index - 1).cloned();
    }

    entries
        .into_iter()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name == target)
                .unwrap_or(false)
                || path.display().to_string() == target
        })
        .or_else(|| resolve_transcript_target_fuzzy(dir, target))
}

pub(in crate::commands::info::memory) fn resolve_compare_target(
    dir: &Path,
    target: &str,
) -> Option<PathBuf> {
    if let Some(path) = resolve_latest_alias(dir, target) {
        Some(path)
    } else {
        resolve_transcript_target(dir, target)
    }
}

pub(in crate::commands::info::memory) fn parse_latest_compare_target(
    args: &str,
) -> Option<&str> {
    let target = args.strip_prefix("latest compare ")?.trim();
    if target.is_empty() {
        None
    } else {
        Some(target)
    }
}

pub(in crate::commands::info::memory) fn describe_path(path: &Path) -> String {
    match fs::metadata(path) {
        Ok(meta) => format!("{} ({} bytes)", path.display(), meta.len()),
        Err(_) => format!("{} (missing)", path.display()),
    }
}

pub(in crate::commands::info::memory) fn truncate_for_display(content: &str) -> String {
    if content.chars().count() <= MAX_DISPLAY_CHARS {
        return content.to_string();
    }

    let notice = format!(
        "[Truncated for display at {} chars. Scroll for earlier content if your terminal keeps history, or open the file path directly.]",
        MAX_DISPLAY_CHARS
    );
    let budget = MAX_DISPLAY_CHARS.saturating_sub(notice.chars().count() + 2);
    let truncated = content.chars().take(budget).collect::<String>();
    format!("{}\n\n{}", truncated, notice)
}

pub(in crate::commands::info::memory) fn fold_transcript_preview(content: &str) -> String {
    let lines = content.lines().collect::<Vec<_>>();
    let Some(messages_idx) = lines.iter().position(|line| *line == "## Messages") else {
        return truncate_for_display(content);
    };

    if content.chars().count() <= MAX_DISPLAY_CHARS
        && lines.len()
            <= messages_idx + TRANSCRIPT_PREVIEW_MESSAGE_HEAD_LINES + TRANSCRIPT_PREVIEW_TAIL_LINES
    {
        return content.to_string();
    }

    let tail_start = lines
        .len()
        .saturating_sub(TRANSCRIPT_PREVIEW_TAIL_LINES)
        .max(messages_idx + 1);
    let message_preview_end =
        (messages_idx + TRANSCRIPT_PREVIEW_MESSAGE_HEAD_LINES).min(tail_start);

    let mut output = String::new();
    let pre_messages = lines[..messages_idx].join("\n");
    if !pre_messages.trim().is_empty() {
        output.push_str(pre_messages.trim_end());
        output.push_str("\n\n");
    }

    output.push_str("## Messages Preview\n\n");
    output.push_str(&lines[messages_idx..message_preview_end].join("\n"));

    if tail_start > message_preview_end {
        output.push_str(&format!(
            "\n\n... [transcript preview folded: {} middle lines omitted] ...\n\n",
            tail_start - message_preview_end
        ));
        output.push_str(&lines[tail_start..].join("\n"));
    }

    if output.chars().count() > MAX_DISPLAY_CHARS {
        truncate_for_display(&output)
    } else {
        output
    }
}

fn resolve_transcript_target_fuzzy(dir: &Path, target: &str) -> Option<PathBuf> {
    let target = target.trim();
    if target.is_empty() {
        return None;
    }

    let entries = sorted_transcript_entries(dir);
    let exact_basename = entries
        .iter()
        .filter(|path| {
            path.file_stem()
                .and_then(|name| name.to_str())
                .map(|name| name == target)
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    if exact_basename.len() == 1 {
        return exact_basename.into_iter().next();
    }

    let prefix_matches = entries
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.starts_with(target))
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    if prefix_matches.len() == 1 {
        return prefix_matches.into_iter().next();
    }

    let contains_matches = entries
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.contains(target))
                .unwrap_or(false)
        })
        .cloned()
        .collect::<Vec<_>>();
    if contains_matches.len() == 1 {
        return contains_matches.into_iter().next();
    }

    None
}

fn resolve_latest_alias(dir: &Path, target: &str) -> Option<PathBuf> {
    if target == "latest" {
        return latest_transcript(dir);
    }

    let suffix = target.strip_prefix("latest-")?;
    let offset = suffix.parse::<usize>().ok()?;
    if offset == 0 {
        return latest_transcript(dir);
    }

    sorted_transcript_entries(dir).into_iter().nth(offset)
}
