use serde_json::Value;

use crate::app::{ChatEntry, ChatRole};
use crate::display_text::compact_path_tail as compact_path;
use crate::system_message::{parse_system_message, SystemMessageKind};

pub(crate) fn should_hide_tool_from_transcript(name: &str) -> bool {
    matches!(name, "task_output")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolBatchKind {
    Read,
    Search,
    List,
    Fetch,
    Inspect,
    Analyze,
    Explore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ToolBatchItemKind {
    SearchPattern,
    SearchWeb,
    SearchSymbol,
    ReadFile,
    ReadMemory,
    ReadSkill,
    ListDirectory,
    ListMemory,
    ListSkill,
    FetchPage,
    InspectSymbol,
    AnalyzeProject,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolBatchItem {
    pub tool_name: String,
    pub kind: ToolBatchItemKind,
    pub call_index: usize,
    pub result_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolBatch {
    pub kind: ToolBatchKind,
    pub start_index: usize,
    pub next_index: usize,
    pub items: Vec<ToolBatchItem>,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SystemBatchItem {
    pub entry_index: usize,
    pub kind: SystemMessageKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SystemBatch {
    pub start_index: usize,
    pub next_index: usize,
    pub items: Vec<SystemBatchItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubAgentBatchItem {
    pub call_index: usize,
    pub result_index: Option<usize>,
    pub description: String,
    pub tool_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SubAgentBatch {
    pub start_index: usize,
    pub next_index: usize,
    pub items: Vec<SubAgentBatchItem>,
    pub is_active: bool,
}

pub(crate) fn detect_groupable_tool_batch(
    entries: &[ChatEntry],
    start_index: usize,
) -> Option<ToolBatch> {
    let entry = entries.get(start_index)?;
    let ChatRole::ToolCall { name, .. } = &entry.role else {
        return None;
    };
    let args: Value = serde_json::from_str(&entry.content).unwrap_or(Value::Null);
    classify_groupable_tool(name, &args)?;

    let mut index = start_index;
    let mut items = Vec::new();
    let mut is_active = false;

    while index < entries.len() {
        let Some(entry) = entries.get(index) else {
            break;
        };
        let ChatRole::ToolCall { id, name } = &entry.role else {
            break;
        };
        let args: Value = serde_json::from_str(&entry.content).unwrap_or(Value::Null);
        let Some(kind) = classify_groupable_tool(name, &args) else {
            break;
        };

        let result_index = entries.get(index + 1).and_then(|next| match &next.role {
            ChatRole::ToolResult { id: result_id, .. } if result_id == id => Some(index + 1),
            _ => None,
        });
        is_active |= result_index.is_none();

        items.push(ToolBatchItem {
            tool_name: name.clone(),
            kind,
            call_index: index,
            result_index,
        });
        index = result_index.map(|value| value + 1).unwrap_or(index + 1);
    }

    if items.len() < 2 {
        return None;
    }

    Some(ToolBatch {
        kind: tool_batch_kind(&items),
        start_index,
        next_index: index,
        items,
        is_active,
    })
}

pub(crate) fn tool_batch_summary_text(batch: &ToolBatch) -> String {
    let mut parts = Vec::new();
    for kind in SUMMARY_KIND_ORDER {
        let count = batch.items.iter().filter(|item| item.kind == *kind).count();
        if count == 0 {
            continue;
        }
        parts.push(summary_part_for_kind(
            *kind,
            count,
            batch.is_active,
            parts.is_empty(),
        ));
    }

    let text = parts.join(", ");
    if batch.is_active {
        format!("{}...", text)
    } else {
        text
    }
}

pub(crate) fn tool_batch_stable_id(entries: &[ChatEntry], batch: &ToolBatch) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    hash_bytes(&mut hash, format!("{:?}", batch.kind).as_bytes());
    for item in &batch.items {
        if let Some(entry) = entries.get(item.call_index) {
            if let ChatRole::ToolCall { id, name } = &entry.role {
                hash_bytes(&mut hash, id.as_bytes());
                hash_bytes(&mut hash, name.as_bytes());
                hash_bytes(&mut hash, entry.content.as_bytes());
            }
        }
        hash_bytes(&mut hash, format!("{:?}", item.kind).as_bytes());
    }
    format!("tool-batch-{hash:016x}")
}

pub(crate) fn summarize_groupable_tool_call(
    tool_name: &str,
    args_json: &str,
    is_active: bool,
) -> Option<String> {
    let args: Value = serde_json::from_str(args_json).unwrap_or(Value::Null);
    let kind = classify_groupable_tool(tool_name, &args)?;
    let text = summary_part_for_kind(kind, 1, is_active, true);
    Some(if is_active {
        format!("{}...", text)
    } else {
        text
    })
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}

pub(crate) fn tool_batch_hint_text(entries: &[ChatEntry], batch: &ToolBatch) -> Option<String> {
    let item = batch.items.last()?;
    let entry = entries.get(item.call_index)?;
    let ChatRole::ToolCall { name, .. } = &entry.role else {
        return None;
    };
    let args: Value = serde_json::from_str(&entry.content).unwrap_or(Value::Null);
    describe_tool_call(name, &args, batch.is_active)
}

pub(crate) fn tool_batch_progress_text(entries: &[ChatEntry], batch: &ToolBatch) -> Option<String> {
    batch
        .items
        .iter()
        .rev()
        .filter_map(|item| entries.get(item.call_index))
        .find_map(|entry| {
            let progress = entry.progress.as_ref()?;
            let mut text = progress.message.clone();
            if let Some(percent) = progress.percent {
                text.push_str(&format!(" {}%", percent));
            }
            Some(text)
        })
}

pub(crate) fn summarize_batch_invocations(
    args: &Value,
    is_active: bool,
) -> Option<(String, Option<String>)> {
    let invocations = args.get("invocations")?.as_array()?;
    let mut counts = std::collections::BTreeMap::new();
    let mut first_target = None;

    for invocation in invocations {
        let tool_name = invocation.get("tool_name").and_then(Value::as_str)?;
        let params = invocation.get("params").unwrap_or(&Value::Null);
        let kind = classify_groupable_tool(tool_name, params)?;
        *counts.entry(kind).or_insert(0usize) += 1;
        if first_target.is_none() {
            first_target = batch_invocation_target(tool_name, params);
        }
    }

    if counts.is_empty() {
        return None;
    }

    let mut parts = Vec::new();
    for kind in SUMMARY_KIND_ORDER {
        let count = counts.get(kind).copied().unwrap_or(0);
        if count == 0 {
            continue;
        }
        parts.push(summary_part_for_kind(
            *kind,
            count,
            is_active,
            parts.is_empty(),
        ));
    }

    let text = parts.join(", ");
    Some((
        if is_active {
            format!("{}...", text)
        } else {
            text
        },
        first_target,
    ))
}

pub(crate) fn describe_tool_call(tool_name: &str, args: &Value, is_active: bool) -> Option<String> {
    if let Some(description) = describe_groupable_tool_call(tool_name, args, is_active) {
        return Some(description);
    }

    match tool_name {
        "edit_file" | "multi_edit" => Some(format!(
            "{} {}",
            if is_active { "Editing" } else { "Edited" },
            compact_path(
                args.get("file_path")
                    .and_then(Value::as_str)
                    .unwrap_or("file")
            )
        )),
        "write_file" => Some(format!(
            "{} {}",
            if is_active { "Writing" } else { "Wrote" },
            compact_path(
                args.get("file_path")
                    .and_then(Value::as_str)
                    .unwrap_or("file")
            )
        )),
        "bash" | "powershell" => Some(format!(
            "{} {}",
            if is_active { "Running" } else { "Ran" },
            truncate_words(
                args.get("command")
                    .and_then(Value::as_str)
                    .unwrap_or("command")
                    .lines()
                    .next()
                    .unwrap_or("command"),
                72,
            )
        )),
        "batch" => Some(
            summarize_batch_invocations(args, is_active)
                .map(|(summary, _)| summary)
                .unwrap_or_else(|| {
                    format!(
                        "{} {} tools in parallel",
                        if is_active { "Running" } else { "Ran" },
                        args.get("invocations")
                            .and_then(Value::as_array)
                            .map(|items| items.len())
                            .unwrap_or(0)
                    )
                })
                .to_string(),
        ),
        "agent" | "coordinator" => Some(format!(
            "{} {}",
            if is_active {
                "Running agent"
            } else {
                "Ran agent"
            },
            truncate_words(
                args.get("description")
                    .or_else(|| args.get("prompt"))
                    .or_else(|| args.get("task"))
                    .and_then(Value::as_str)
                    .unwrap_or("task"),
                72,
            )
        )),
        "notebook_edit" => Some(format!(
            "{} notebook {}",
            if is_active { "Editing" } else { "Edited" },
            compact_path(
                args.get("notebook_path")
                    .and_then(Value::as_str)
                    .unwrap_or("notebook")
            )
        )),
        "web_browser" => Some(format!(
            "{} browser {}",
            if is_active { "Using" } else { "Used" },
            args.get("action")
                .and_then(Value::as_str)
                .unwrap_or("action")
        )),
        "memory" => {
            let action = args
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("manage");
            let name = args.get("name").and_then(Value::as_str).unwrap_or("memory");
            Some(format!(
                "{} memory {}",
                describe_memory_action(action, is_active),
                name
            ))
        }
        _ => None,
    }
}

pub(crate) fn describe_groupable_tool_call(
    tool_name: &str,
    args: &Value,
    is_active: bool,
) -> Option<String> {
    let kind = classify_groupable_tool(tool_name, args)?;
    Some(match kind {
        ToolBatchItemKind::ReadFile => format!(
            "{} {}",
            if is_active { "Reading" } else { "Read" },
            compact_path(
                args.get("file_path")
                    .and_then(Value::as_str)
                    .unwrap_or("file")
            )
        ),
        ToolBatchItemKind::SearchPattern => format!(
            "{} {}",
            if is_active {
                "Searching for"
            } else {
                "Searched for"
            },
            args.get("pattern")
                .and_then(Value::as_str)
                .unwrap_or("pattern")
        ),
        ToolBatchItemKind::SearchWeb => format!(
            "{} {}",
            if is_active {
                "Searching the web for"
            } else {
                "Searched the web for"
            },
            args.get("query").and_then(Value::as_str).unwrap_or("query")
        ),
        ToolBatchItemKind::SearchSymbol => format!(
            "{} {}",
            if is_active {
                "Searching references in"
            } else {
                "Searched references in"
            },
            compact_path(
                args.get("filePath")
                    .and_then(Value::as_str)
                    .unwrap_or("file")
            )
        ),
        ToolBatchItemKind::ReadMemory => format!(
            "{} {}",
            if is_active { "Recalling" } else { "Recalled" },
            args.get("name").and_then(Value::as_str).unwrap_or("memory")
        ),
        ToolBatchItemKind::ReadSkill => format!(
            "{} skill {}",
            if is_active { "Reading" } else { "Read" },
            args.get("name").and_then(Value::as_str).unwrap_or("skill")
        ),
        ToolBatchItemKind::ListDirectory => format!(
            "{} {}",
            if is_active { "Listing" } else { "Listed" },
            compact_path(args.get("path").and_then(Value::as_str).unwrap_or("."))
        ),
        ToolBatchItemKind::ListMemory => format!(
            "{} {} memories",
            if is_active { "Listing" } else { "Listed" },
            args.get("scope")
                .and_then(Value::as_str)
                .unwrap_or("project")
        ),
        ToolBatchItemKind::ListSkill => {
            if is_active {
                "Listing available skills".to_string()
            } else {
                "Listed available skills".to_string()
            }
        }
        ToolBatchItemKind::FetchPage => format!(
            "{} {}",
            if is_active { "Fetching" } else { "Fetched" },
            args.get("url").and_then(Value::as_str).unwrap_or("page")
        ),
        ToolBatchItemKind::InspectSymbol => format!(
            "{} {} in {}",
            if is_active { "Inspecting" } else { "Inspected" },
            lsp_inspect_label(
                args.get("operation")
                    .and_then(Value::as_str)
                    .unwrap_or("symbol")
            ),
            compact_path(
                args.get("filePath")
                    .and_then(Value::as_str)
                    .unwrap_or("file")
            )
        ),
        ToolBatchItemKind::AnalyzeProject => {
            if is_active {
                "Analyzing project structure".to_string()
            } else {
                "Analyzed project structure".to_string()
            }
        }
    })
}

pub(crate) fn detect_groupable_system_batch(
    entries: &[ChatEntry],
    start_index: usize,
) -> Option<SystemBatch> {
    let entry = entries.get(start_index)?;
    if !matches!(entry.role, ChatRole::System) {
        return None;
    }
    let first_kind = parse_system_message(&entry.content).kind;
    if !is_groupable_system_kind(first_kind) {
        return None;
    }

    let mut index = start_index;
    let mut items = Vec::new();
    while index < entries.len() {
        let Some(entry) = entries.get(index) else {
            break;
        };
        if !matches!(entry.role, ChatRole::System) {
            break;
        }
        let kind = parse_system_message(&entry.content).kind;
        if !is_groupable_system_kind(kind) {
            break;
        }
        items.push(SystemBatchItem {
            entry_index: index,
            kind,
        });
        index += 1;
    }

    if items.len() < 2 {
        return None;
    }

    Some(SystemBatch {
        start_index,
        next_index: index,
        items,
    })
}

pub(crate) fn detect_groupable_subagent_batch(
    entries: &[ChatEntry],
    start_index: usize,
) -> Option<SubAgentBatch> {
    if !is_subagent_batch_start(entries.get(start_index)?) {
        return None;
    }

    let mut index = start_index;
    let mut items = Vec::new();
    while let Some(item) = parse_subagent_segment(entries, index) {
        index = item.result_index.map(|value| value + 1).unwrap_or_else(|| {
            let mut next = item.call_index + 1;
            while matches!(
                entries.get(next).map(|entry| &entry.role),
                Some(ChatRole::SubAgentToolCall { .. })
            ) {
                next += 1;
            }
            next
        });
        items.push(item);
        let Some(next_entry) = entries.get(index) else {
            break;
        };
        if !is_subagent_batch_start(next_entry) {
            break;
        }
    }

    if items.len() < 2 {
        return None;
    }

    Some(SubAgentBatch {
        start_index,
        next_index: index,
        is_active: items.iter().any(|item| item.result_index.is_none()),
        items,
    })
}

fn is_subagent_batch_start(entry: &ChatEntry) -> bool {
    match &entry.role {
        ChatRole::SubAgentCall { .. } => true,
        ChatRole::ToolCall { name, .. } => is_agent_tool_name(name),
        _ => false,
    }
}

pub(crate) fn is_agent_tool_name(name: &str) -> bool {
    matches!(name, "agent" | "coordinator")
}

const SUMMARY_KIND_ORDER: &[ToolBatchItemKind] = &[
    ToolBatchItemKind::SearchPattern,
    ToolBatchItemKind::SearchSymbol,
    ToolBatchItemKind::SearchWeb,
    ToolBatchItemKind::ReadFile,
    ToolBatchItemKind::ReadMemory,
    ToolBatchItemKind::ReadSkill,
    ToolBatchItemKind::ListDirectory,
    ToolBatchItemKind::ListMemory,
    ToolBatchItemKind::ListSkill,
    ToolBatchItemKind::FetchPage,
    ToolBatchItemKind::InspectSymbol,
    ToolBatchItemKind::AnalyzeProject,
];

fn classify_groupable_tool(name: &str, args: &Value) -> Option<ToolBatchItemKind> {
    match name {
        "read_file" => Some(ToolBatchItemKind::ReadFile),
        "grep" | "glob" => Some(ToolBatchItemKind::SearchPattern),
        "ls" => Some(ToolBatchItemKind::ListDirectory),
        "web_search" => Some(ToolBatchItemKind::SearchWeb),
        "web_fetch" => Some(ToolBatchItemKind::FetchPage),
        "project_map" => Some(ToolBatchItemKind::AnalyzeProject),
        "discover_skills" => Some(ToolBatchItemKind::ListSkill),
        "skill" => match args.get("action").and_then(Value::as_str).unwrap_or("get") {
            "list" => Some(ToolBatchItemKind::ListSkill),
            "get" => Some(ToolBatchItemKind::ReadSkill),
            _ => None,
        },
        "memory" => match args.get("action").and_then(Value::as_str).unwrap_or("") {
            "read" => Some(ToolBatchItemKind::ReadMemory),
            "list" => Some(ToolBatchItemKind::ListMemory),
            _ => None,
        },
        "lsp" => match args.get("operation").and_then(Value::as_str).unwrap_or("") {
            "findReferences" => Some(ToolBatchItemKind::SearchSymbol),
            "hover" | "goToDefinition" | "documentSymbol" => Some(ToolBatchItemKind::InspectSymbol),
            _ => None,
        },
        _ => None,
    }
}

fn batch_invocation_target(tool_name: &str, args: &Value) -> Option<String> {
    match tool_name {
        "read_file" => Some(compact_path(
            args.get("file_path")
                .and_then(Value::as_str)
                .unwrap_or("file"),
        )),
        "grep" | "glob" => Some(format!(
            "\"{}\"",
            args.get("pattern")
                .and_then(Value::as_str)
                .unwrap_or("pattern")
        )),
        "ls" => Some(compact_path(
            args.get("path").and_then(Value::as_str).unwrap_or("."),
        )),
        _ => None,
    }
}

fn tool_batch_kind(items: &[ToolBatchItem]) -> ToolBatchKind {
    let mut families = items.iter().map(|item| family_for_item_kind(item.kind));
    let Some(first_family) = families.next() else {
        return ToolBatchKind::Explore;
    };
    if families.any(|family| family != first_family) {
        ToolBatchKind::Explore
    } else {
        first_family
    }
}

fn family_for_item_kind(kind: ToolBatchItemKind) -> ToolBatchKind {
    match kind {
        ToolBatchItemKind::SearchPattern
        | ToolBatchItemKind::SearchWeb
        | ToolBatchItemKind::SearchSymbol => ToolBatchKind::Search,
        ToolBatchItemKind::ReadFile
        | ToolBatchItemKind::ReadMemory
        | ToolBatchItemKind::ReadSkill => ToolBatchKind::Read,
        ToolBatchItemKind::ListDirectory
        | ToolBatchItemKind::ListMemory
        | ToolBatchItemKind::ListSkill => ToolBatchKind::List,
        ToolBatchItemKind::FetchPage => ToolBatchKind::Fetch,
        ToolBatchItemKind::InspectSymbol => ToolBatchKind::Inspect,
        ToolBatchItemKind::AnalyzeProject => ToolBatchKind::Analyze,
    }
}

fn summary_part_for_kind(
    kind: ToolBatchItemKind,
    count: usize,
    is_active: bool,
    is_first: bool,
) -> String {
    match kind {
        ToolBatchItemKind::SearchPattern => action_summary_part(
            count,
            "pattern",
            "patterns",
            is_active,
            is_first,
            "Searching for",
            "searching for",
            "Searched for",
            "searched for",
        ),
        ToolBatchItemKind::SearchSymbol => action_summary_part(
            count,
            "symbol",
            "symbols",
            is_active,
            is_first,
            "Searching for",
            "searching for",
            "Searched for",
            "searched for",
        ),
        ToolBatchItemKind::SearchWeb => action_summary_part(
            count,
            "query",
            "queries",
            is_active,
            is_first,
            "Searching the web for",
            "searching the web for",
            "Searched the web for",
            "searched the web for",
        ),
        ToolBatchItemKind::ReadFile => action_summary_part(
            count, "file", "files", is_active, is_first, "Reading", "reading", "Read", "read",
        ),
        ToolBatchItemKind::ReadMemory => action_summary_part(
            count,
            "memory",
            "memories",
            is_active,
            is_first,
            "Recalling",
            "recalling",
            "Recalled",
            "recalled",
        ),
        ToolBatchItemKind::ReadSkill => action_summary_part(
            count, "skill", "skills", is_active, is_first, "Reading", "reading", "Read", "read",
        ),
        ToolBatchItemKind::ListDirectory => action_summary_part(
            count,
            "directory",
            "directories",
            is_active,
            is_first,
            "Listing",
            "listing",
            "Listed",
            "listed",
        ),
        ToolBatchItemKind::ListMemory => action_summary_part(
            count, "memory", "memories", is_active, is_first, "Listing", "listing", "Listed",
            "listed",
        ),
        ToolBatchItemKind::ListSkill => action_summary_part(
            count, "skill", "skills", is_active, is_first, "Listing", "listing", "Listed", "listed",
        ),
        ToolBatchItemKind::FetchPage => action_summary_part(
            count, "page", "pages", is_active, is_first, "Fetching", "fetching", "Fetched",
            "fetched",
        ),
        ToolBatchItemKind::InspectSymbol => action_summary_part(
            count,
            "symbol",
            "symbols",
            is_active,
            is_first,
            "Inspecting",
            "inspecting",
            "Inspected",
            "inspected",
        ),
        ToolBatchItemKind::AnalyzeProject => action_summary_part(
            count,
            "project",
            "projects",
            is_active,
            is_first,
            "Analyzing",
            "analyzing",
            "Analyzed",
            "analyzed",
        ),
    }
}

fn action_summary_part(
    count: usize,
    singular: &str,
    plural: &str,
    is_active: bool,
    is_first: bool,
    active_first: &str,
    active_rest: &str,
    done_first: &str,
    done_rest: &str,
) -> String {
    let verb = if is_active {
        if is_first {
            active_first
        } else {
            active_rest
        }
    } else if is_first {
        done_first
    } else {
        done_rest
    };
    let noun = if count == 1 { singular } else { plural };
    format!("{} {} {}", verb, count, noun)
}

fn lsp_inspect_label(operation: &str) -> &'static str {
    match operation {
        "hover" => "hover",
        "goToDefinition" => "definition",
        "documentSymbol" => "document symbols",
        _ => "symbol",
    }
}

fn truncate_words(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    format!("{}...", text.chars().take(max_chars).collect::<String>())
}

fn describe_memory_action(action: &str, is_active: bool) -> &'static str {
    match (action, is_active) {
        ("save", true) => "Saving",
        ("save", false) => "Saved",
        ("delete", true) => "Deleting",
        ("delete", false) => "Deleted",
        ("list", true) => "Listing",
        ("list", false) => "Listed",
        ("read", true) => "Recalling",
        ("read", false) => "Recalled",
        (_, true) => "Managing",
        (_, false) => "Managed",
    }
}

fn parse_subagent_segment(entries: &[ChatEntry], start_index: usize) -> Option<SubAgentBatchItem> {
    let call = entries.get(start_index)?;
    let (description, tool_id) = match &call.role {
        ChatRole::SubAgentCall { description } => (description.clone(), None),
        ChatRole::ToolCall { id, name } if is_agent_tool_name(name) => {
            let args: Value = serde_json::from_str(&call.content).unwrap_or(Value::Null);
            let description = args
                .get("description")
                .or_else(|| args.get("prompt"))
                .or_else(|| args.get("task"))
                .and_then(Value::as_str)
                .unwrap_or("Agent")
                .to_string();
            (description, Some(id.clone()))
        }
        _ => return None,
    };
    let mut tool_count = 0usize;
    let mut index = start_index + 1;
    while let Some(entry) = entries.get(index) {
        match &entry.role {
            ChatRole::SubAgentToolCall { .. } => {
                tool_count += 1;
                index += 1;
            }
            ChatRole::ToolResult { id, .. } if tool_id.as_ref() == Some(id) => {
                return Some(SubAgentBatchItem {
                    call_index: start_index,
                    result_index: Some(index),
                    description,
                    tool_count,
                });
            }
            ChatRole::SubAgentResult => {
                return Some(SubAgentBatchItem {
                    call_index: start_index,
                    result_index: Some(index),
                    description,
                    tool_count,
                });
            }
            _ if is_subagent_batch_start(entry) => {
                return Some(SubAgentBatchItem {
                    call_index: start_index,
                    result_index: None,
                    description,
                    tool_count,
                });
            }
            _ => return None,
        }
    }
    Some(SubAgentBatchItem {
        call_index: start_index,
        result_index: None,
        description,
        tool_count,
    })
}

fn is_groupable_system_kind(kind: SystemMessageKind) -> bool {
    matches!(
        kind,
        SystemMessageKind::Context
            | SystemMessageKind::Memory
            | SystemMessageKind::Budget
            | SystemMessageKind::Export
            | SystemMessageKind::Task
            | SystemMessageKind::Update
    )
}

#[cfg(test)]
mod tests {
    use crate::app::{ChatEntry, ChatRole};

    use super::{
        describe_groupable_tool_call, describe_tool_call, detect_groupable_subagent_batch,
        detect_groupable_system_batch, detect_groupable_tool_batch, summarize_batch_invocations,
        tool_batch_hint_text, tool_batch_progress_text, tool_batch_stable_id,
        tool_batch_summary_text, ToolBatchItemKind, ToolBatchKind,
    };

    #[test]
    fn detects_mixed_exploration_tool_calls() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "glob".to_string(),
                },
                "{\"pattern\":\"src/**/*.rs\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "glob".to_string(),
                    is_error: false,
                },
                "ok".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/b\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "ok".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "c".to_string(),
                    name: "ls".to_string(),
                },
                "{\"path\":\"src\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "c".to_string(),
                    name: "ls".to_string(),
                    is_error: false,
                },
                "ok".to_string(),
            ),
        ];

        let batch = detect_groupable_tool_batch(&entries, 0).unwrap();
        assert_eq!(batch.kind, ToolBatchKind::Explore);
        assert_eq!(batch.items.len(), 3);
        assert_eq!(batch.items[0].kind, ToolBatchItemKind::SearchPattern);
        assert_eq!(batch.items[1].kind, ToolBatchItemKind::ReadFile);
        assert_eq!(batch.items[2].kind, ToolBatchItemKind::ListDirectory);
        assert_eq!(batch.next_index, 6);
        assert_eq!(
            tool_batch_summary_text(&batch),
            "Searched for 1 pattern, read 1 file, listed 1 directory"
        );
    }

    #[test]
    fn tool_batch_stable_id_is_replay_stable() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "call-1".to_string(),
                    name: "grep".to_string(),
                },
                "{\"pattern\":\"todo\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "call-1".to_string(),
                    name: "grep".to_string(),
                    is_error: false,
                },
                "ok".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "call-2".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"src/lib.rs\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "call-2".to_string(),
                    name: "read_file".to_string(),
                    is_error: false,
                },
                "ok".to_string(),
            ),
        ];
        let batch = detect_groupable_tool_batch(&entries, 0).unwrap();
        let replayed = entries.clone();
        let replayed_batch = detect_groupable_tool_batch(&replayed, 0).unwrap();

        assert_eq!(
            tool_batch_stable_id(&entries, &batch),
            tool_batch_stable_id(&replayed, &replayed_batch)
        );
    }

    #[test]
    fn active_batch_uses_present_tense_summary_for_web_and_lsp() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "web_search".to_string(),
                },
                "{\"query\":\"rust ansi tui\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "lsp".to_string(),
                },
                "{\"operation\":\"hover\",\"filePath\":\"/tmp/demo.rs\",\"line\":1,\"character\":2}"
                    .to_string(),
            ),
        ];

        let batch = detect_groupable_tool_batch(&entries, 0).unwrap();
        assert!(batch.is_active);
        assert_eq!(batch.kind, ToolBatchKind::Explore);
        assert_eq!(batch.items[0].kind, ToolBatchItemKind::SearchWeb);
        assert_eq!(batch.items[1].kind, ToolBatchItemKind::InspectSymbol);
        assert_eq!(
            tool_batch_summary_text(&batch),
            "Searching the web for 1 query, inspecting 1 symbol..."
        );
    }

    #[test]
    fn groups_memory_and_skill_reads() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "memory".to_string(),
                },
                "{\"action\":\"read\",\"name\":\"plan\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "memory".to_string(),
                    is_error: false,
                },
                "ok".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "skill".to_string(),
                },
                "{\"action\":\"get\",\"name\":\"rust\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "b".to_string(),
                    name: "skill".to_string(),
                    is_error: false,
                },
                "ok".to_string(),
            ),
        ];

        let batch = detect_groupable_tool_batch(&entries, 0).unwrap();
        assert_eq!(batch.kind, ToolBatchKind::Read);
        assert_eq!(batch.items[0].kind, ToolBatchItemKind::ReadMemory);
        assert_eq!(batch.items[1].kind, ToolBatchItemKind::ReadSkill);
        assert_eq!(
            tool_batch_summary_text(&batch),
            "Recalled 1 memory, read 1 skill"
        );
    }

    #[test]
    fn detects_consecutive_groupable_system_messages() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::System,
                "Context compacted · auto · -4 msgs".to_string(),
            ),
            ChatEntry::new(
                ChatRole::System,
                "Session memory updated · summary · /tmp/live.md".to_string(),
            ),
            ChatEntry::new(ChatRole::Assistant, "ok".to_string()),
        ];

        let batch = detect_groupable_system_batch(&entries, 0).unwrap();
        assert_eq!(batch.items.len(), 2);
        assert_eq!(batch.next_index, 2);
    }

    #[test]
    fn describes_groupable_tool_calls_with_targets() {
        let read = describe_groupable_tool_call(
            "read_file",
            &serde_json::json!({"file_path": "/tmp/src/main.rs"}),
            false,
        )
        .unwrap();
        let search = describe_groupable_tool_call(
            "web_search",
            &serde_json::json!({"query": "ratatui status summary"}),
            true,
        )
        .unwrap();
        assert_eq!(read, "Read .../src/main.rs");
        assert_eq!(search, "Searching the web for ratatui status summary");
    }

    #[test]
    fn describes_common_non_groupable_tool_calls() {
        let edit = describe_tool_call(
            "edit_file",
            &serde_json::json!({"file_path": "/tmp/src/main.rs"}),
            true,
        )
        .unwrap();
        let shell = describe_tool_call(
            "bash",
            &serde_json::json!({"command": "cargo test -p yode-tui"}),
            false,
        )
        .unwrap();
        let agent = describe_tool_call(
            "agent",
            &serde_json::json!({"description": "Analyze Yode architecture"}),
            true,
        )
        .unwrap();
        assert_eq!(edit, "Editing .../src/main.rs");
        assert_eq!(shell, "Ran cargo test -p yode-tui");
        assert_eq!(agent, "Running agent Analyze Yode architecture");
    }

    #[test]
    fn detects_consecutive_completed_subagent_segments() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::SubAgentCall {
                    description: "审查UI/UX分析遗漏".to_string(),
                },
                String::new(),
            ),
            ChatEntry::new(
                ChatRole::SubAgentToolCall {
                    name: "read_file".to_string(),
                },
                String::new(),
            ),
            ChatEntry::new(ChatRole::SubAgentResult, "done".to_string()),
            ChatEntry::new(
                ChatRole::SubAgentCall {
                    description: "审查交互与动效细节".to_string(),
                },
                String::new(),
            ),
            ChatEntry::new(ChatRole::SubAgentResult, "done".to_string()),
        ];
        let batch = detect_groupable_subagent_batch(&entries, 0).unwrap();
        assert_eq!(batch.items.len(), 2);
        assert_eq!(batch.items[0].tool_count, 1);
        assert_eq!(batch.items[1].tool_count, 0);
        assert_eq!(batch.next_index, 5);
        assert!(!batch.is_active);
    }

    #[test]
    fn detects_consecutive_active_subagent_segments() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::SubAgentCall {
                    description: "Analyze Yode architecture".to_string(),
                },
                String::new(),
            ),
            ChatEntry::new(
                ChatRole::SubAgentCall {
                    description: "Find claude-code-rev project".to_string(),
                },
                String::new(),
            ),
        ];
        let batch = detect_groupable_subagent_batch(&entries, 0).unwrap();
        assert_eq!(batch.items.len(), 2);
        assert_eq!(batch.next_index, 2);
        assert!(batch.is_active);
        assert!(batch.items.iter().all(|item| item.result_index.is_none()));
    }

    #[test]
    fn detects_agent_tool_calls_as_explore_agent_batch() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "agent".to_string(),
                },
                "{\"description\":\"Analyze Yode architecture\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "agent".to_string(),
                    is_error: false,
                },
                "Background sub-agent launched as task-1. Output: /tmp/task-1.log".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "agent".to_string(),
                },
                "{\"description\":\"Find claude-code-rev project\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "b".to_string(),
                    name: "agent".to_string(),
                    is_error: false,
                },
                "done".to_string(),
            ),
        ];

        let batch = detect_groupable_subagent_batch(&entries, 0).unwrap();
        assert_eq!(batch.items.len(), 2);
        assert_eq!(batch.next_index, 4);
        assert_eq!(batch.items[0].description, "Analyze Yode architecture");
        assert_eq!(batch.items[1].description, "Find claude-code-rev project");
        assert!(!batch.is_active);
    }

    #[test]
    fn summarizes_batch_invocations_as_exploration_summary() {
        let (summary, target) = summarize_batch_invocations(
            &serde_json::json!({
                "invocations": [
                    {"tool_name": "grep", "params": {"pattern": "showDialog"}},
                    {"tool_name": "read_file", "params": {"file_path": "/tmp/lib/a.dart"}},
                    {"tool_name": "read_file", "params": {"file_path": "/tmp/lib/b.dart"}}
                ]
            }),
            true,
        )
        .unwrap();
        assert_eq!(summary, "Searching for 1 pattern, reading 2 files...");
        assert_eq!(target.as_deref(), Some("\"showDialog\""));
    }

    #[test]
    fn tool_batch_hint_uses_latest_grouped_target() {
        let entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "grep".to_string(),
                },
                "{\"pattern\":\"retry\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "grep".to_string(),
                    is_error: false,
                },
                "ok".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/src/main.rs\"}".to_string(),
            ),
        ];
        let batch = detect_groupable_tool_batch(&entries, 0).unwrap();
        assert_eq!(
            tool_batch_hint_text(&entries, &batch).as_deref(),
            Some("Reading .../src/main.rs")
        );
    }

    #[test]
    fn tool_batch_progress_uses_latest_progress_update() {
        let mut entries = vec![
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "a".to_string(),
                    name: "grep".to_string(),
                },
                "{\"pattern\":\"retry\"}".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolResult {
                    id: "a".to_string(),
                    name: "grep".to_string(),
                    is_error: false,
                },
                "ok".to_string(),
            ),
            ChatEntry::new(
                ChatRole::ToolCall {
                    id: "b".to_string(),
                    name: "read_file".to_string(),
                },
                "{\"file_path\":\"/tmp/src/main.rs\"}".to_string(),
            ),
        ];
        entries[2].progress = Some(yode_tools::tool::ToolProgress {
            message: "chunk 2/4".to_string(),
            percent: Some(50),
        });
        let batch = detect_groupable_tool_batch(&entries, 0).unwrap();
        assert_eq!(
            tool_batch_progress_text(&entries, &batch).as_deref(),
            Some("chunk 2/4 50%")
        );
    }
}
