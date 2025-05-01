use crate::model::roadwork_data::RoadworkData;
use crate::opendata::json::model::lat_lng::LatLng;
use crate::opendata::json::model::service_descriptor::ServiceDescriptor;
use crate::opendata::json::opendata_service::OpendataService;
use crate::service::synchronization_service::SynchronizationService;
use crate::settings::Settings;
use log::{debug, error, info};
use roadwork_sync::SyncData;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use unicode_normalization::UnicodeNormalization;

pub(crate) struct OpenDataServiceManager {
    settings: Arc<Mutex<Settings>>,
    service_names: Vec<String>,
    opendata_services: HashMap<String, OpendataService>,
    synchronization_service: SynchronizationService,
}

impl OpenDataServiceManager {
    const VERSION: &'static str = "2";

    pub(crate) fn new(settings: Arc<Mutex<Settings>>) -> Self {
        let opendata_services = Self::get_json_file_names("opendata/json");
        Self {
            synchronization_service: SynchronizationService::new(Arc::clone(&settings)),
            settings,
            service_names: opendata_services.keys().map(|s| s.to_string()).collect(),
            opendata_services,
        }
    }

    pub(crate) fn get_center(&self) -> LatLng {
        let service_name = &self.settings.lock().unwrap().opendataService;
        self.opendata_services
            .get(service_name)
            .map(|service| service.service_descriptor.metadata.center)
            .unwrap_or_default()
    }

    pub(crate) fn get_data(&self) -> Option<RoadworkData> {
        let mut roadworks_option = self.get_roadworks();
        if let Some(roadwork_data) = &mut roadworks_option {
            Self::apply_finished_status(roadwork_data);
            self.synchronization_service.synchronize(roadwork_data);
        }
        roadworks_option
    }

    /// Save the roadwork state
    pub(crate) fn save(&self, roadwork_data: &RoadworkData) {
        info!("save {}", roadwork_data.source);
        if let Some(save_path) = self.get_path(&roadwork_data.source) {
            info!("save to {save_path:?}");
            match save_path.parent() {
                None => error!("Unable to save cache to {save_path:?} because parent directory does not exist"),
                Some(parent) => {
                    fs::create_dir_all(parent).ok();
                    match File::create(&save_path) {
                        Ok(file) => {
                            if let Err(e) = serde_json::to_writer_pretty(file, roadwork_data) {
                                error!("Unable to save cache to {save_path:?} because {e}");
                            }
                        }
                        Err(e) => error!("Unable to save cache to {save_path:?} because {e}"),
                    }
                }
            }
        }
    }

    /**
     * Returns roadwork data.
     * If
     *
     * @return an optional that should contain Roadwork data
     */
    fn get_roadworks(&self) -> Option<RoadworkData> {
        let current_path = self.get_path(&self.settings.lock().unwrap().opendataService);
        if let Some(current_path) = &current_path {
            info!("getData {current_path:?}");
            match Self::load_cache(&current_path) {
                None => {
                    info!("There is no cached data");
                    self.get_opendata_service()
                        .and_then(|ods| ods.get_data().ok())
                        .inspect(|new_data| self.save(new_data))
                }
                Some(mut cached_roadwork_data) => {
                    if (cached_roadwork_data.created + Duration::from_secs(86400))
                        .le(&SystemTime::now().duration_since(UNIX_EPOCH).unwrap())
                    {
                        info!("Cache is obsolete {current_path:?}");
                        fs::remove_file(&current_path).ok();
                        let mut new_data_optional = self
                            .get_opendata_service()
                            .and_then(|ods| ods.get_data().ok());
                        if let Some(new_data) = &mut new_data_optional {
                            let new_roadworks = &mut new_data.roadworks;
                            info!("reloaded {} new roadworks", new_roadworks.len());
                            for existing_roadwork in &mut cached_roadwork_data {
                                if let Some(new_roadwork) = new_roadworks.get_mut(&existing_roadwork.id) {
                                    new_roadwork.sync_data = SyncData::new_from(&existing_roadwork.sync_data);
                                    info!(
                                    "Roadwork {} -> status {}",
                                    existing_roadwork.id, existing_roadwork.sync_data.status
                                );
                                }
                            }
                            self.save(new_data);
                        }
                        return new_data_optional;
                    }

                    Some(cached_roadwork_data)
                }
            }
        } else {
            info!("There is no cached folder");
            self.get_opendata_service()
                .and_then(|ods| ods.get_data().ok())
                .inspect(|new_data| self.save(new_data))
        }
    }

    pub(crate) fn get_opendata_service(&self) -> Option<&OpendataService> {
        debug!("get_opendata_service");
        let opendata_service = &self.settings.lock().unwrap().opendataService;
        debug!("opendata_service: {opendata_service}");
        self.opendata_services.get(opendata_service)
    }

    fn get_path(&self, opendata_service: &str) -> Option<PathBuf> {
        Settings::settings_folder()
            .map(|mut folder| {
                folder.push(format!("{opendata_service}.{}.json", Self::VERSION));
                folder
            })
    }

    fn load_cache(cache_path: &Path) -> Option<RoadworkData> {
        File::open(cache_path)
            .ok()
            .map(|file| serde_json::from_reader::<File, RoadworkData>(file).ok())
            .flatten()
    }

    fn apply_finished_status(roadwork_data: &mut RoadworkData) {
        for roadwork in roadwork_data.roadworks.values_mut() {
            if roadwork.is_expired() {
                roadwork.sync_data.status = roadwork_sync::Status::Finished;
            }
        }
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
                                        Err(e) => error!("Failed to parse file {path:?}: {e}"),
                                    }
                                }
                                Err(e) => error!("Failed to open file {path:?}: {e}"),
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

    pub(crate) fn delete_cache(&self) {
        if let Some(current_path) = self.get_path(&self.settings.lock().unwrap().opendataService) {
            info!("delete_cache {current_path:?}");
            fs::remove_file(&current_path).ok();
        }
    }
}
