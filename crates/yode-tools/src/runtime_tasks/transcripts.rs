use std::path::Path;

pub fn latest_transcript_artifact_path(project_root: &Path) -> Option<String> {
    let dir = project_root.join(".yode").join("transcripts");
    let mut entries = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| transcript_sort_key(b).cmp(&transcript_sort_key(a)));
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
