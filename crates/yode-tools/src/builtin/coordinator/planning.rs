use super::*;

#[derive(Debug, Clone)]
pub(super) struct NormalizedWorkstream {
    pub(super) id: String,
    pub(super) description: String,
    pub(super) prompt: String,
    pub(super) subagent_type: Option<String>,
    pub(super) model: Option<String>,
    pub(super) run_in_background: Option<bool>,
    pub(super) allowed_tools: Vec<String>,
    pub(super) depends_on: Vec<String>,
}

pub(super) fn normalize_workstreams(
    workstreams: Vec<Workstream>,
) -> Result<Vec<NormalizedWorkstream>> {
    let mut seen = std::collections::HashSet::new();
    let mut normalized = Vec::new();
    for (index, workstream) in workstreams.into_iter().enumerate() {
        let id = workstream
            .id
            .clone()
            .unwrap_or_else(|| format!("ws{}", index + 1));
        if !seen.insert(id.clone()) {
            return Err(anyhow::anyhow!(
                "Duplicate coordinator workstream id '{}'.",
                id
            ));
        }
        normalized.push(NormalizedWorkstream {
            id,
            description: workstream.description,
            prompt: workstream.prompt,
            subagent_type: workstream.subagent_type,
            model: workstream.model,
            run_in_background: workstream.run_in_background,
            allowed_tools: workstream.allowed_tools,
            depends_on: workstream.depends_on,
        });
    }

    let all_ids = normalized
        .iter()
        .map(|workstream| workstream.id.clone())
        .collect::<std::collections::HashSet<_>>();
    for workstream in &normalized {
        for dependency in &workstream.depends_on {
            if !all_ids.contains(dependency) {
                return Err(anyhow::anyhow!(
                    "Workstream '{}' depends on unknown id '{}'.",
                    workstream.id,
                    dependency
                ));
            }
        }
    }

    Ok(normalized)
}

pub(super) fn build_execution_phases(
    workstreams: &[NormalizedWorkstream],
) -> Result<Vec<Vec<NormalizedWorkstream>>> {
    let mut finished: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut pending = workstreams.to_vec();
    let mut phases = Vec::new();

    while !pending.is_empty() {
        let mut ready = Vec::new();
        let mut still_pending = Vec::new();

        for workstream in pending.into_iter() {
            if workstream
                .depends_on
                .iter()
                .all(|dependency| finished.contains(dependency))
            {
                ready.push(workstream);
            } else {
                still_pending.push(workstream);
            }
        }

        if ready.is_empty() {
            let blocked = still_pending
                .iter()
                .map(|workstream| {
                    let missing = workstream
                        .depends_on
                        .iter()
                        .filter(|dependency| !finished.contains(*dependency))
                        .cloned()
                        .collect::<Vec<_>>();
                    format!("{} -> waiting for {}", workstream.id, missing.join(", "))
                })
                .collect::<Vec<_>>()
                .join("; ");
            return Err(anyhow::anyhow!(
                "Coordinator could not resolve workstream dependencies. Blocked set: {}",
                blocked
            ));
        }

        for workstream in &ready {
            finished.insert(workstream.id.clone());
        }
        phases.push(ready);
        pending = still_pending;
    }

    Ok(phases)
}

pub(super) fn render_phase_plan(
    phases: &[Vec<NormalizedWorkstream>],
    max_parallel: usize,
) -> Vec<Value> {
    phases
        .iter()
        .enumerate()
        .map(|(phase_index, workstreams)| {
            json!({
                "phase": phase_index + 1,
                "batches": workstreams
                    .chunks(max_parallel)
                    .enumerate()
                    .map(|(batch_index, batch)| {
                        json!({
                            "batch": batch_index + 1,
                            "workstreams": batch
                                .iter()
                                .map(|workstream| workstream.id.clone())
                                .collect::<Vec<_>>(),
                        })
                    })
                    .collect::<Vec<_>>(),
                "workstreams": workstreams
                    .iter()
                    .map(|workstream| {
                        json!({
                            "id": workstream.id,
                            "description": workstream.description,
                            "depends_on": workstream.depends_on,
                            "run_in_background": workstream.run_in_background.unwrap_or(true),
                            "allowed_tools": workstream.allowed_tools,
                        })
                    })
                    .collect::<Vec<_>>(),
            })
        })
        .collect()
}

pub(super) fn render_phase_timeline(
    phases: &[Vec<NormalizedWorkstream>],
    max_parallel: usize,
) -> String {
    let mut lines = Vec::new();
    for (phase_index, workstreams) in phases.iter().enumerate() {
        lines.push(format!(
            "  Phase {} [{} workstream(s)]",
            phase_index + 1,
            workstreams.len()
        ));
        for (batch_index, batch) in workstreams.chunks(max_parallel).enumerate() {
            lines.push(format!(
                "    Batch {}: {}",
                batch_index + 1,
                batch
                    .iter()
                    .map(|workstream| format!("{} ({})", workstream.id, workstream.description))
                    .collect::<Vec<_>>()
                    .join(" | ")
            ));
        }
    }
    lines.join("\n")
}

pub(super) fn max_parallel_label(max_parallel: usize) -> Value {
    if max_parallel == usize::MAX {
        Value::String("all".to_string())
    } else {
        json!(max_parallel)
    }
}
