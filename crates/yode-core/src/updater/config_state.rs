use super::*;

impl Updater {
    fn config_path(&self) -> PathBuf {
        self.config_dir.join("updater.toml")
    }

    async fn read_config(&self) -> UpdaterConfig {
        let config_path = self.config_path();
        if !fs::try_exists(&config_path).await.unwrap_or(false) {
            return UpdaterConfig::default();
        }

        fs::read_to_string(&config_path)
            .await
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    async fn write_config(&self, config: &UpdaterConfig, label: &str) {
        match toml::to_string(config) {
            Ok(toml_str) => {
                if let Err(err) = fs::write(self.config_path(), toml_str).await {
                    warn!("Failed to write updater config after {}: {}", label, err);
                }
            }
            Err(err) => warn!(
                "Failed to serialize updater config after {}: {}",
                label, err
            ),
        }
    }

    pub(in crate::updater) async fn update_last_checked(&self) {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut config = self.read_config().await;
        config.last_checked = Some(timestamp);
        self.write_config(&config, "last-check update").await;
    }

    pub(in crate::updater) async fn get_last_checked(&self) -> Option<u64> {
        self.read_config().await.last_checked
    }

    pub(in crate::updater) async fn mark_as_downloaded(&self, version: &str) {
        let mut config = self.read_config().await;
        config.downloaded_version = Some(version.to_string());
        self.write_config(&config, "download marker update").await;
    }

    fn lock_path(&self) -> PathBuf {
        self.config_dir.join(".update.lock")
    }

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
                                .unwrap_or_default()
                                .as_millis();
                            if age_ms < LOCK_TIMEOUT_MS as u128 {
                                return Err(UpdateError::LockHeld(pid));
                            }
                        }
                    }
                }
            }
        }

        if let Err(err) = fs::write(&lock_path, std::process::id().to_string()).await {
            warn!("Failed to write updater lock file: {}", err);
        }
        Ok(true)
    }

    pub(in crate::updater) async fn release_lock(&self) -> Result<()> {
        let lock_path = self.lock_path();
        if lock_path.exists() {
            if let Ok(content) = fs::read_to_string(&lock_path).await {
                if let Ok(pid) = content.trim().parse::<u32>() {
                    if pid == std::process::id() {
                        if let Err(err) = fs::remove_file(&lock_path).await {
                            warn!("Failed to remove updater lock file: {}", err);
                        }
                    }
                }
            }
        }
        Ok(())
    }

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

        versions.sort_by_key(|b| std::cmp::Reverse(b.0));

        if versions.len() > VERSION_RETENTION_COUNT {
            for (_, path) in versions.iter().skip(VERSION_RETENTION_COUNT) {
                match fs::remove_file(path).await {
                    Ok(()) => info!("Cleaned up old version: {:?}", path),
                    Err(err) => warn!("Failed to cleanup old version {:?}: {}", path, err),
                }
            }
        }

        Ok(())
    }
}
