use crate::model::roadwork_data::RoadworkData;
use crate::service::http_service::HttpService;
use crate::settings::Settings;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use log::{info, warn};
use roadwork_sync::SyncData;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub(crate) struct SynchronizationService {
    settings: Arc<Mutex<Settings>>,
    http_service: HttpService,
    // localizationService: LocalizationService,
}

impl SynchronizationService {
    pub(crate) fn new(settings: Arc<Mutex<Settings>>) -> SynchronizationService {
        Self {
            settings,
            http_service: HttpService::default(),
        }
    }
}

impl SynchronizationService {
    /**
     * Synchronize the data with the server
     *
     * @param roadwork_data the data to synchronize. Status might be updated
     */
    pub(crate) fn synchronize(&self, roadwork_data: &mut RoadworkData) {
        if self.settings.lock().unwrap().synchronizationEnabled {
            info!("synchronize");
            let url = self.get_url(&roadwork_data.source);
            info!("Will synchronize with url {}", url);
            let mut body: HashMap<String, SyncData> = HashMap::new();
            roadwork_data.iter().for_each(|roadwork| {
                body.insert(
                    roadwork.id.clone(),
                    roadwork.sync_data.clone(),
                );
            });
            let headers = self.create_headers();
            let synchronized_data: HashMap<String, SyncData> = self
                .http_service
                .post_json_object(&url, &body, &headers)
                .unwrap();

            for (id, server_sync_data) in synchronized_data {
                match roadwork_data.get_mut_roadwork(&id) {
                    Some(roadwork) => roadwork.sync_data.copy(&server_sync_data),
                    None => warn!("Roadwork {id} not found"),
                }
            }
        }
    }

    fn get_url(&self, source: &str) -> String {
        let settings = self.settings.lock().unwrap();
        let synchronization_team = &settings.synchronizationTeam;
        let mut url = settings.synchronizationUrl.clone();
        if !url.ends_with("/") {
            url.push_str("/");
        }
        format!("{url}/setData/{synchronization_team}/{source}")
    }

    fn create_headers(&self) -> HashMap<String, String> {
        let auth = {
            let settings = self.settings.lock().unwrap();
            format!(
                "{}:{}",
                settings.synchronizationLogin, settings.synchronizationPassword
            )
        };
        let encoded_auth = BASE64_STANDARD.encode(&auth);
        let auth_header = format!("Basic {encoded_auth}");
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), auth_header);
        headers
    }
}
