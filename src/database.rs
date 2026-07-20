use indexed_db_futures::database::Database;
use indexed_db_futures::prelude::*;
use indexed_db_futures::transaction::TransactionMode;
use log::info;

use crate::model::roadwork_data::RoadworkData;
use crate::opendata::json::model::service_descriptor::ServiceDescriptor;
use crate::settings::Settings;

const DB_NAME: &str = "roadwork";
const DB_VERSION: u32 = 1;
const SETTINGS_STORE: &str = "settings";
const DESCRIPTORS_STORE: &str = "opendata_descriptors";
const CACHE_STORE: &str = "roadwork_cache";
const SETTINGS_KEY: &str = "default";

pub struct RoadworkDb {
    db: Database,
}

impl RoadworkDb {
    pub async fn new() -> Self {
        let db = Database::open(DB_NAME)
            .with_version(DB_VERSION)
            .with_on_upgrade_needed(|_, db| {
                db.create_object_store(SETTINGS_STORE).build()?;
                db.create_object_store(DESCRIPTORS_STORE).build()?;
                db.create_object_store(CACHE_STORE).build()?;
                Ok(())
            })
            .await
            .expect("Failed to open IndexedDB");
        Self { db }
    }

    pub(crate) async fn load_settings(&self) -> Settings {
        let tx = self
            .db
            .transaction(SETTINGS_STORE)
            .with_mode(TransactionMode::Readonly)
            .build()
            .expect("Failed to start transaction");
        let store = tx
            .object_store(SETTINGS_STORE)
            .expect("Failed to access settings store");
        let result: Option<Settings> = store
            .get(SETTINGS_KEY)
            .serde()
            .expect("Failed to read settings")
            .await
            .expect("Failed to await settings");
        result.unwrap_or_default()
    }

    pub(crate) async fn save_settings(&self, settings: &Settings) {
        let tx = self
            .db
            .transaction(SETTINGS_STORE)
            .with_mode(TransactionMode::Readwrite)
            .build()
            .expect("Failed to start transaction");
        let store = tx
            .object_store(SETTINGS_STORE)
            .expect("Failed to access settings store");
        store
            .put(settings)
            .with_key(SETTINGS_KEY.to_string())
            .serde()
            .expect("Failed to serialize settings")
            .await
            .expect("Failed to save settings");
        tx.commit().await.expect("Failed to commit settings");
    }

    pub(crate) async fn load_descriptors(
        &self,
    ) -> std::collections::HashMap<String, ServiceDescriptor> {
        let tx = self
            .db
            .transaction(DESCRIPTORS_STORE)
            .with_mode(TransactionMode::Readonly)
            .build()
            .expect("Failed to start transaction");
        let store = tx
            .object_store(DESCRIPTORS_STORE)
            .expect("Failed to access descriptors store");
        let iter = store
            .get_all()
            .serde()
            .expect("Failed to read descriptors")
            .await
            .expect("Failed to await descriptors");
        iter.into_iter().filter_map(|entry| entry.ok()).collect()
    }

    pub(crate) async fn save_descriptors(
        &self,
        descriptors: &std::collections::HashMap<String, ServiceDescriptor>,
    ) {
        let tx = self
            .db
            .transaction(DESCRIPTORS_STORE)
            .with_mode(TransactionMode::Readwrite)
            .build()
            .expect("Failed to start transaction");
        let store = tx
            .object_store(DESCRIPTORS_STORE)
            .expect("Failed to access descriptors store");
        for (name, descriptor) in descriptors {
            store
                .put(descriptor)
                .with_key(name.clone())
                .serde()
                .expect("Failed to serialize descriptor")
                .await
                .expect("Failed to save descriptor");
        }
        tx.commit().await.expect("Failed to commit descriptors");
    }

    pub(crate) async fn load_cache(&self, service_name: &str) -> Option<RoadworkData> {
        let tx = self
            .db
            .transaction(CACHE_STORE)
            .with_mode(TransactionMode::Readonly)
            .build()
            .expect("Failed to start transaction");
        let store = tx
            .object_store(CACHE_STORE)
            .expect("Failed to access cache store");
        store
            .get(service_name)
            .serde()
            .expect("Failed to read cache")
            .await
            .expect("Failed to await cache")
    }

    pub(crate) async fn save_cache(&self, service_name: &str, data: &RoadworkData) {
        info!("Saving cache for {service_name}");
        let tx = self
            .db
            .transaction(CACHE_STORE)
            .with_mode(TransactionMode::Readwrite)
            .build()
            .expect("Failed to start transaction");
        let store = tx
            .object_store(CACHE_STORE)
            .expect("Failed to access cache store");
        store
            .put(data)
            .with_key(service_name.to_string())
            .serde()
            .expect("Failed to serialize cache")
            .await
            .expect("Failed to save cache");
        tx.commit().await.expect("Failed to commit cache");
    }

    pub(crate) async fn delete_cache(&self, service_name: &str) {
        info!("Deleting cache for {service_name}");
        let tx = self
            .db
            .transaction(CACHE_STORE)
            .with_mode(TransactionMode::Readwrite)
            .build()
            .expect("Failed to start transaction");
        let store = tx
            .object_store(CACHE_STORE)
            .expect("Failed to access cache store");
        store
            .delete(service_name)
            .serde()
            .expect("Failed to prepare delete")
            .await
            .expect("Failed to delete cache");
        tx.commit().await.expect("Failed to commit delete");
    }
}
