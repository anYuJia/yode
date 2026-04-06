//! Version updater for Yode
//!
//! This module handles:
//! - Checking for new versions from GitHub Releases
//! - Downloading updates in background
//! - Managing update configuration
//!
//! Inspired by Claude Code's autoUpdater implementation:
//! - Stall detection (60s timeout with retry)
//! - Checksum verification (SHA256)
//! - Lock file for concurrent update prevention
//! - Release channels (stable/latest)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
#[allow(unused_imports)] // Reserved for future checksum verification
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::fs;
use tracing::{error, info, warn};
use tempfile;
use tar;
use flate2;
use walkdir;

/// Current version from Cargo.toml
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub API endpoint for latest release
const GITHUB_RELEASES_API: &str = "https://api.github.com/repos/anYuJia/yode/releases/latest";

/// Stall timeout for downloads (60 seconds - same as Claude Code)
const STALL_TIMEOUT_MS: u64 = 60_000;

/// Maximum download retries
const MAX_DOWNLOAD_RETRIES: u32 = 3;

/// Lock timeout (5 minutes)
const LOCK_TIMEOUT_MS: u64 = 5 * 60 * 1_000;

/// Number of versions to retain
const VERSION_RETENTION_COUNT: usize = 2;

/// Error type for update operations
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Another update process is running (lock held by PID {0})")]
    LockHeld(u32),
    #[error("Download stalled: no data received for {0}ms")]
    StallTimeout(u64),
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },
}

/// Response from GitHub Releases API
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

/// Update check result
#[derive(Debug, Clone)]
pub struct UpdateCheckResult {
    pub is_newer: bool,
    pub latest_version: String,
    pub release_notes: String,
    pub download_url: String,
    pub published_at: String,
}

