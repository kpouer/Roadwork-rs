use crate::config::Config;
use crate::model::roadwork_data::RoadworkData;
use crate::opendata::json::model::lat_lng::LatLng;
use crate::opendata::json::model::service_descriptor::ServiceDescriptor;
use crate::opendata::json::opendata_service::OpendataService;
use crate::service::synchronization_service::SynchronizationService;
use crate::settings::Settings;
use eframe::glow::VERSION;
use log::{debug, error, info, warn};
use roadwork_sync::SyncData;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use unicode_normalization::UnicodeNormalization;

pub(crate) struct OpenDataServiceManager {
    config: Config,
    settings: Arc<Mutex<Settings>>,
    service_names: Vec<String>,
    opendata_services: HashMap<String, OpendataService>,
    synchronizationService: SynchronizationService,
}

impl OpenDataServiceManager {
    pub(crate) fn new(settings: Arc<Mutex<Settings>>) -> Self {
        let opendata_services = Self::get_json_file_names("opendata/json");
        Self {
            config: Config::default(),
            synchronizationService: SynchronizationService::new(Arc::clone(&settings)),
            settings,
            service_names: opendata_services.keys().map(|s| s.to_string()).collect(),
            opendata_services,
        }
    }

    pub(crate) fn get_center(&self) -> LatLng {
        let service_name = &self.settings.lock().unwrap().opendataService;
        self.opendata_services
            .get(service_name)
            .map(|service| service.serviceDescriptor.metadata.center)
            .unwrap_or_default()
    }

    pub(crate) fn get_data(&self) -> Option<RoadworkData> {
        let mut roadworks_option = self.getRoadworks();
        if let Some(roadwork_data) = &mut roadworks_option {
            Self::applyFinishedStatus(roadwork_data);
            self.synchronizationService.synchronize(roadwork_data);
        }
        roadworks_option
    }

    fn save(&self, roadworkData: &RoadworkData) {
        info!("save {}", roadworkData.source);
        let savePath = self.getPath(&roadworkData.source);
        savePath.parent().iter().for_each(|p| {
            if !p.exists() {
                fs::create_dir_all(p).ok();
            }
        });
        match File::create(&savePath) {
            Ok(file) => {
                if let Err(e) = serde_json::to_writer_pretty(file, roadworkData) {
                    error!("Unable to save cache to {savePath:?} because {e}");
                }
            }
            Err(e) => error!("Unable to save cache to {savePath:?} because {e}"),
        }
    }

    /**
     * Returns roadwork data.
     * If
     *
     * @return an optional that should contains Roadwork data
     */
    fn getRoadworks(&self) -> Option<RoadworkData> {
        let currentPath = self.getPath(&self.settings.lock().unwrap().opendataService);
        info!("getData {:?}", currentPath);
        match Self::loadCache(&currentPath) {
            None => {
                info!("There is no cached data");
                self.getOpendataService()
                    .and_then(|ods| ods.get_data().ok())
                    .inspect(|new_data| self.save(new_data))
            }
            Some(mut cachedRoadworkData) => {
                if (cachedRoadworkData.created + Duration::from_secs(86400))
                    .le(&SystemTime::now().duration_since(UNIX_EPOCH).unwrap())
                {
                    info!("Cache is obsolete {currentPath:?}");
                    fs::remove_file(&currentPath).ok();
                    let mut newDataOptional = self
                        .getOpendataService()
                        .and_then(|ods| ods.get_data().ok());
                    if let Some(newData) = &mut newDataOptional {
                        let newRoadworks = &mut newData.roadworks;
                        info!("reloaded {} new roadworks", newRoadworks.len());
                        for existingRoadwork in &mut cachedRoadworkData {
                            if let Some(newRoadwork) = newRoadworks.get_mut(&existingRoadwork.id) {
                                if newRoadwork.syncData.is_none() {
                                    newRoadwork.syncData = Some(SyncData::default());
                                }
                                match &existingRoadwork.syncData {
                                    Some(syncData) => {
                                        info!(
                                            "Roadwork {} -> status {}",
                                            existingRoadwork.id, syncData.status
                                        );
                                        newRoadwork
                                            .syncData
                                            .iter_mut()
                                            .for_each(|newSyncData| newSyncData.copy(syncData));
                                        existingRoadwork.updateMarker();
                                    }
                                    None => warn!("No sync data for roadwork {newRoadwork:?}"),
                                }
                            }
                        }
                        self.save(newData);
                    }
                    return newDataOptional;
                }

                Some(cachedRoadworkData)
            }
        }
    }

    fn getOpendataService(&self) -> Option<&OpendataService> {
        debug!("getOpendataService");
        let opendata_service = &self.settings.lock().unwrap().opendataService;
        debug!("opendata_service: {opendata_service}");
        self.opendata_services.get(opendata_service)
    }

    fn getPath(&self, opendata_service: &str) -> PathBuf {
        let mut path = PathBuf::from(&self.config.dataPath);
        path.push(format!("{opendata_service}.{VERSION}.json"));
        path
    }

    fn loadCache(cache_path: &Path) -> Option<RoadworkData> {
        File::open(cache_path)
            .ok()
            .map(|file| serde_json::from_reader::<File, RoadworkData>(file).ok())
            .flatten()
    }

    fn applyFinishedStatus(roadwork_data: &mut RoadworkData) {
        roadwork_data
            .roadworks
            .values_mut()
            .filter(|roadwork| roadwork.isExpired())
            .flat_map(|roadwork| &mut roadwork.syncData)
            .for_each(|sync_data| sync_data.status = roadwork_sync::Status::Finished);
    }

    /// Returns a vector of JSON file names without the `.json` extension from the given directory.
    fn get_json_file_names(path: &str) -> HashMap<String, OpendataService> {
        info!("get_json_file_names {:?}", path);
        let path = Path::new(path);
        let mut services = HashMap::new();
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                        let file_name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .map(|name| name.nfc().collect::<String>());
                        if let Some(name) = file_name {
                            let name = name.strip_suffix(".json").unwrap();
                            match File::open(&path) {
                                Ok(file) => {
                                    match serde_json::from_reader::<File, ServiceDescriptor>(file) {
                                        Ok(service_descriptor) => {
                                            let opendata_service = OpendataService::new(
                                                name.into(),
                                                service_descriptor,
                                            );
                                            services.insert(name.to_string(), opendata_service);
                                        }
                                        Err(e) => error!("Failed to parse file {:?}: {}", path, e),
                                    }
                                }
                                Err(e) => error!("Failed to open file {:?}: {}", path, e),
                            }
                        }
                    }
                }
            }
        }
        services
    }

    pub(crate) fn services(&self) -> &[String] {
        &self.service_names
    }

    pub(crate) fn deleteCache(&self) {
        let currentPath = self.getPath(&self.settings.lock().unwrap().opendataService);
        info!("deleteCache {currentPath:?}");
        fs::remove_file(&currentPath).ok();
    }
}
