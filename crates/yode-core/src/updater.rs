mod config_state;
mod download;
mod versioning;

use anyhow::{Context, Result};
use flate2;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tar;
use tempfile;
use tokio::fs;
use tracing::{error, info, warn};
use walkdir;

pub use self::versioning::{latest_local_release_tag, release_version_matches_tag};

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/anYuJia/yode/releases/latest";
const STALL_TIMEOUT_MS: u64 = 60_000;
const MAX_DOWNLOAD_RETRIES: u32 = 3;
const LOCK_TIMEOUT_MS: u64 = 5 * 60 * 1_000;
const VERSION_RETENTION_COUNT: usize = 2;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Another update process is running (lock held by PID {0})")]
    LockHeld(u32),
    #[error("Download stalled: no data received for {0}ms")]
    StallTimeout(u64),
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub name: String,
    pub body: String,
    pub html_url: String,
    pub published_at: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Clone)]
pub struct UpdateCheckResult {
    pub is_newer: bool,
    pub latest_version: String,
    pub release_notes: String,
    pub download_url: String,
    pub published_at: String,
}

pub struct Updater {
    config_dir: PathBuf,
    #[allow(dead_code)]
    auto_check: bool,
    #[allow(dead_code)]
    auto_download: bool,
}

impl Updater {
    pub fn new(config_dir: PathBuf, auto_check: bool, auto_download: bool) -> Self {
        Self {
            config_dir,
            auto_check,
            auto_download,
        }
    }

    pub async fn check_for_updates(&self) -> Result<Option<UpdateCheckResult>> {
        if !self.auto_check {
            info!("Auto-check is disabled, skipping update check");
            return Ok(None);
        }

        if let Some(last_checked) = self.get_last_checked() {
            if SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - last_checked
                < 3600
            {
                info!("Update check skipped (checked within 1 hour)");
                return Ok(None);
            }
        }

        info!("Checking for updates...");

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(format!("yode/{}", CURRENT_VERSION))
            .build()
            .context("Failed to create HTTP client")?;

        let response = client
            .get(GITHUB_RELEASES_API)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .context("Failed to fetch releases")?;

        if !response.status().is_success() {
            warn!("GitHub API returned status: {}", response.status());
            return Ok(None);
        }

        let release: GitHubRelease = response
            .json()
            .await
            .context("Failed to parse release response")?;

        let latest_version = release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name)
            .to_string();

        let is_newer = versioning::is_version_newer(CURRENT_VERSION, &latest_version);
        self.update_last_checked().await;

