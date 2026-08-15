//! Roadwork WME extension — builds a `.user.js` userscript for the Waze Map Editor.
//!
//! The actual logic lives in the static JS/CSS files assembled at build time.

/// The version of the WME extension, sourced from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The build date of the WME extension, injected by `build.rs`.
pub const BUILD_DATE: &str = env!("WME_BUILD_DATE");

use log::info;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

use roadwork_core::model::opendata_data::OpendataData;
use roadwork_core::model::roadwork_data::RoadworkData;
use roadwork_core::model::service_info::ServiceInfo;
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;
use roadwork_core::opendata::json::opendata_service::OpendataService;

#[wasm_bindgen(start)]
pub fn init_logger() {
    console_log::init_with_level(log::Level::Debug).ok();
}

#[wasm_bindgen]
pub fn set_log_level(level: &str) {
    let filter = match level.to_lowercase().as_str() {
        "error" => log::LevelFilter::Error,
        "warn" => log::LevelFilter::Warn,
        "info" => log::LevelFilter::Info,
        "debug" => log::LevelFilter::Debug,
        "trace" => log::LevelFilter::Trace,
        _ => return,
    };
    log::set_max_level(filter);
}

thread_local! {
    static CUSTOM_DESCRIPTORS: RefCell<HashMap<String, ServiceDescriptor>> =
        RefCell::new(HashMap::new());
    static OPENDATA_DESCRIPTORS: RefCell<HashMap<String, ServiceDescriptor>> =
        RefCell::new(HashMap::new());
}

/// Maximum age of a cached roadworks snapshot before it is re-fetched (24h).
const ROADWORK_CACHE_MAX_AGE_MS: u64 = 86_400_000;

thread_local! {
    /// Lazily-opened persistent cache. The store is taken out for the duration
    /// of an operation (a `RefCell` borrow must never cross an `await`).
    static STORE: RefCell<Option<roadwork_db::Store>> = const { RefCell::new(None) };
}

async fn store_take() -> Result<roadwork_db::Store, JsValue> {
    match STORE.with(|cell| cell.borrow_mut().take()) {
        Some(store) => Ok(store),
        None => roadwork_db::Store::open()
            .await
            .map_err(|e| JsValue::from_str(&format!("Cache open error: {e}"))),
    }
}

fn store_put_back(store: roadwork_db::Store) {
    STORE.with(|cell| *cell.borrow_mut() = Some(store));
}

/// Opens the persistent store (installs the OPFS VFS). Called by the worker at
/// startup so the first cache access does not block on I/O.
#[wasm_bindgen]
pub async fn open_store() -> Result<(), JsValue> {
    info!("[wasm] open_store");
    let store = store_take().await?;
    store_put_back(store);
    Ok(())
}

fn js_err(e: impl std::fmt::Display) -> JsValue {
    JsValue::from_str(&e.to_string())
}

async fn fetch_roadworks_data(
    service_name: &str,
    descriptor: &ServiceDescriptor,
) -> Result<RoadworkData, JsValue> {
    roadwork_service::fetch_roadworks(service_name, descriptor)
        .await
        .map_err(|e| JsValue::from_str(&format!("Fetch error: {e}")))
}

fn serialize_data<T: Serialize>(data: &T) -> Result<JsValue, JsValue> {
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    data.serialize(&serializer)
        .map_err(|e| JsValue::from_str(&format!("Serialize error: {e}")))
}

#[wasm_bindgen]
pub fn set_custom_descriptors(pairs: JsValue) -> Result<(), JsValue> {
    let pairs: Vec<(String, String)> = serde_wasm_bindgen::from_value(pairs)
        .map_err(|e| JsValue::from_str(&format!("Invalid descriptor pairs: {e}")))?;
    info!("[wasm] set_custom_descriptors: {} pairs", pairs.len());
    let mut map = HashMap::new();
    for (name, json) in pairs {
        match serde_json::from_str::<ServiceDescriptor>(&json) {
            Ok(descriptor) => {
                map.insert(name, descriptor);
            }
            Err(e) => {
                log::error!("Failed to parse custom descriptor {name}: {e}");
                return Err(JsValue::from_str(&format!(
                    "Failed to parse custom descriptor {name}: {e}"
                )));
            }
        }
    }
    CUSTOM_DESCRIPTORS.with(|cell| {
        *cell.borrow_mut() = map;
    });
    Ok(())
}

fn all_descriptors() -> HashMap<String, ServiceDescriptor> {
    let mut map = roadwork_service::load_descriptors();
    CUSTOM_DESCRIPTORS.with(|cell| {
        for (name, descriptor) in cell.borrow().iter() {
            map.insert(name.clone(), descriptor.clone());
        }
    });
    map
}

