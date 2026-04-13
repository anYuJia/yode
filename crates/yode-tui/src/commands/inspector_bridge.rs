use crate::ui::inspector::{InspectorDocument, InspectorPanel, InspectorState, InspectorTab};

pub(crate) fn document_from_command_output(title: &str, lines: Vec<String>) -> InspectorDocument {
    let panels = split_into_panels(&lines);
    let tabs = panels
        .iter()
        .enumerate()
        .map(|(index, panel)| InspectorTab {
            id: format!("tab-{}", index),
            label: panel.0.clone(),
            item_count: Some(panel.1.len()),
        })
        .collect::<Vec<_>>();
    let inspector_panels = panels
        .into_iter()
        .zip(tabs.iter().cloned())
        .map(|((label, lines), tab)| InspectorPanel {
            tab: InspectorTab {
                label,
                ..tab
            },
            lines,
            badges: Vec::new(),
        })
        .collect::<Vec<_>>();

    InspectorDocument {
        state: InspectorState::new(title.to_string(), tabs),
        panels: inspector_panels,
        footer: None,
    }
}

fn split_into_panels(lines: &[String]) -> Vec<(String, Vec<String>)> {
    if lines.is_empty() {
        return vec![("Main".to_string(), vec!["(empty)".to_string()])];
    }

    let mut panels: Vec<(String, Vec<String>)> = Vec::new();
    let mut current_title = "Overview".to_string();
    let mut current_lines = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        let is_header = !trimmed.is_empty()
            && !trimmed.starts_with('-')
            && !trimmed.starts_with("  -")
            && trimmed.ends_with(':');

        if is_header {
            if !current_lines.is_empty() {
                panels.push((current_title, current_lines));
                current_lines = Vec::new();
            }
            current_title = trimmed.trim_end_matches(':').to_string();
            continue;
        }

        current_lines.push(trimmed.to_string());
    }

    if !current_lines.is_empty() {
        panels.push((current_title, current_lines));
    }

    if panels.is_empty() {
        panels.push(("Overview".to_string(), lines.to_vec()));
    }

    panels
}

#[cfg(test)]
mod tests {
    use super::document_from_command_output;

    #[test]
    fn builds_multiple_tabs_from_section_headers() {
        let doc = document_from_command_output(
            "demo",
            vec![
                "Task workspace 1".to_string(),
                "Timeline:".to_string(),
                "  - created".to_string(),
                "Artifacts:".to_string(),
                "  - output: /tmp/x".to_string(),
            ],
        );
        assert_eq!(doc.panels.len(), 3);
        assert_eq!(doc.panels[1].tab.label, "Timeline");
        assert_eq!(doc.panels[2].tab.label, "Artifacts");
    }
}
