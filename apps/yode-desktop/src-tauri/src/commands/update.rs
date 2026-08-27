use serde::Deserialize;

use crate::protocol;

const RELEASES_API: &str = "https://api.github.com/repos/anYuJia/yode/releases/latest";
const RELEASES_PAGE: &str = "https://github.com/anYuJia/yode/releases";

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    published_at: Option<String>,
}

#[tauri::command]
pub async fn check_for_updates() -> Result<Option<protocol::UpdateCheckResult>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(format!("yode-desktop/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|err| format!("创建更新检查客户端失败：{err}"))?;

    let response = client
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| format!("检查 GitHub Release 失败：{err}"))?
        .error_for_status()
        .map_err(|err| format!("GitHub Release 返回错误：{err}"))?;

    let release = response
        .json::<GitHubRelease>()
        .await
        .map_err(|err| format!("解析 GitHub Release 失败：{err}"))?;
    let latest = release.tag_name.trim_start_matches('v').to_string();

    if !version_is_newer(env!("CARGO_PKG_VERSION"), &latest) {
        return Ok(None);
    }

    Ok(Some(protocol::UpdateCheckResult {
        version: latest,
        release_url: if release.html_url.trim().is_empty() {
            RELEASES_PAGE.to_string()
        } else {
            release.html_url
        },
        published_at: release.published_at.unwrap_or_default(),
    }))
}

fn version_is_newer(current: &str, candidate: &str) -> bool {
    let Some(current) = parse_version(current) else {
        return false;
    };
    let Some(candidate) = parse_version(candidate) else {
        return false;
    };
    candidate > current
}

fn parse_version(raw: &str) -> Option<(u64, u64, u64)> {
    let stable = raw.trim().trim_start_matches('v').split('-').next()?;
    let mut parts = stable.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_version_comparison_is_monotonic() {
        assert!(version_is_newer("1.0.0", "1.0.1"));
        assert!(version_is_newer("1.9.9", "2.0.0"));
        assert!(!version_is_newer("1.0.0", "1.0.0"));
        assert!(!version_is_newer("1.2.0", "1.1.9"));
        assert!(!version_is_newer("bad", "1.0.1"));
    }
}
