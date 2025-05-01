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
    httpService: HttpService,
    // localizationService: LocalizationService,
}

impl SynchronizationService {
    pub(crate) fn new(settings: Arc<Mutex<Settings>>) -> SynchronizationService {
        Self {
            settings,
            httpService: HttpService::default(),
        }
    }
}

impl SynchronizationService {
    /**
     * Synchronize the data with the server
     *
     * @param roadworkData the data to synchronize. Status might be updated
     */
    pub(crate) fn synchronize(&self, roadworkData: &mut RoadworkData) {
        if self.settings.lock().unwrap().synchronizationEnabled {
            info!("synchronize");
            let url = self.getUrl(&roadworkData.source);
            info!("Will synchronize with url {}", url);
            let mut body: HashMap<String, SyncData> = HashMap::new();
            roadworkData.iter().for_each(|roadwork| {
                body.insert(
                    roadwork.id.clone(),
                    roadwork.syncData.as_ref().unwrap().clone(),
                ); // todo remove this unwrap
            });
            let headers = self.createHeaders();
            let synchronizedData: HashMap<String, SyncData> = self
                .httpService
                .postJsonObject(&url, &body, &headers)
                .unwrap();

            for (id, serverSyncData) in synchronizedData {
                match roadworkData.get_mut_roadwork(&id) {
                    Some(roadwork) => {
                        match &mut roadwork.syncData {
                            None => warn!("Roadwork {id} has no sync data"),
                            Some(sync_data) => sync_data.copy(&serverSyncData),
                        }
                        roadwork.updateMarker();
                    }
                    None => warn!("Roadwork {id} not found"),
                }
            }
        }
    }

    fn getUrl(&self, source: &str) -> String {
        let settings = self.settings.lock().unwrap();
        let synchronization_team = &settings.synchronizationTeam;
        let mut url = settings.synchronizationUrl.clone();
        if !url.ends_with("/") {
            url.push_str("/");
        }
        format!("{url}/setData/{synchronization_team}/{source}")
    }

    fn createHeaders(&self) -> HashMap<String, String> {
        let auth = {
            let settings = self.settings.lock().unwrap();
            format!(
                "{}:{}",
                settings.synchronizationLogin, settings.synchronizationPassword
            )
        };
        let encodedAuth = BASE64_STANDARD.encode(&auth);
        let authHeader = format!("Basic {encodedAuth}");
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), authHeader);
        headers
    }
    //
    //    private static class MapParameterizedTypeReference extends ParameterizedTypeReference<Map<String, SyncData>> {
    //    }
}
