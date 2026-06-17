use std::path::{Path, PathBuf};

use crate::protocol::LicenseNotice;

pub(super) fn read_license_notices(workspace_path: &Path) -> Vec<LicenseNotice> {
    let root = find_workspace_root(workspace_path).unwrap_or_else(|| workspace_path.to_path_buf());
    let mut notices = vec![LicenseNotice {
        name: "yode".to_string(),
        version: Some(env!("CARGO_PKG_VERSION").to_string()),
        license: Some("MIT".to_string()),
        source: "workspace".to_string(),
    }];

    let cargo_lock = root.join("Cargo.lock");
    if let Ok(lock) = std::fs::read_to_string(&cargo_lock) {
        notices.extend(parse_cargo_lock_notices(&lock));
    }
    let package_lock = root.join("apps/yode-desktop/pnpm-lock.yaml");
    if let Ok(lock) = std::fs::read_to_string(&package_lock) {
        notices.extend(parse_pnpm_lock_notices(&lock));
    }
    notices.sort_by(|a, b| a.name.cmp(&b.name).then(a.version.cmp(&b.version)));
    notices.dedup_by(|a, b| a.name == b.name && a.version == b.version && a.source == b.source);
    notices
}

fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut current = start;
    loop {
        if is_cargo_workspace_root(current) {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

fn is_cargo_workspace_root(path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(path.join("Cargo.toml")) else {
        return false;
    };
    content.contains("[workspace]") && content.contains("apps/yode-desktop/src-tauri")
}

fn parse_cargo_lock_notices(lock: &str) -> Vec<LicenseNotice> {
    let mut notices = Vec::new();
    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    for line in lock.lines() {
        let trimmed = line.trim();
        if trimmed == "[[package]]" {
            if let Some(package_name) = name.take() {
                notices.push(LicenseNotice {
                    name: package_name,
                    version: version.take(),
                    license: None,
                    source: "Cargo.lock".to_string(),
                });
            }
        } else if let Some(value) = trimmed.strip_prefix("name = ") {
            name = Some(value.trim_matches('"').to_string());
        } else if let Some(value) = trimmed.strip_prefix("version = ") {
            version = Some(value.trim_matches('"').to_string());
        }
    }
    if let Some(package_name) = name.take() {
        notices.push(LicenseNotice {
            name: package_name,
            version,
            license: None,
            source: "Cargo.lock".to_string(),
        });
    }
    notices
}

fn parse_pnpm_lock_notices(lock: &str) -> Vec<LicenseNotice> {
    lock.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with('/') || !trimmed.ends_with(':') {
                return None;
            }
            let package = trimmed.trim_start_matches('/').trim_end_matches(':');
            let (name, version) = package.rsplit_once('@')?;
            if name.is_empty() || version.is_empty() {
                return None;
            }
            Some(LicenseNotice {
                name: name.to_string(),
                version: Some(version.to_string()),
                license: None,
                source: "pnpm-lock.yaml".to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cargo_lock_parser_collects_package_names_and_versions() {
        let notices = parse_cargo_lock_notices(
            r#"
[[package]]
name = "anyhow"
version = "1.0.0"

[[package]]
name = "serde"
version = "1.0.0"
"#,
        );

        assert_eq!(notices.len(), 2);
        assert_eq!(notices[0].name, "anyhow");
        assert_eq!(notices[0].version.as_deref(), Some("1.0.0"));
        assert_eq!(notices[1].name, "serde");
    }

    #[test]
    fn pnpm_lock_parser_handles_scoped_packages() {
        let notices = parse_pnpm_lock_notices(
            r#"
/@tauri-apps/api@2.9.0:
  resolution: {}
/vite@5.4.21:
  resolution: {}
"#,
        );

        assert_eq!(notices.len(), 2);
        assert_eq!(notices[0].name, "@tauri-apps/api");
        assert_eq!(notices[0].version.as_deref(), Some("2.9.0"));
        assert_eq!(notices[1].name, "vite");
    }
}
