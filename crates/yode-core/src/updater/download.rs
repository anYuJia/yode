use super::*;

/// Download a file with stall detection
pub(in crate::updater) async fn download_with_stall_detection(
    url: &str,
    filepath: &PathBuf,
) -> Result<u64> {
    use tokio::io::AsyncWriteExt;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(300))
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

    let mut file = fs::File::create(filepath)
        .await
        .context("Failed to create download file")?;

    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_progress = SystemTime::now();

    use futures::StreamExt;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("Failed to read download chunk")?;

        file.write_all(&chunk)
            .await
            .context("Failed to write to download file")?;

        downloaded += chunk.len() as u64;

        let now = SystemTime::now();
        if now.duration_since(last_progress).unwrap().as_millis() > STALL_TIMEOUT_MS as u128 {
            anyhow::bail!(
                "Download stalled: no data received for {}ms",
                STALL_TIMEOUT_MS
            );
        }
        last_progress = now;
    }

    file.flush().await?;

    Ok(downloaded)
}
