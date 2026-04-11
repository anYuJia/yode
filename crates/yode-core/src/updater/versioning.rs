pub(in crate::updater) fn is_version_newer(current: &str, latest: &str) -> bool {
    let current_parts: Vec<u32> = parse_version(current);
    let latest_parts: Vec<u32> = parse_version(latest);

    for (c, l) in current_parts.iter().zip(latest_parts.iter()) {
        if l > c {
            return true;
        } else if l < c {
            return false;
        }
    }

    latest_parts.len() > current_parts.len()
}

pub(in crate::updater) fn parse_version(version: &str) -> Vec<u32> {
    version.split('.').filter_map(|s| s.parse().ok()).collect()
}

pub fn latest_local_release_tag() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["tag", "--list", "v*", "--sort=-version:refname"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.to_string())
}

pub fn release_version_matches_tag(tag: &str, version: &str) -> bool {
    tag.strip_prefix('v').unwrap_or(tag) == version
}

pub(in crate::updater) fn get_target_triple() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    match (os, arch) {
        ("macos", "aarch64") => "aarch64-apple-darwin".to_string(),
        ("macos", "x86_64") => "x86_64-apple-darwin".to_string(),
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu".to_string(),
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu".to_string(),
        ("windows", "x86_64") => "x86_64-pc-windows-msvc".to_string(),
        _ => format!("{}-{}", arch, os),
    }
}
