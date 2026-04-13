pub(crate) struct WorkspaceText {
    title: String,
    subtitle: Option<String>,
    metadata: Vec<(String, String)>,
    sections: Vec<WorkspaceSection>,
    footer: Option<String>,
}

pub(crate) struct WorkspaceSection {
    title: String,
    lines: Vec<String>,
}

impl WorkspaceText {
    pub(crate) fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            metadata: Vec::new(),
            sections: Vec::new(),
            footer: None,
        }
    }

    pub(crate) fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub(crate) fn field(mut self, label: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.push((label.into(), value.into()));
        self
    }

    pub(crate) fn section(
        mut self,
        title: impl Into<String>,
        lines: impl IntoIterator<Item = String>,
    ) -> Self {
        self.sections.push(WorkspaceSection {
            title: title.into(),
            lines: lines.into_iter().collect(),
        });
        self
    }

    pub(crate) fn footer(mut self, footer: impl Into<String>) -> Self {
        self.footer = Some(footer.into());
        self
    }

    pub(crate) fn render(self) -> String {
        let mut out = String::new();
        out.push_str(&self.title);
        out.push('\n');

        if let Some(subtitle) = self.subtitle {
            out.push_str(&subtitle);
            out.push('\n');
        }

        if !self.metadata.is_empty() {
            for (label, value) in self.metadata {
                out.push_str(&format!("  {:<14} {}\n", format!("{}:", label), value));
            }
        }

        for section in self.sections {
            out.push('\n');
            out.push_str(&format!("{}:\n", section.title));
            if section.lines.is_empty() {
                out.push_str("  - none\n");
            } else {
                for line in section.lines {
                    out.push_str(&format!("  - {}\n", line));
                }
            }
        }

        if let Some(footer) = self.footer {
            out.push('\n');
            out.push_str(&footer);
        }

        out.trim_end().to_string()
    }
}

pub(crate) fn workspace_bullets(
    lines: impl IntoIterator<Item = impl Into<String>>,
) -> Vec<String> {
    lines.into_iter().map(Into::into).collect()
}

pub(crate) fn workspace_preview_line(label: &str, value: Option<&str>) -> String {
    format!("{}: {}", label, value.unwrap_or("none"))
}

pub(crate) fn workspace_artifact_lines(
    entries: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
) -> Vec<String> {
    entries
        .into_iter()
        .map(|(label, value)| format!("{}: {}", label.into(), value.into()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        workspace_artifact_lines, workspace_bullets, workspace_preview_line, WorkspaceText,
    };

    #[test]
    fn workspace_renderer_formats_metadata_sections_and_footer() {
        let rendered = WorkspaceText::new("Task workspace")
            .subtitle("demo")
            .field("Status", "running")
            .section("Timeline", workspace_bullets(["created", "started"]))
            .footer("Use /tasks read latest")
            .render();
        assert!(rendered.contains("Task workspace"));
        assert!(rendered.contains("Status:"));
        assert!(rendered.contains("Timeline:"));
        assert!(rendered.contains("Use /tasks read latest"));
    }

    #[test]
    fn preview_line_falls_back_to_none() {
        assert_eq!(workspace_preview_line("Preview", None), "Preview: none");
        assert_eq!(
            workspace_preview_line("Preview", Some("value")),
            "Preview: value"
        );
    }

    #[test]
    fn artifact_lines_render_label_value_pairs() {
        let lines = workspace_artifact_lines([("output", "/tmp/a.log"), ("transcript", "/tmp/a.md")]);
        assert_eq!(lines[0], "output: /tmp/a.log");
        assert_eq!(lines[1], "transcript: /tmp/a.md");
    }
}
