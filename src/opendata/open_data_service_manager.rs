use crate::database::RoadworkDb;
use crate::model::roadwork_data::RoadworkData;
use crate::now_millis;
use crate::opendata::json::model::lat_lng::LatLng;
use crate::opendata::json::opendata_service::OpendataService;
use crate::service::synchronization_service::SynchronizationService;
use crate::settings::Settings;
use log::{debug, info};
use roadwork_sync::SyncData;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub(crate) struct OpenDataServiceManager {
    db: Arc<RoadworkDb>,
    settings: Arc<Mutex<Settings>>,
    opendata_services: HashMap<String, OpendataService>,
    service_names: Vec<String>,
    synchronization_service: SynchronizationService,
}

impl OpenDataServiceManager {
    pub(crate) async fn new(db: Arc<RoadworkDb>, settings: Arc<Mutex<Settings>>) -> Self {
        let descriptors = db.load_descriptors().await;
        let opendata_services: HashMap<String, OpendataService> = descriptors
            .into_iter()
            .map(|(name, sd)| {
                let ods = OpendataService::new(name.clone(), sd);
                (name, ods)
            })
            .collect();
        let service_names = opendata_services.keys().cloned().collect();
        Self {
            synchronization_service: SynchronizationService::new(Arc::clone(&settings)),
            db,
            settings,
            service_names,
            opendata_services,
        }
    }

    pub(crate) fn get_center(&self) -> LatLng {
        let service_name = &self.settings.lock().unwrap().opendata_service;
        self.opendata_services
            .get(service_name)
            .map(|service| service.service_descriptor.metadata.center)
            .unwrap_or_default()
    }

    pub(crate) async fn get_data(&self) -> Option<RoadworkData> {
        let mut roadworks_option = self.get_roadworks().await;
        if let Some(roadwork_data) = &mut roadworks_option {
            Self::apply_finished_status(roadwork_data);
            self.synchronization_service
                .synchronize(roadwork_data)
                .await;
        }
        roadworks_option
    }

    pub(crate) async fn save(&self, roadwork_data: &RoadworkData) {
        info!("save {}", roadwork_data.source);
        self.db
            .save_cache(&roadwork_data.source, roadwork_data)
            .await;
    }

    async fn get_roadworks(&self) -> Option<RoadworkData> {
        let service_name = self.settings.lock().unwrap().opendata_service.clone();
        info!("get_roadworks for {service_name}");

        match self.db.load_cache(&service_name).await {
            None => {
                info!("There is no cached data");
                if let Some(ods) = self.get_opendata_service() {
                    let data = ods.get_data().await.ok()?;
                    self.db.save_cache(&service_name, &data).await;
                    Some(data)
                } else {
                    None
                }
            }
            Some(mut cached_roadwork_data) => {
                let now = now_millis();
                if cached_roadwork_data.created + 86_400_000 <= now {
                    info!("Cache is obsolete");
                    self.db.delete_cache(&service_name).await;
                    if let Some(ods) = self.get_opendata_service() {
                        if let Some(mut new_data) = ods.get_data().await.ok() {
                            let new_roadworks = &mut new_data.roadworks;
                            info!("reloaded {} new roadworks", new_roadworks.len());
                            for existing_roadwork in &mut cached_roadwork_data {
                                if let Some(new_roadwork) =
                                    new_roadworks.get_mut(&existing_roadwork.id)
                                {
                                    new_roadwork.sync_data =
                                        SyncData::new_from(&existing_roadwork.sync_data);
                                    info!(
                                        "Roadwork {} -> status {}",
                                        existing_roadwork.id, existing_roadwork.sync_data.status
                                    );
                                }
                            }
                            self.db.save_cache(&service_name, &new_data).await;
                            return Some(new_data);
                        }
                    }
                    return None;
                }

                Some(cached_roadwork_data)
            }
        }
    }

    pub(crate) fn get_opendata_service(&self) -> Option<&OpendataService> {
        debug!("get_opendata_service");
        let opendata_service = &self.settings.lock().unwrap().opendata_service;
        debug!("opendata_service: {opendata_service}");
        self.opendata_services.get(opendata_service)
    }

    fn apply_finished_status(roadwork_data: &mut RoadworkData) {
        for roadwork in roadwork_data.roadworks.values_mut() {
            if roadwork.is_expired() {
                roadwork.sync_data.status = roadwork_sync::Status::Finished;
            }
        }
    }

    pub(crate) fn services(&self) -> &[String] {
        &self.service_names
    }

    pub(crate) async fn delete_cache(&self) {
        let service_name = self.settings.lock().unwrap().opendata_service.clone();
        info!("delete_cache for {service_name}");
        self.db.delete_cache(&service_name).await;
    }
}
