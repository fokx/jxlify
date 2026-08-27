use crate::config::JxlifyConfig;
use crate::helper::hash_string;
use std::path::PathBuf;
use tracing::debug;

/// Fetch remote image from target URL and cache to REMOTE_RAW_PATH
pub async fn fetch_remote_image(
    client: &reqwest::Client,
    remote_url: &str,
    subdir: &str,
    config: &JxlifyConfig,
) -> Result<PathBuf, String> {
    let url_hash = hash_string(remote_url);
    let ext = crate::helper::get_extension(remote_url);

    let local_dest = crate::helper::get_sharded_path(
        &config.remote_cache_path(),
        subdir,
        &url_hash,
        &ext,
    );

    if local_dest.exists() {
        return Ok(local_dest);
    }

    debug!("Fetching remote upstream image: {}", remote_url);
    let response = client
        .get(remote_url)
        .send()
        .await
        .map_err(|e| format!("Remote fetch failed for {}: {}", remote_url, e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Remote server returned HTTP status {} for {}",
            response.status(),
            remote_url
        ));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body for {}: {}", remote_url, e))?;

    crate::helper::atomic_write(&local_dest, &bytes)
        .map_err(|e| format!("Failed to write remote image to disk: {}", e))?;

    Ok(local_dest)
}