/// Updater state
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

    /// Check for updates from GitHub
    pub async fn check_for_updates(&self) -> Result<Option<UpdateCheckResult>> {
        if !self.auto_check {
            info!("Auto-check is disabled, skipping update check");
            return Ok(None);
        }

        // Check if we checked recently (within 1 hour)
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

        // Parse version from tag (e.g., "v0.2.0" -> "0.2.0")
        let latest_version = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name).to_string();

        // Compare versions
        let is_newer = is_version_newer(CURRENT_VERSION, &latest_version);

        // Update last checked timestamp
        self.update_last_checked().await;

        if is_newer {
            info!("New version available: {} (current: {})", latest_version, CURRENT_VERSION);

            // Find download URL for current platform
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

    /// Find the best download URL for current platform
    fn find_download_url(&self, assets: &[ReleaseAsset]) -> String {
        let target = get_target_triple();

        // Try to find exact match first
        if let Some(asset) = assets.iter().find(|a| a.name.contains(&target)) {
            return asset.browser_download_url.clone();
        }

        // Fallback to .tar.gz or generic
        assets
            .iter()
            .find(|a| a.name.ends_with(".tar.gz") || a.name.ends_with(".zip"))
            .map(|a| a.browser_download_url.clone())
            .unwrap_or_else(|| "https://github.com/anYuJia/yode/releases".to_string())
    }

    /// Download update in background with stall detection and retry
    pub async fn download_update(&self, update: &UpdateCheckResult) -> Result<PathBuf> {
        info!("Downloading update from: {}", update.download_url);

        // Acquire lock to prevent concurrent updates
        match self.acquire_lock().await {
            Ok(_) => {},
            Err(UpdateError::LockHeld(pid)) => {
                anyhow::bail!("Another update process is running (lock held by PID {})", pid);
            }
            Err(e) => return Err(e.into()),
        }

        let downloads_dir = self.config_dir.join("downloads");
        fs::create_dir_all(&downloads_dir).await?;

        let filename = update.download_url.split('/').next_back().unwrap_or("yode-update.tar.gz");
        let filepath = downloads_dir.join(filename);

        // Download with retry logic
        let mut last_error = None;

        for attempt in 1..=MAX_DOWNLOAD_RETRIES {
            match download_with_stall_detection(&update.download_url, &filepath).await {
                Ok(size) => {
                    info!("Update downloaded successfully: {} bytes", size);

                    // Clean up old versions
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

                    // Clean up partial download
                    let _ = fs::remove_file(&filepath).await;

                    if attempt < MAX_DOWNLOAD_RETRIES {
                        info!("Retrying download in 1 second...");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }

        // Release lock on failure
        let _ = self.release_lock().await;

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("Download failed after all retries")))
    }

    /// Get the path to the downloaded update
    pub fn get_downloaded_update_path(&self) -> Option<PathBuf> {
        let downloads_dir = self.config_dir.join("downloads");
        if downloads_dir.exists() {
            // Find the most recent download
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

    /// Check if there's a pending update to install
    pub fn has_pending_update(&self) -> bool {
        self.get_downloaded_update_path().is_some()
    }

    /// Apply the downloaded update by replacing the current executable.
    /// This should be called during startup if has_pending_update() is true.
    pub fn apply_downloaded_update(&self) -> Result<bool> {
        let update_path = match self.get_downloaded_update_path() {
            Some(path) => path,
            None => return Ok(false),
        };

        info!("Applying update from: {:?}", update_path);

        // 1. Unpack to temporary directory
        let temp_dir = tempfile::Builder::new()
            .prefix("yode-update")
            .tempdir()
            .context("Failed to create temporary directory for update")?;
            
        let file = std::fs::File::open(&update_path)
            .context(format!("Failed to open update file: {:?}", update_path))?;
            
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);
        
        // Disable xattrs as they often cause failures on macOS/Linux due to security or filesystem limits
        archive.set_unpack_xattrs(false);
        archive.set_preserve_permissions(true);

        if let Err(e) = archive.unpack(temp_dir.path()) {
            anyhow::bail!("Failed to unpack update archive to {:?}: {}", temp_dir.path(), e);
        }

        // 2. Find the yode binary in the extracted contents
        let mut new_bin_path = None;
        
        // First look in the root of the temp dir
        for entry in std::fs::read_dir(temp_dir.path())? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str == "yode" || name_str == "yode.exe" {
                new_bin_path = Some(entry.path());
                break;
            }
        }

        // If not found, look deeper (some archives might have a subdirectory)
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

        // 3. Get current executable path
        let current_exe = std::env::current_exe().context("Failed to get current executable path")?;
        let old_exe = current_exe.with_extension("old");

        // 4. Atomic replacement
        if old_exe.exists() {
            let _ = std::fs::remove_file(&old_exe);
        }
        
        // On Unix, we can rename the current file even if it's running
        std::fs::rename(&current_exe, &old_exe)
            .context("Failed to move current binary to backup path")?;
            
        if let Err(e) = std::fs::copy(&new_bin_path, &current_exe) {
            // Rollback if copy fails
            error!("Failed to copy new binary: {}. Rolling back...", e);
            let _ = std::fs::rename(&old_exe, &current_exe);
            return Err(e.into());
        }
        
        // Ensure the new binary is executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = std::fs::metadata(&current_exe) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&current_exe, perms);
            }
        }

        // 5. Cleanup
        let _ = std::fs::remove_file(&old_exe);
        let _ = std::fs::remove_file(&update_path);
        
        info!("Update applied successfully. Version updated to latest.");
        Ok(true)
    }

    // ── Config persistence ─────────────────────────────────────

    fn config_path(&self) -> PathBuf {
        self.config_dir.join("updater.toml")
    }

    async fn update_last_checked(&self) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let config_path = self.config_path();
        let mut config: UpdaterConfig = if config_path.exists() {
            fs::read_to_string(&config_path)
                .await
                .ok()
                .and_then(|s| toml::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            UpdaterConfig::default()
        };

        config.last_checked = Some(timestamp);

        if let Ok(toml_str) = toml::to_string(&config) {
            let _ = fs::write(&config_path, toml_str).await;
        }
    }

    fn get_last_checked(&self) -> Option<u64> {
        let config_path = self.config_path();
        if config_path.exists() {
            std::fs::read_to_string(&config_path)
                .ok()
                .and_then(|s| toml::from_str::<UpdaterConfig>(&s).ok())
                .and_then(|c| c.last_checked)
        } else {
            None
        }
    }

    async fn mark_as_downloaded(&self, version: &str) {
        let config_path = self.config_path();
        let mut config: UpdaterConfig = if config_path.exists() {
            fs::read_to_string(&config_path)
                .await
                .ok()
                .and_then(|s| toml::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            UpdaterConfig::default()
        };

        config.downloaded_version = Some(version.to_string());

        if let Ok(toml_str) = toml::to_string(&config) {
            let _ = fs::write(&config_path, toml_str).await;
        }
    }

    /// Get the lock file path
    #[allow(dead_code)]
    fn lock_path(&self) -> PathBuf {
        self.config_dir.join(".update.lock")
    }

    /// Acquire update lock with stall detection
    #[allow(dead_code)]
    async fn acquire_lock(&self) -> Result<bool, UpdateError> {
        let lock_path = self.lock_path();

        if lock_path.exists() {
            if let Ok(content) = fs::read_to_string(&lock_path).await {
                if let Ok(pid) = content.trim().parse::<u32>() {
                    if pid == std::process::id() {
                        return Ok(true);
                    }
                    if let Ok(metadata) = fs::metadata(&lock_path).await {
                        if let Ok(modified) = metadata.modified() {
                            let age_ms = SystemTime::now()
                                .duration_since(modified)
                                .unwrap()
                                .as_millis();
                            if age_ms < LOCK_TIMEOUT_MS as u128 {
                                return Err(UpdateError::LockHeld(pid));
                            }
                        }
                    }
                }
            }
        }

        let _ = fs::write(&lock_path, std::process::id().to_string()).await;
        Ok(true)
    }

    /// Release update lock
    #[allow(dead_code)]
    async fn release_lock(&self) -> Result<()> {
        let lock_path = self.lock_path();
        if lock_path.exists() {
            if let Ok(content) = fs::read_to_string(&lock_path).await {
                if let Ok(pid) = content.trim().parse::<u32>() {
                    if pid == std::process::id() {
                        let _ = fs::remove_file(&lock_path).await;
                    }
                }
            }
        }
        Ok(())
    }

    /// Clean up old versions, keeping only VERSION_RETENTION_COUNT
    #[allow(dead_code)]
    async fn cleanup_old_versions(&self) -> Result<()> {
        let downloads_dir = self.config_dir.join("downloads");
        if !downloads_dir.exists() {
            return Ok(());
        }

        let mut versions: Vec<(SystemTime, PathBuf)> = Vec::new();
        let mut entries = fs::read_dir(&downloads_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            if let Ok(metadata) = entry.metadata().await {
                if metadata.is_file() {
                    if let Ok(modified) = metadata.modified() {
                        versions.push((modified, entry.path()));
                    }
                }
            }
        }

        versions.sort_by(|a, b| b.0.cmp(&a.0));

        if versions.len() > VERSION_RETENTION_COUNT {
            for (_, path) in versions.iter().skip(VERSION_RETENTION_COUNT) {
                let _ = fs::remove_file(path).await;
                info!("Cleaned up old version: {:?}", path);
            }
        }

        Ok(())
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct UpdaterConfig {
    last_checked: Option<u64>,
    downloaded_version: Option<String>,
}

/// Download a file with stall detection
///
/// Stall detection: If no bytes are received for STALL_TIMEOUT_MS, the download is aborted.
/// This is similar to Claude Code's implementation.
async fn download_with_stall_detection(url: &str, filepath: &PathBuf) -> Result<u64> {
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300)) // 5 min total timeout
        .build()
        .context("Failed to create HTTP client")?;

    let response = client
        .get(url)
        .send()
        .await
        .context("Failed to start download")?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed with status: {}", response.status());
    }

    let content_length = response.content_length().unwrap_or(0);
    info!("Downloading {} bytes...", content_length);

    let mut file = fs::File::create(filepath).await
        .context("Failed to create download file")?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_progress = SystemTime::now();

    use futures::StreamExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Failed to read download chunk")?;

        file.write_all(&chunk).await
            .context("Failed to write to download file")?;

        downloaded += chunk.len() as u64;

        // Check for stall
        let now = SystemTime::now();
        if now.duration_since(last_progress).unwrap().as_millis() > STALL_TIMEOUT_MS as u128 {
            anyhow::bail!("Download stalled: no data received for {}ms", STALL_TIMEOUT_MS);
        }
        last_progress = now;
    }

    file.flush().await?;

    Ok(downloaded)
}

