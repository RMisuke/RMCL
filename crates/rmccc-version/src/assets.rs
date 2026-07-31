//! Minecraft asset resolution.
//!
//! Downloads and parses the asset index, then generates download items
//! for each individual asset file (mapped from hash → path).

use rmccc_download::DownloadItem;
use rmccc_error::{RmcccError, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// Parsed asset index (from the JSON at asset_index.url).
#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexContent {
    pub objects: HashMap<String, AssetObject>,
}

/// A single asset in the index.
#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

/// Resolve assets into download items.
///
/// The asset URL pattern is: `https://resources.download.minecraft.net/{hash_2char}/{full_hash}`
/// Each asset is stored at: `assets/objects/{hash_2char}/{full_hash}`
pub async fn resolve_asset_items(
    client: &reqwest::Client,
    asset_index: &super::types::AssetIndex,
    assets_dir: &Path,
) -> Result<Vec<DownloadItem>> {
    // Download the asset index JSON
    let resp = client
        .get(&asset_index.url)
        .send()
        .await
        .map_err(|e| RmcccError::Network(e.to_string()))?;
    let body = resp
        .bytes()
        .await
        .map_err(|e| RmcccError::Network(e.to_string()))?;

    // Verify SHA1
    let actual_sha1 = sha1_bytes(&body);
    if actual_sha1.to_lowercase() != asset_index.sha1.to_lowercase() {
        return Err(RmcccError::HashMismatch {
            path: asset_index.url.clone(),
            expected: asset_index.sha1.clone(),
            actual: actual_sha1,
        });
    }

    // ── Save the asset index JSON to assets/indexes/{id}.json ──
    // Bug #8: The index file was not being persisted, causing `resource check`
    // to always report "资源索引文件缺失".
    let indexes_dir = assets_dir.join("indexes");
    std::fs::create_dir_all(&indexes_dir)?;
    let index_path = indexes_dir.join(format!("{}.json", asset_index.id));
    std::fs::write(&index_path, &body)?;
    tracing::debug!(
        path = %index_path.display(),
        size = body.len(),
        "Asset index saved"
    );

    let content: AssetIndexContent = serde_json::from_slice(&body)?;

    let items: Vec<DownloadItem> = content
        .objects
        .iter()
        .map(|(_name, obj)| {
            let hash_prefix = &obj.hash[..2];
            let url = format!(
                "https://resources.download.minecraft.net/{}/{}",
                hash_prefix, obj.hash
            );
            let dest = assets_dir
                .join("objects")
                .join(hash_prefix)
                .join(&obj.hash);

            DownloadItem {
                url,
                dest,
                expected_sha1: Some(obj.hash.clone()),
                size: Some(obj.size),
                use_cache: true,
                candidates: Vec::new(),
            }
        })
        .collect();

    tracing::info!(
        count = items.len(),
        total_size = items.iter().filter_map(|i| i.size).sum::<u64>(),
        "Resolved asset download items"
    );

    Ok(items)
}

fn sha1_bytes(data: &[u8]) -> String {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
