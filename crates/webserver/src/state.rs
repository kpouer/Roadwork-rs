use crate::descriptor_manager::DescriptorManager;
use crate::storage::SqliteStorage;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use log::{info, warn};
use roadwork_core::http_service::HttpService;
use roadwork_core::model::roadwork_data::RoadworkData;
use roadwork_core::now_millis;
use roadwork_core::opendata::json::opendata_service::OpendataService;
use roadwork_sync::SyncData;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct SyncConfig {
    pub url: String,
    pub team: String,
    pub login: String,
    pub password: String,
    pub enabled: bool,
}

#[derive(Clone)]
pub struct AppState {
    pub descriptor_manager: Arc<DescriptorManager>,
    pub storage: Arc<SqliteStorage>,
    pub sync_config: Option<SyncConfig>,
    pub default_service: Arc<Mutex<Option<String>>>,
}

impl AppState {
    pub async fn get_or_fetch_roadworks(&self, service_name: &str) -> Option<RoadworkData> {
        let cached = self.storage.load_cache(service_name).await;
        match cached {
            None => {
                info!("No cached data for {service_name}, fetching");
                let data = self.fetch_from_opendata(service_name).await;
                if let Some(data) = &data {
                    let _ = self.storage.save_cache(service_name, &data).await;
                }
                data
            }
            Some(mut cached_data) => {
                let now = now_millis();
                if cached_data.created + 86_400_000 <= now {
                    info!("Cache obsolete for {service_name}, re-fetching");
                    self.refresh_service(service_name).await
                } else {
                    cached_data.apply_finished_status();
                    Some(cached_data)
                }
            }
        }
    }

    async fn fetch_from_opendata(&self, service_name: &str) -> Option<RoadworkData> {
        let descriptor = self.descriptor_manager.descriptor(service_name)?;
        let ods = OpendataService::new(service_name.to_string(), descriptor.clone());
        match ods.get_data().await {
            Ok(mut data) => {
                data.apply_finished_status();
                Some(data)
            }
            Err(e) => {
                log::error!("Failed to fetch opendata for {service_name}: {e}");
                None
            }
        }
    }

    pub async fn refresh_all(&self) {
        let service_names = self.descriptor_manager.descriptor_names();
        info!("Refreshing all {} services", service_names.len());
        for service_name in &service_names {
            self.refresh_service(service_name).await;
        }
    }

    pub async fn refresh_service(&self, service_name: &str) -> Option<RoadworkData> {
        info!("Refreshing service {service_name}");
        match self.fetch_from_opendata(service_name).await {
            Some(mut new_data) => {
                let old_data = self.storage.load_cache(service_name).await;
                if let Some(cached_data) = old_data {
                    new_data.merge(&cached_data);
                }
                new_data.apply_finished_status();
                // todo : this should be in the same transaction
                self.storage.delete_cache(service_name).await;
                let _ = self.storage.save_cache(service_name, &new_data).await;
                Some(new_data)
            }
            None => None,
        }
    }
}

impl AppState {
    pub async fn synchronize(&self, roadwork_data: &mut RoadworkData) {
        if let Some(sync_config) = &self.sync_config {
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
            let encoded_auth = BASE64_STANDARD.encode(&auth);
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
                            None => warn!("Roadwork {id} not found"),
                        }
                    }
                }
                Err(e) => warn!("Synchronization failed: {e}"),
            }
        }
    }
}
