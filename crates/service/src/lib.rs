//! Roadwork service crate — shared access to the opendata service descriptors.
//!
//! The built-in descriptors from `opendata/roadwork/` are embedded at compile time
//! by `build.rs` and exposed through [`load_descriptors`] and [`get_services`].
//! This crate is used by the WME extension, the egui app and the webserver.

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use log::info;
use roadwork_core::http_service::HttpService;
use roadwork_core::model::roadwork_data::RoadworkData;
use roadwork_core::model::service_info::ServiceInfo;
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;
use roadwork_core::opendata::json::opendata_service::{OpendataError, OpendataService};
use roadwork_sync::SyncData;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Base URL for the GitHub-hosted opendata roadwork descriptors.
pub const INDEX_URL: &str = "https://raw.githubusercontent.com/kpouer/Roadwork-rs/refs/heads/main/opendata/roadwork/index.json";

/// Base URL for individual descriptor files (append the `path` from an [`IndexEntry`]).
pub const DESCRIPTORS_BASE_URL: &str =
    "https://raw.githubusercontent.com/kpouer/Roadwork-rs/refs/heads/main/opendata/roadwork/";

/// A remote index of available roadwork service descriptors.
#[derive(Debug, Clone, Deserialize)]
pub struct RoadworkIndex {
    pub version: u32,
    pub generated_at: Option<String>,
    pub count: Option<usize>,
    pub files: Vec<IndexEntry>,
}

/// One entry in the remote [`RoadworkIndex`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexEntry {
    pub key: String,
    pub path: String,
    pub country: Option<String>,
    pub name: Option<String>,
    pub size: Option<u64>,
    pub modified: Option<String>,
    pub sha256: Option<String>,
}

/// Fetches and parses the remote roadwork index from GitHub.
pub async fn fetch_index() -> Result<RoadworkIndex, ServiceError> {
    let http = HttpService;
    let json = http
        .get_url(INDEX_URL)
        .await
        .map_err(|e| ServiceError::FetchError(format!("Failed to fetch index: {e}")))?;
    let index: RoadworkIndex = serde_json::from_str(&json)
        .map_err(|e| ServiceError::FetchError(format!("Invalid index JSON: {e}")))?;
    Ok(index)
}

/// Fetches a single descriptor from GitHub by its relative `path`
/// (e.g. `"France/France-Paris.json"`).
pub async fn fetch_descriptor(path: &str) -> Result<ServiceDescriptor, ServiceError> {
    let url = format!("{DESCRIPTORS_BASE_URL}{path}");
    let http = HttpService;
    let json = http
        .get_url(&url)
        .await
        .map_err(|e| ServiceError::FetchError(format!("Failed to fetch descriptor {path}: {e}")))?;
    let descriptor: ServiceDescriptor = serde_json::from_str(&json)
        .map_err(|e| ServiceError::FetchError(format!("Invalid descriptor {path}: {e}")))?;
    Ok(descriptor)
}

include!(concat!(env!("OUT_DIR"), "/descriptors.rs"));

/// Returns the relative file path of every built-in descriptor, keyed by its
/// resolved key (used to build source URLs).
pub fn builtin_paths() -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (path, json) in DESCRIPTORS {
        if let Ok(descriptor) = serde_json::from_str::<ServiceDescriptor>(json) {
            let key = descriptor
                .metadata
                .effective_key(path.trim_end_matches(".json"));
            map.insert(key, path.to_string());
        }
    }
    map
}

/// Loads the built-in opendata descriptors embedded at compile time.
pub fn load_descriptors() -> HashMap<String, ServiceDescriptor> {
    let mut map = HashMap::new();
    for (path, json) in DESCRIPTORS {
        match serde_json::from_str::<ServiceDescriptor>(json) {
            Ok(descriptor) => {
                let key = descriptor
                    .metadata
                    .effective_key(path.trim_end_matches(".json"));
                map.insert(key, descriptor);
            }
            Err(e) => {
                log::error!("Failed to parse descriptor {path}: {e}");
            }
        }
    }
    map
}

/// Returns the built-in services (name and center), sorted by name.
pub fn get_services() -> Vec<ServiceInfo> {
    let mut services: Vec<ServiceInfo> = load_descriptors()
        .into_iter()
        .map(|(name, desc)| ServiceInfo {
            name,
            label: desc.metadata.label(),
            center: desc.metadata.center,
        })
        .collect();
    services.sort_by(|a, b| a.name.cmp(&b.name));
    services
}

/// Returns the descriptor of a built-in service.
pub fn get_descriptor(service_name: &str) -> Option<ServiceDescriptor> {
    load_descriptors().remove(service_name)
}

/// Fetches and parses roadworks for a built-in service.
pub async fn get_roadworks(service_name: &str) -> Result<RoadworkData, ServiceError> {
    match get_descriptor(service_name) {
        None => Err(ServiceError::UnknownService(service_name.to_string())),
        Some(descriptor) => Ok(fetch_roadworks(service_name, &descriptor).await?),
    }
}

/// Fetches and parses roadworks for the given descriptor.
pub async fn fetch_roadworks(
    service_name: &str,
    descriptor: &ServiceDescriptor,
) -> Result<RoadworkData, OpendataError> {
    let ods = OpendataService {
        service_name: service_name.to_string(),
        service_descriptor: descriptor.clone(),
    };
    let mut data = ods.get_roadworks_data().await?;
    data.apply_finished_status();
    Ok(data)
}

/// Configuration of the synchronization with a Roadwork server.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncConfig {
    pub url: String,
    pub team: String,
    pub login: String,
    pub password: String,
    pub enabled: bool,
}

/// Synchronizes the given data with the configured server.
///
/// Pushes the sync data of every roadwork and copies back the server statuses.
pub async fn synchronize(sync_config: &SyncConfig, roadwork_data: &mut RoadworkData) {
    if !sync_config.enabled {
        return;
    }
    info!("Synchronizing {}", roadwork_data.source);
    let url = format!(
        "{}/setData/{}/{}",
        sync_config.url.trim_end_matches('/'),
        sync_config.team,
        roadwork_data.source
    );

    let mut body: HashMap<String, SyncData> = HashMap::new();
    roadwork_data.iter().for_each(|roadwork| {
        body.insert(roadwork.opendata.id.clone(), roadwork.sync_data.clone());
    });

    let auth = format!("{}:{}", sync_config.login, sync_config.password);
    let encoded_auth = BASE64_STANDARD.encode(auth);
    let auth_header = format!("Basic {encoded_auth}");
    let mut headers = HashMap::new();
    headers.insert("Authorization".to_string(), auth_header);

    let http = HttpService;
    match http
        .post_json_object::<HashMap<String, SyncData>>(&url, &body, &headers)
        .await
    {
        Ok(synchronized_data) => {
            for (id, server_sync_data) in synchronized_data {
                match roadwork_data.get_mut_roadwork(&id) {
                    Some(roadwork) => roadwork.sync_data.copy(&server_sync_data),
                    None => log::warn!("Roadwork {id} not found"),
                }
            }
        }
        Err(e) => log::warn!("Synchronization failed: {e}"),
    }
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Unknown service descriptor {0}")]
    UnknownService(String),
    #[error(transparent)]
    OpendataError(#[from] OpendataError),
    #[error("{0}")]
    FetchError(String),
}
