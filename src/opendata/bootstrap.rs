use crate::database::RoadworkDb;
use crate::service::http_service::HttpService;
use log::{error, info};
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

pub(crate) async fn ensure_opendata_available(db: &RoadworkDb) {
    let existing = db.load_descriptors().await;
    if !existing.is_empty() {
        info!(
            "Bootstrap skipped: {} descriptors already in IndexedDB",
            existing.len()
        );
        return;
    }

    info!("Bootstrap: no descriptors found, downloading from GitHub");
    match bootstrap_download(db).await {
        Ok(()) => info!("Bootstrap completed successfully"),
        Err(e) => error!("Bootstrap failed: {e}"),
    }
}

async fn bootstrap_download(db: &RoadworkDb) -> Result<(), String> {
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
                match serde_json::from_str::<
                    crate::opendata::json::model::service_descriptor::ServiceDescriptor,
                >(&content)
                {
                    Ok(descriptor) => {
                        descriptors.insert(name, descriptor);
                    }
                    Err(e) => error!("Failed to parse {path}: {e}"),
                }
            }
            Err(e) => error!("Failed to download {path}: {e}"),
        }
    }

    db.save_descriptors(&descriptors).await;
    Ok(())
}
