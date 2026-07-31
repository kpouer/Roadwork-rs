use crate::descriptor_manager::DescriptorManager;
use log::{error, info};
use roadwork_core::http_service::HttpService;
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;
use serde::Deserialize;
use std::collections::HashMap;
use std::fmt::format;

const GITHUB_RAW_PREFIX: &str =
    "https://raw.githubusercontent.com/kpouer/Roadwork-rs/main/opendata/json";

struct RoadworkIndex<'a> {
    prefix: &'a str,
    index_url: String,
}

impl<'a> RoadworkIndex<'a> {
    fn new(prefix: &'a str) -> Self {
        Self {
            prefix,
            index_url: format!("{GITHUB_RAW_PREFIX}/index.json"),
        }
    }

    async fn get_index(&self) -> Result<IndexFile, String> {
        let index_str = HttpService
            .get_url(&self.index_url)
            .await
            .map_err(|e| format!("get index: {e}"))?;
        let index: IndexFile =
            serde_json::from_str(&index_str).map_err(|e| format!("parse index: {e}"))?;
        Ok(index)
    }

    async fn get_descriptors(&self) -> Result<HashMap<String, ServiceDescriptor>, String> {
        let mut descriptors = HashMap::new();
        let index = self.get_index().await?;
        for file in index.files {
            let path = &file.path;
            if path.contains("..") || path.starts_with('/') {
                error!("Skipping invalid path: {path}");
                continue;
            }
            let url = format!("{GITHUB_RAW_PREFIX}/{path}");
            info!("Downloading {path}");
            match HttpService.get_url(&url).await {
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
        Ok(descriptors)
    }
}

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
    let roadwork_index = RoadworkIndex::new(GITHUB_RAW_PREFIX);
    let descriptors = roadwork_index.get_descriptors()?;

    descriptor_manager.save_descriptors(&descriptors);
    Ok(())
}