#[wasm_bindgen]
pub fn get_services() -> JsValue {
    info!("[wasm] get_services");
    let descriptors = all_descriptors();
    let mut services: Vec<ServiceInfo> = descriptors
        .into_iter()
        .map(|(name, desc)| ServiceInfo {
            name,
            center: desc.metadata.center,
        })
        .collect();
    services.sort_by(|a, b| a.name.cmp(&b.name));
    serde_wasm_bindgen::to_value(&services).unwrap()
}

#[wasm_bindgen]
pub async fn get_roadworks(service_name: &str, force_refresh: bool) -> Result<JsValue, JsValue> {
    info!("[wasm] get_roadworks force_refresh={force_refresh}");
    let descriptors = all_descriptors();
    let descriptor = descriptors
        .get(service_name)
        .ok_or_else(|| JsValue::from_str(&format!("Unknown service: {service_name}")))?;

    let data = {
        let mut store = store_take().await?;
        let cached = if force_refresh {
            None
        } else {
            store
                .get_roadworks_cached(service_name, ROADWORK_CACHE_MAX_AGE_MS)
                .map_err(js_err)?
        };
        let data = match cached {
            Some(data) => {
                info!("[wasm] get_roadworks: cache hit");
                data
            }
            None => {
                let data = fetch_roadworks_data(service_name, descriptor).await?;
                store.save_roadworks(service_name, &data).map_err(js_err)?;
                data
            }
        };
        store_put_back(store);
        data
    };

    info!("[wasm] get_roadworks: data loaded {}", data.roadworks.len());
    serialize_data(&data)
}

#[wasm_bindgen]
pub async fn clear_all_cache() -> Result<(), JsValue> {
    let mut store = store_take().await?;
    store.clear_all().map_err(js_err)?;
    store_put_back(store);
    Ok(())
}

#[wasm_bindgen]
pub fn set_opendata_custom_descriptors(pairs: JsValue) -> Result<(), JsValue> {
    let pairs: Vec<(String, String)> = serde_wasm_bindgen::from_value(pairs)
        .map_err(|e| JsValue::from_str(&format!("Invalid descriptor pairs: {e}")))?;
    info!(
        "[wasm] set_opendata_custom_descriptors: {} pairs",
        pairs.len()
    );
    let mut map = HashMap::new();
    for (name, json) in pairs {
        match serde_json::from_str::<ServiceDescriptor>(&json) {
            Ok(descriptor) => {
                map.insert(name, descriptor);
            }
            Err(e) => {
                log::error!("Failed to parse opendata descriptor {name}: {e}");
                return Err(JsValue::from_str(&format!(
                    "Failed to parse opendata descriptor {name}: {e}"
                )));
            }
        }
    }
    OPENDATA_DESCRIPTORS.with(|cell| {
        *cell.borrow_mut() = map;
    });
    Ok(())
}

#[wasm_bindgen]
pub async fn get_opendata(service_name: &str, force_refresh: bool) -> Result<JsValue, JsValue> {
    info!("[wasm] get_opendata force_refresh={force_refresh}");
    let descriptor = OPENDATA_DESCRIPTORS
        .with(|cell| cell.borrow().get(service_name).cloned())
        .ok_or_else(|| JsValue::from_str(&format!("Unknown opendata service: {service_name}")))?;

    let data = {
        let mut store = store_take().await?;
        let has_url = descriptor
            .metadata
            .url
            .as_deref()
            .is_some_and(|url| !url.trim().is_empty());
        let cached = store.get_opendata(service_name).map_err(js_err)?;
        let data = match cached {
            Some(data) if !force_refresh => {
                info!("[wasm] get_opendata: cache hit");
                data
            }
            _ if has_url => {
                let service = OpendataService {
                    service_name: service_name.to_string(),
                    service_descriptor: descriptor,
                };
                let data = service
                    .get_data()
                    .await
                    .map_err(|e| JsValue::from_str(&format!("Fetch error: {e}")))?;
                store.save_opendata(service_name, &data).map_err(js_err)?;
                data
            }
            Some(data) => data,
            None => {
                return Err(JsValue::from_str(&format!(
                    "No URL and no cached data for {service_name}"
                )));
            }
        };
        store_put_back(store);
        data
    };

    info!("[wasm] get_opendata: data loaded {}", data.opendata.len());
    serialize_data(&data)
}

#[wasm_bindgen]
pub async fn get_opendata_cached(service_name: &str) -> Result<JsValue, JsValue> {
    info!("[wasm] get_opendata_cached {service_name}");
    let store = store_take().await?;
    let cached = store.get_opendata(service_name).map_err(js_err)?;
    store_put_back(store);
    match cached {
        Some(data) => serialize_data(&data),
        None => Ok(JsValue::NULL),
    }
}

#[wasm_bindgen]
pub async fn get_opendata_counts() -> Result<JsValue, JsValue> {
    info!("[wasm] get_opendata_counts");
    let store = store_take().await?;
    let counts = store.opendata_counts().map_err(js_err)?;
    store_put_back(store);
    serialize_data(&counts)
}

