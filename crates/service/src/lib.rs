//! Roadwork service crate — shared access to the opendata service descriptors.
//!
//! The built-in descriptors from `opendata/json/` are embedded at compile time
//! by `build.rs` and exposed through [`load_descriptors`] and [`get_services`].
//! This crate is used by the WME extension, the egui app and the webserver.

use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use log::info;
use roadwork_core::MyError;
use roadwork_core::http_service::HttpService;
use roadwork_core::model::roadwork_data::RoadworkData;
use roadwork_core::model::service_info::ServiceInfo;
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;
use roadwork_core::opendata::json::opendata_service::OpendataService;
use roadwork_sync::SyncData;
use serde::Deserialize;
use std::collections::HashMap;

include!(concat!(env!("OUT_DIR"), "/descriptors.rs"));

/// Loads the built-in opendata descriptors embedded at compile time.
pub fn load_descriptors() -> HashMap<String, ServiceDescriptor> {
    let mut map = HashMap::new();
    for (name, json) in DESCRIPTORS {
        match serde_json::from_str::<ServiceDescriptor>(json) {
            Ok(descriptor) => {
                map.insert(name.to_string(), descriptor);
            }
            Err(e) => {
                log::error!("Failed to parse descriptor {name}: {e}");
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
pub async fn get_roadworks(service_name: &str) -> Result<RoadworkData, MyError> {
    let descriptor = get_descriptor(service_name)
        .ok_or_else(|| MyError::ParsingError(format!("Unknown service: {service_name}")))?;
    fetch_roadworks(service_name, &descriptor).await
}

/// Fetches and parses roadworks for the given descriptor.
pub async fn fetch_roadworks(
    service_name: &str,
    descriptor: &ServiceDescriptor,
) -> Result<RoadworkData, MyError> {
    let ods = OpendataService::new(service_name.to_string(), descriptor.clone());
    let mut data = ods.get_data().await?;
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
        body.insert(roadwork.id.clone(), roadwork.sync_data.clone());
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