        if is_newer {
            info!(
                "New version available: {} (current: {})",
                latest_version, CURRENT_VERSION
            );

            let download_url = self.find_download_url(&release.assets);

            Ok(Some(UpdateCheckResult {
                is_newer: true,
                latest_version,
                release_notes: release.body,
                download_url,
                published_at: release.published_at,
            }))
        } else {
            info!("Already on latest version: {}", latest_version);
            Ok(None)
        }
    }

    fn find_download_url(&self, assets: &[ReleaseAsset]) -> String {
        let target = versioning::get_target_triple();

        if let Some(asset) = assets.iter().find(|a| a.name.contains(&target)) {
            return asset.browser_download_url.clone();
        }

        assets
            .iter()
            .find(|a| a.name.ends_with(".tar.gz") || a.name.ends_with(".zip"))
            .map(|a| a.browser_download_url.clone())
            .unwrap_or_else(|| "https://github.com/anYuJia/yode/releases".to_string())
    }

    pub async fn download_update(&self, update: &UpdateCheckResult) -> Result<PathBuf> {
        info!("Downloading update from: {}", update.download_url);

        match self.acquire_lock().await {
            Ok(_) => {}
            Err(UpdateError::LockHeld(pid)) => {
                anyhow::bail!(
                    "Another update process is running (lock held by PID {})",
                    pid
                );
            }
            Err(e) => return Err(e.into()),
        }

        let downloads_dir = self.config_dir.join("downloads");
        fs::create_dir_all(&downloads_dir).await?;

        let filename = update
            .download_url
            .split('/')
            .next_back()
            .unwrap_or("yode-update.tar.gz");
        let filepath = downloads_dir.join(filename);

        let mut last_error = None;

        for attempt in 1..=MAX_DOWNLOAD_RETRIES {
            match download::download_with_stall_detection(&update.download_url, &filepath).await {
                Ok(size) => {
                    info!("Update downloaded successfully: {} bytes", size);

                    if let Err(e) = self.cleanup_old_versions().await {
                        warn!("Failed to cleanup old versions: {}", e);
                    }

                    self.mark_as_downloaded(&update.latest_version).await;
                    let _ = self.release_lock().await;
                    return Ok(filepath);
                }
                Err(e) => {
                    warn!("Download attempt {} failed: {}", attempt, e);
                    last_error = Some(e);
                    let _ = fs::remove_file(&filepath).await;

                    if attempt < MAX_DOWNLOAD_RETRIES {
                        info!("Retrying download in 1 second...");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }

        let _ = self.release_lock().await;

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Download failed after all retries")))
    }

    pub fn get_downloaded_update_path(&self) -> Option<PathBuf> {
        let downloads_dir = self.config_dir.join("downloads");
        if downloads_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&downloads_dir) {
                let mut latest: Option<(SystemTime, PathBuf)> = None;
                for entry in entries.flatten() {
                    if let Ok(metadata) = entry.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if latest.is_none() || modified > latest.as_ref().unwrap().0 {
                                latest = Some((modified, entry.path()));
                            }
                        }
                    }
                }
                return latest.map(|(_, path)| path);
            }
        }
        None
    }

    pub fn has_pending_update(&self) -> bool {
        self.get_downloaded_update_path().is_some()
    }

    pub fn apply_downloaded_update(&self) -> Result<bool> {
        let update_path = match self.get_downloaded_update_path() {
            Some(path) => path,
            None => return Ok(false),
        };

        info!("Applying update from: {:?}", update_path);

        let temp_dir = tempfile::Builder::new()
            .prefix("yode-update")
            .tempdir()
            .context("Failed to create temporary directory for update")?;

        let file = std::fs::File::open(&update_path)
            .context(format!("Failed to open update file: {:?}", update_path))?;

        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);

        archive.set_unpack_xattrs(false);
        archive.set_preserve_permissions(true);

        if let Err(e) = archive.unpack(temp_dir.path()) {
            warn!(
                "Failed to unpack update archive to {:?}: {}",
                temp_dir.path(),
                e
            );
            warn!("This is often due to macOS permission restrictions or file format mismatch. Skipping update.");
            return Ok(false);
        }

        let mut new_bin_path = None;

        for entry in std::fs::read_dir(temp_dir.path())? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str == "yode" || name_str == "yode.exe" {
                new_bin_path = Some(entry.path());
                break;
            }
        }

        let new_bin_path = if let Some(path) = new_bin_path {
            path
        } else {
            let mut found = None;
            for entry in walkdir::WalkDir::new(temp_dir.path()) {
                let entry = entry.context("Failed to traverse update contents")?;
                let name = entry.file_name().to_string_lossy();
                if (name == "yode" || name == "yode.exe") && entry.file_type().is_file() {
                    found = Some(entry.path().to_path_buf());
                    break;
                }
            }
            match found {
                Some(p) => p,
                None => anyhow::bail!("Could not find 'yode' binary in update archive"),
            }
        };

        info!("Found new binary at: {:?}", new_bin_path);

        let current_exe =
            std::env::current_exe().context("Failed to get current executable path")?;
        let old_exe = current_exe.with_extension("old");

        if old_exe.exists() {
            let _ = std::fs::remove_file(&old_exe);
        }

        std::fs::rename(&current_exe, &old_exe)
            .context("Failed to move current binary to backup path")?;

        if let Err(e) = std::fs::copy(&new_bin_path, &current_exe) {
            error!("Failed to copy new binary: {}. Rolling back...", e);
            let _ = std::fs::rename(&old_exe, &current_exe);
            return Err(e.into());
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&current_exe) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&current_exe, perms);
            }
        }

        let _ = std::fs::remove_file(&old_exe);
        let _ = std::fs::remove_file(&update_path);

        info!("Update applied successfully. Version updated to latest.");
        Ok(true)
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(in crate::updater) struct UpdaterConfig {
    last_checked: Option<u64>,
    downloaded_version: Option<String>,
}

#[cfg(test)]
mod tests;