#[wasm_bindgen]
pub async fn get_roadworks_in_bbox(
    service_name: &str,
    lat_min: f64,
    lon_min: f64,
    lat_max: f64,
    lon_max: f64,
) -> Result<JsValue, JsValue> {
    info!("[wasm] get_roadworks_in_bbox {service_name}");
    let store = store_take().await?;
    let result = store
        .get_roadworks_in_bbox(service_name, lat_min, lon_min, lat_max, lon_max)
        .map_err(js_err)?;
    store_put_back(store);
    match result {
        Some(data) => serialize_data(&data),
        None => Ok(JsValue::NULL),
    }
}

#[wasm_bindgen]
pub async fn get_opendata_in_bbox(
    service_name: &str,
    lat_min: f64,
    lon_min: f64,
    lat_max: f64,
    lon_max: f64,
    limit: Option<u32>,
) -> Result<JsValue, JsValue> {
    info!("[wasm] get_opendata_in_bbox {service_name} limit={limit:?}");
    let store = store_take().await?;
    let result = store
        .get_opendata_in_bbox(
            service_name,
            lat_min,
            lon_min,
            lat_max,
            lon_max,
            limit.map(|l| l as u64),
        )
        .map_err(js_err)?;
    store_put_back(store);
    match result {
        Some(data) => serialize_data(&data),
        None => Ok(JsValue::NULL),
    }
}

#[wasm_bindgen]
pub async fn store_opendata_data(service_name: &str, data_json: &str) -> Result<(), JsValue> {
    info!("[wasm] store_opendata_data {service_name}");
    let data: OpendataData = serde_json::from_str(data_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid opendata data: {e}")))?;
    let mut store = store_take().await?;
    store.save_opendata(service_name, &data).map_err(js_err)?;
    store_put_back(store);
    Ok(())
}

#[wasm_bindgen]
pub async fn clear_roadworks_cache(service_name: &str) -> Result<(), JsValue> {
    let mut store = store_take().await?;
    store.remove_roadworks(service_name).map_err(js_err)?;
    store_put_back(store);
    Ok(())
}

#[wasm_bindgen]
pub async fn clear_opendata_cache(service_name: &str) -> Result<(), JsValue> {
    let mut store = store_take().await?;
    store.remove_opendata(service_name).map_err(js_err)?;
    store_put_back(store);
    Ok(())
}

const POLYGON_GROUPS_KEY: &str = "polygon_groups";

#[wasm_bindgen]
pub async fn get_polygon_groups() -> Result<JsValue, JsValue> {
    info!("[wasm] get_polygon_groups");
    let store = store_take().await?;
    let raw = store.get_raw(POLYGON_GROUPS_KEY).map_err(js_err)?;
    store_put_back(store);
    match raw {
        Some(raw) => js_sys::JSON::parse(&raw),
        None => Ok(JsValue::NULL),
    }
}

#[wasm_bindgen]
pub async fn save_polygon_groups(payload: JsValue) -> Result<(), JsValue> {
    let raw = js_sys::JSON::stringify(&payload)?
        .as_string()
        .ok_or_else(|| JsValue::from_str("Failed to stringify polygon groups"))?;
    let mut store = store_take().await?;
    store.put_raw(POLYGON_GROUPS_KEY, &raw).map_err(js_err)?;
    store_put_back(store);
    Ok(())
}

/// Returns a summary of the database for the DB explorer: table row counts,
/// total size, and per-service counts for roadworks and opendata.
#[wasm_bindgen]
pub async fn get_db_overview() -> Result<JsValue, JsValue> {
    info!("[wasm] get_db_overview");
    let store = store_take().await?;
    let overview = store.overview().map_err(js_err)?;
    store_put_back(store);
    serialize_data(&overview)
}

/// Reads a paginated page of `table` for the DB explorer.
#[wasm_bindgen]
pub async fn get_db_table(table: &str, offset: u32, limit: u32) -> Result<JsValue, JsValue> {
    info!("[wasm] get_db_table {table} offset={offset} limit={limit}");
    let store = store_take().await?;
    let data = store
        .table_rows(table, offset as i64, limit as i64)
        .map_err(js_err)?;
    store_put_back(store);
    serialize_data(&data)
}

/// Deletes a row of `table`, identified by `keys_json` (a JSON array of
/// `[column, value]` pairs). Deleting a `cache` row also removes the linked
/// cached rows for the same service.
#[wasm_bindgen]
pub async fn delete_db_row(table: &str, keys_json: &str) -> Result<usize, JsValue> {
    info!("[wasm] delete_db_row {table}");
    let keys: Vec<(String, serde_json::Value)> = serde_json::from_str(keys_json)
        .map_err(|e| JsValue::from_str(&format!("Invalid delete keys: {e}")))?;
    let mut store = store_take().await?;
    let deleted = store.delete_table_row(table, &keys).map_err(js_err)?;
    store_put_back(store);
    Ok(deleted)
}
