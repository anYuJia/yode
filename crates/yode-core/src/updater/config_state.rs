use super::*;

impl Updater {
    fn config_path(&self) -> PathBuf {
        self.config_dir.join("updater.toml")
    }

    pub(in crate::updater) async fn update_last_checked(&self) {
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

    pub(in crate::updater) fn get_last_checked(&self) -> Option<u64> {
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

    pub(in crate::updater) async fn mark_as_downloaded(&self, version: &str) {
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

    #[allow(dead_code)]
    fn lock_path(&self) -> PathBuf {
        self.config_dir.join(".update.lock")
    }

    #[allow(dead_code)]
    pub(in crate::updater) async fn acquire_lock(&self) -> Result<bool, UpdateError> {
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

    #[allow(dead_code)]
    pub(in crate::updater) async fn release_lock(&self) -> Result<()> {
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

    #[allow(dead_code)]
    pub(in crate::updater) async fn cleanup_old_versions(&self) -> Result<()> {
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
