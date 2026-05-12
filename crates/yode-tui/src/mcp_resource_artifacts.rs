use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
struct McpResourceManifest {
    server: Option<String>,
    uri: Option<String>,
    blobs: Option<String>,
    retention: Option<String>,
    decode_warnings: usize,
}

pub(crate) fn mcp_resource_decode_warning_count(path: &Path) -> usize {
    std::fs::read_to_string(path)
        .ok()
        .map(|content| decode_warning_count(&content))
        .unwrap_or(0)
}

pub(crate) fn mcp_resource_manifest_summary(
    path: &Path,
    include_retention: bool,
    separator: &str,
) -> Option<String> {
    let manifest = McpResourceManifest::from_path(path)?;
    let parts = manifest.summary_parts(include_retention);
    (!parts.is_empty()).then(|| parts.join(separator))
}

pub(crate) fn mcp_resource_manifest_badges(path: &Path) -> Vec<(String, String)> {
    let Some(manifest) = McpResourceManifest::from_path(path) else {
        return Vec::new();
    };
    let mut badges = Vec::new();
    if let Some(server) = manifest.server {
        badges.push(("server".to_string(), server));
    }
    if let Some(blobs) = manifest.blobs {
        badges.push(("blobs".to_string(), blobs));
    }
    badges.push((
        "decode".to_string(),
        if manifest.decode_warnings == 0 {
            "ok".to_string()
        } else {
            format!("warnings={}", manifest.decode_warnings)
        },
    ));
    badges
}

pub(crate) fn render_mcp_resource_artifact_index(candidates: &[PathBuf]) -> String {
    let mut lines = vec![
        "# MCP Resource Artifacts".to_string(),
        String::new(),
        format!("- Files: {}", candidates.len()),
        String::new(),
        "## Resources".to_string(),
        String::new(),
    ];
    let mut listed = 0usize;
    for path in candidates
        .iter()
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
    {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("mcp-resource.md");
        let summary = mcp_resource_manifest_summary(path, true, " · ")
            .unwrap_or_else(|| "summary=unavailable".to_string());
        lines.push(format!("- {} · {}", name, summary));
        listed += 1;
    }
    if listed == 0 {
        lines.push("- none".to_string());
    }
    lines.push(String::new());
    lines.push("## Cleanup".to_string());
    lines.push(String::new());
    lines.push("- /mcp resources cleanup [keep=N|all]".to_string());
    lines.join("\n")
}

impl McpResourceManifest {
    fn from_path(path: &Path) -> Option<Self> {
        if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
            return None;
        }
        let content = std::fs::read_to_string(path).ok()?;
        Some(Self::from_content(&content))
    }

    fn from_content(content: &str) -> Self {
        Self {
            server: manifest_field(content, "Server"),
            uri: manifest_field(content, "URI"),
            blobs: manifest_field(content, "Blob count"),
            retention: manifest_field(content, "Retention"),
            decode_warnings: decode_warning_count(content),
        }
    }

    fn summary_parts(&self, include_retention: bool) -> Vec<String> {
        let mut parts = Vec::new();
        if let Some(server) = self.server.as_deref() {
            parts.push(format!("server={}", server));
        }
        if let Some(uri) = self.uri.as_deref() {
            parts.push(format!("uri={}", uri));
        }
        if let Some(blobs) = self.blobs.as_deref() {
            parts.push(format!("blobs={}", blobs));
        }
        if self.decode_warnings > 0 {
            parts.push(format!("decode_warnings={}", self.decode_warnings));
        }
        if include_retention {
            if let Some(retention) = self.retention.as_deref() {
                parts.push(format!("retention={}", retention));
            }
        }
        parts
    }
}

fn decode_warning_count(content: &str) -> usize {
    content
        .lines()
        .filter(|line| line.trim_start().starts_with("- Decode warning:"))
        .count()
}

fn manifest_field(content: &str, field: &str) -> Option<String> {
    let prefix = format!("- {}: ", field);
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix).map(str::trim))
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{
        mcp_resource_manifest_badges, mcp_resource_manifest_summary,
        render_mcp_resource_artifact_index,
    };

    #[test]
    fn manifest_summary_and_badges_extract_common_fields() {
        let dir = std::env::temp_dir().join(format!(
            "yode-mcp-resource-artifact-helper-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("artifact.md");
        std::fs::write(
            &path,
            "# MCP Resource Blob Artifact\n\n- Server: demo\n- URI: mcp://image\n- Blob count: 2\n- Retention: keep newest 120 artifact files\n\n## Blob 1\n\n- Decode warning: invalid base64\n",
        )
        .unwrap();

        let summary = mcp_resource_manifest_summary(&path, true, " · ").unwrap();
        assert!(summary.contains("server=demo"));
        assert!(summary.contains("uri=mcp://image"));
        assert!(summary.contains("blobs=2"));
        assert!(summary.contains("decode_warnings=1"));
        assert!(summary.contains("retention=keep newest 120 artifact files"));

        let badges = mcp_resource_manifest_badges(&path);
        assert!(badges.contains(&("server".to_string(), "demo".to_string())));
        assert!(badges.contains(&("blobs".to_string(), "2".to_string())));
        assert!(badges.contains(&("decode".to_string(), "warnings=1".to_string())));

        let index = render_mcp_resource_artifact_index(&[path]);
        assert!(index.contains("- Files: 1"));
        assert!(index.contains("server=demo"));
        assert!(index.contains("/mcp resources cleanup"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn artifact_index_handles_decoded_files_without_manifest() {
        let path = PathBuf::from("/tmp/resource.png");
        let index = render_mcp_resource_artifact_index(&[path]);
        assert!(index.contains("- Files: 1"));
        assert!(index.contains("- none"));
    }
}
