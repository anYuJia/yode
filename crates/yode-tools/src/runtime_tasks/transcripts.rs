use std::path::Path;

pub async fn latest_transcript_artifact_path(project_root: &Path) -> Option<String> {
    let dir = project_root.join(".yode").join("transcripts");
    let mut read_dir = tokio::fs::read_dir(&dir).await.ok()?;
    let mut entries = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            entries.push(path);
        }
    }
    entries.sort_by_key(|path| std::cmp::Reverse(transcript_sort_key(path)));
    entries
        .into_iter()
        .next()
        .map(|path| path.display().to_string())
}

fn transcript_sort_key(path: &Path) -> (Option<String>, String) {
    let timestamp = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .and_then(|stem| {
            stem.rsplit_once("-compact-")
                .map(|(_, timestamp)| timestamp)
        })
        .map(str::to_string);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();
    (timestamp, file_name)
}
