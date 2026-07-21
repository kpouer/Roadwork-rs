use crate::descriptor_manager::DescriptorManager;
use log::{error, info};
use roadwork_core::http_service::HttpService;
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;
use serde::Deserialize;
use std::collections::HashMap;

const GITHUB_RAW_PREFIX: &str =
    "https://raw.githubusercontent.com/kpouer/Roadwork-rs/main/opendata/json";

#[derive(Debug, Deserialize)]
struct IndexFile {
    files: Vec<IndexEntry>,
}

#[derive(Debug, Deserialize)]
struct IndexEntry {
    path: String,
}

pub async fn ensure_descriptors_available(descriptor_manager: &DescriptorManager) {
    let existing = descriptor_manager.descriptor_names();
    if !existing.is_empty() {
        info!(
            "Bootstrap skipped: {} descriptors already available",
            existing.len()
        );
        return;
    }

    info!("Bootstrap: no descriptors found, downloading from GitHub");
    match bootstrap_download(descriptor_manager).await {
        Ok(()) => info!("Bootstrap completed successfully"),
        Err(e) => error!("Bootstrap failed: {e}"),
    }
}

async fn bootstrap_download(descriptor_manager: &DescriptorManager) -> Result<(), String> {
    let http = HttpService;

    let index_url = format!("{GITHUB_RAW_PREFIX}/index.json");
    let index_str = http
        .get_url(&index_url)
        .await
        .map_err(|e| format!("get index: {e}"))?;
    let index: IndexFile =
        serde_json::from_str(&index_str).map_err(|e| format!("parse index: {e}"))?;

    let mut descriptors = HashMap::new();

    for file in index.files {
        let path = &file.path;
        if path.contains("..") || path.starts_with('/') {
            error!("Skipping invalid path: {path}");
            continue;
        }
        let url = format!("{GITHUB_RAW_PREFIX}/{path}");
        info!("Downloading {path}");
        match http.get_url(&url).await {
            Ok(content) => {
                let name = path.strip_suffix(".json").unwrap_or(path).to_string();
                match serde_json::from_str::<ServiceDescriptor>(&content) {
                    Ok(descriptor) => {
                        descriptors.insert(name, descriptor);
                    }
                    Err(e) => error!("Failed to parse {path}: {e}"),
                }
            }
            Err(e) => error!("Failed to download {path}: {e}"),
        }
    }

    descriptor_manager.save_descriptors(&descriptors);
    Ok(())
}