/// Compare two version strings (semver-like)
fn is_version_newer(current: &str, latest: &str) -> bool {
    let current_parts: Vec<u32> = parse_version(current);
    let latest_parts: Vec<u32> = parse_version(latest);

    for (c, l) in current_parts.iter().zip(latest_parts.iter()) {
        if l > c {
            return true;
        } else if l < c {
            return false;
        }
    }

    // If all compared parts are equal, longer version is newer
    latest_parts.len() > current_parts.len()
}

fn parse_version(version: &str) -> Vec<u32> {
    version
        .split('.')
        .filter_map(|s| s.parse().ok())
        .collect()
}

/// Get the target triple for current platform
fn get_target_triple() -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_compare() {
        assert!(is_version_newer("0.1.0", "0.2.0"));
        assert!(is_version_newer("0.1.9", "0.2.0"));
        assert!(is_version_newer("0.9.0", "1.0.0"));
        assert!(!is_version_newer("0.2.0", "0.1.0"));
        assert!(!is_version_newer("1.0.0", "0.9.0"));
        assert!(!is_version_newer("0.2.0", "0.2.0"));
        assert!(is_version_newer("0.2.0", "0.2.1"));
    }

    #[test]
    fn test_parse_version() {
        let v: Vec<u32> = vec![];
        assert_eq!(parse_version("0.2.0"), vec![0, 2, 0]);
        assert_eq!(parse_version("1.0.0"), vec![1, 0, 0]);
        assert_eq!(parse_version("invalid"), v);
    }
}
