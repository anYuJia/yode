use super::types::{RemoteControlPayload, RemoteQueueItem};

pub(super) fn insert_queue_item(payload: &mut RemoteControlPayload, command: &str) {
    let next_id = next_queue_id(payload);
    payload.command_queue.insert(
        0,
        RemoteQueueItem {
            id: next_id,
            command: command.to_string(),
            status: "queued".to_string(),
            attempts: 0,
            runtime_task_id: None,
            transcript_path: None,
            last_run_at: None,
            last_result_preview: None,
            execution_artifact: None,
            acknowledged_at: None,
        },
    );
}

pub(super) fn resolve_queue_index(payload: &RemoteControlPayload, target: &str) -> Option<usize> {
    let trimmed = target.trim();
    if trimmed.is_empty() || trimmed == "latest" {
        return (!payload.command_queue.is_empty()).then_some(0);
    }
    if let Ok(index) = trimmed.parse::<usize>() {
        return index
            .checked_sub(1)
            .filter(|index| *index < payload.command_queue.len());
    }
    payload
        .command_queue
        .iter()
        .position(|item| item.id == trimmed || item.command == trimmed)
}

fn next_queue_id(payload: &RemoteControlPayload) -> String {
    let max = payload
        .command_queue
        .iter()
        .filter_map(|item| item.id.strip_prefix("q-"))
        .filter_map(|value| value.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    format!("q-{}", max + 1)
}

pub(super) fn default_queue_items() -> Vec<RemoteQueueItem> {
    [
        "/doctor remote",
        "/doctor remote-review",
        "/inspect artifact latest-remote-capability",
        "/inspect artifact latest-remote-execution",
        "/inspect artifact latest-checkpoint",
        "/inspect artifact latest-orchestration",
    ]
    .into_iter()
    .enumerate()
    .map(|(index, command)| RemoteQueueItem {
        id: format!("q-{}", index + 1),
        command: command.to_string(),
        status: "queued".to_string(),
        attempts: 0,
        runtime_task_id: None,
        transcript_path: None,
        last_run_at: None,
        last_result_preview: None,
        execution_artifact: None,
        acknowledged_at: None,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::{default_queue_items, insert_queue_item, resolve_queue_index};
    use crate::builtin::remote_runtime::types::RemoteControlPayload;

    fn payload() -> RemoteControlPayload {
        RemoteControlPayload {
            kind: "remote_control_session".to_string(),
            goal: "ship".to_string(),
            session_id: "session".to_string(),
            provider: "provider".to_string(),
            model: "model".to_string(),
            working_dir: "/tmp".to_string(),
            remote_dir: "/tmp/.yode/remote".to_string(),
            created_at: "now".to_string(),
            status: "queued".to_string(),
            command_queue: default_queue_items(),
            latest_remote_capability: None,
            latest_remote_execution: None,
            latest_checkpoint: None,
            latest_orchestration: None,
        }
    }

    #[test]
    fn queue_helpers_insert_and_resolve_targets() {
        let mut payload = payload();
        insert_queue_item(&mut payload, "/run smoke");

        assert_eq!(payload.command_queue[0].id, "q-7");
        assert_eq!(resolve_queue_index(&payload, "latest"), Some(0));
        assert_eq!(resolve_queue_index(&payload, "2"), Some(1));
        assert_eq!(resolve_queue_index(&payload, "q-7"), Some(0));
        assert_eq!(resolve_queue_index(&payload, "/run smoke"), Some(0));
        assert_eq!(resolve_queue_index(&payload, "missing"), None);
    }
}
