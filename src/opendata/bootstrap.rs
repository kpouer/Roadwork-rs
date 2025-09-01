use crate::service::http_service::HttpService;
use log::{error, info, warn};
use serde::Deserialize;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use crate::OPENDATA_FOLDER;

const GITHUB_RAW_PREFIX: &str = "https://raw.githubusercontent.com/kpouer/Roadwork-rs/main/opendata/json";

#[derive(Debug, Deserialize)]
struct IndexFile {
    files: Vec<IndexEntry>,
}

#[derive(Debug, Deserialize)]
struct IndexEntry {
    path: String,
}

pub(crate) fn ensure_opendata_available() {
    let opendata_folder = Path::new(OPENDATA_FOLDER);
    if opendata_folder.exists() {
        info!("Bootstrap skipped: {OPENDATA_FOLDER} already exists");
        return;
    }

    info!("Bootstrap: {OPENDATA_FOLDER} not found, downloading descriptors");
    if let Err(e) = bootstrap_download() {
        error!("Bootstrap failed: {e}");
        return;
    }

    if let Err(e) = fs::create_dir_all(opendata_folder) {
        warn!("Unable to create marker directory {OPENDATA_FOLDER}: {e}");
    }
}

fn bootstrap_download() -> Result<(), String> {
    let http = HttpService::default();

    // Download index.json from raw GitHub URL
    let index_url = format!("{GITHUB_RAW_PREFIX}/index.json");
    let index_str = http.get_url(&index_url).map_err(|e| format!("get index: {e}"))?;
    let index: IndexFile = serde_json::from_str(&index_str).map_err(|e| format!("parse index: {e}"))?;

    for file in index.files {
        let rel_path = sanitize_path(&file.path).ok_or_else(|| format!("invalid path in index: {}", file.path))?;
        let url = format!("{GITHUB_RAW_PREFIX}/{}", rel_path.to_string_lossy());
        // Map destination to data/json/...
        let dest_path = PathBuf::from(format!("{OPENDATA_FOLDER}/{}", file.path));
        info!("Downloading {} -> {}", rel_path.display(), dest_path.display());
        match http.get_url(&url) {
            Ok(content) => {
                if let Some(parent) = dest_path.parent() {
                    if let Err(e) = fs::create_dir_all(parent) {
                        return Err(format!("create dir {}: {e}", parent.display()));
                    }
                }
                let mut f = File::create(&dest_path).map_err(|e| format!("create {}: {e}", dest_path.display()))?;
                f.write_all(content.as_bytes())
                    .map_err(|e| format!("write {}: {e}", dest_path.display()))?;
            }
            Err(e) => return Err(format!("download {}: {e}", url)),
        }
    }

    Ok(())
}

// Ensure the path is relative and does not escape the working directory
fn sanitize_path(p: &str) -> Option<PathBuf> {
    let path = Path::new(p);
    if path.is_absolute() {
        return None;
    }
    let mut cleaned = PathBuf::new();
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir => return None,
            _ => cleaned.push(comp.as_os_str()),
        }
    }
    Some(cleaned)
}
