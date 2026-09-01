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
    static INDEX_DESCRIPTORS: RefCell<HashMap<String, ServiceDescriptor>> =
        RefCell::new(HashMap::new());
    /// Relative file path (with `.json`) of each index descriptor, keyed by key,
    /// kept so source URLs can be built from the physical file location.
    static INDEX_PATHS: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
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

/// Result of [`sync_index`]: the full services list, which keys are new or updated,
/// and the full modified map to persist client-side.
#[derive(Serialize)]
struct SyncIndexResult {
    services: Vec<ServiceInfo>,
    new_or_updated: Vec<String>,
    known_modified: HashMap<String, String>,
}

/// Fetches the remote roadwork index from GitHub, compares each entry's
/// `modified` timestamp against `known_modified` (a map of key → ISO date),
/// fetches descriptors for new/updated entries, stores them in
/// `INDEX_DESCRIPTORS`, and returns the merged services list.
#[wasm_bindgen]
pub async fn sync_index(known_modified: JsValue) -> Result<JsValue, JsValue> {
    info!("[wasm] sync_index");
    let known: HashMap<String, String> = serde_wasm_bindgen::from_value(known_modified)
        .map_err(|e| JsValue::from_str(&format!("Invalid known_modified: {e}")))?;

    let index = roadwork_service::fetch_index()
        .await
        .map_err(|e| JsValue::from_str(&format!("Failed to fetch index: {e}")))?;

    INDEX_PATHS.with(|cell| {
        let mut cell = cell.borrow_mut();
        for entry in &index.files {
            cell.insert(entry.key.clone(), entry.path.clone());
        }
    });

    let mut new_or_updated = Vec::new();

    for entry in &index.files {
        let is_new_or_updated = match known.get(&entry.key) {
            Some(known_mod) => match &entry.modified {
                Some(modified) => modified != known_mod,
                None => true,
            },
            None => true,
        };
        if is_new_or_updated {
            match roadwork_service::fetch_descriptor(&entry.path).await {
                Ok(descriptor) => {
                    info!("[wasm] sync_index: fetched {}", entry.key);
                    INDEX_DESCRIPTORS.with(|cell| {
                        cell.borrow_mut().insert(entry.key.clone(), descriptor);
                    });
                    new_or_updated.push(entry.key.clone());
                }
                Err(e) => {
                    log::warn!("[wasm] sync_index: failed to fetch {}: {e}", entry.key);
                }
            }
        }
    }

    // Prune descriptors that no longer exist in the index, so stale sources
    // (moved to broken/ or removed) disappear from the services list.
    let valid_keys: std::collections::HashSet<&str> =
        index.files.iter().map(|e| e.key.as_str()).collect();
    INDEX_DESCRIPTORS.with(|cell| {
        let mut cell = cell.borrow_mut();
        let stale: Vec<String> = cell
            .keys()
            .filter(|k| !valid_keys.contains(k.as_str()))
            .cloned()
            .collect();
        for key in stale {
            log::warn!("[wasm] sync_index: removing stale descriptor {}", key);
            cell.remove(&key);
        }
    });

    let descriptors = all_descriptors();
    let mut services: Vec<ServiceInfo> = descriptors
        .into_iter()
        .map(|(name, desc)| ServiceInfo {
            name,
            label: desc.metadata.label(),
            center: desc.metadata.center,
        })
        .collect();
    services.sort_by(|a, b| a.name.cmp(&b.name));

    // Build the full known_modified map from the index so the client can persist it.
    let mut known_modified = HashMap::new();
    for entry in &index.files {
        if let Some(modified) = &entry.modified {
            known_modified.insert(entry.key.clone(), modified.clone());
        }
    }

    let result = SyncIndexResult {
        services,
        new_or_updated,
        known_modified,
    };
    serialize_data(&result)
}

fn all_descriptors() -> HashMap<String, ServiceDescriptor> {
    let mut map = roadwork_service::load_descriptors();
    INDEX_DESCRIPTORS.with(|cell| {
        for (name, descriptor) in cell.borrow().iter() {
            map.insert(name.clone(), descriptor.clone());
        }
    });
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
            label: desc.metadata.label(),
            center: desc.metadata.center,
        })
        .collect();
    services.sort_by(|a, b| a.name.cmp(&b.name));
    serde_wasm_bindgen::to_value(&services).unwrap()
}

/// Display information about an embedded opendata source, as shown in the
/// extension About window.
#[derive(Debug, Clone, Serialize)]
pub struct SourceInfo {
    /// Service key of the descriptor (the metadata `id`, or the file path
    /// without the `.json` extension).
    pub name: String,
    pub country: Option<String>,
    pub source_name: String,
    pub producer: Option<String>,
    pub licence_name: Option<String>,
    pub licence_url: Option<String>,
    pub source_url: Option<String>,
    pub descriptor_url: Option<String>,
}

/// Returns the display information of every opendata source (built-in and
/// index/custom roadwork descriptors), sorted by country then service name.
#[wasm_bindgen]
pub fn get_sources_info() -> JsValue {
    info!("[wasm] get_sources_info");
    let builtin_paths = roadwork_service::builtin_paths();
    let mut sources: Vec<SourceInfo> = all_descriptors()
        .into_iter()
        .map(|(name, desc)| {
            // The descriptor source URL points at the physical file: the index
            // `path` for remotely fetched descriptors, the embedded relative
            // path for built-ins. Unknown paths (e.g. local/custom) yield None.
            let rel_path = INDEX_PATHS
                .with(|cell| cell.borrow().get(&name).cloned())
                .or_else(|| builtin_paths.get(&name).cloned());
            SourceInfo {
                name,
                country: desc.metadata.country,
                source_name: desc.metadata.name,
                producer: desc.metadata.producer,
                licence_name: desc.metadata.licence_name,
                licence_url: desc.metadata.licence_url,
                source_url: desc.metadata.source_url,
                descriptor_url: rel_path
                    .map(|path| format!("{}{path}", roadwork_service::DESCRIPTORS_BASE_URL)),
            }
        })
        .collect();
    sources.sort_by(|a, b| {
        a.country
            .as_deref()
            .unwrap_or_default()
            .cmp(b.country.as_deref().unwrap_or_default())
            .then_with(|| a.name.cmp(&b.name))
    });
    serde_wasm_bindgen::to_value(&sources).unwrap()
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
pub async fn get_roadworks_cached(service_name: &str) -> Result<JsValue, JsValue> {
    info!("[wasm] get_roadworks_cached");
    let data = {
        let store = store_take().await?;
        let data = store
            .get_roadworks(service_name)
            .map_err(js_err)?
            .ok_or_else(|| JsValue::from_str(&format!("No cached roadworks for {service_name}")))?;
        store_put_back(store);
        data
    };
    info!(
        "[wasm] get_roadworks_cached: data loaded {}",
        data.roadworks.len()
    );
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
pub async fn get_opendata(service_name: &str, force_refresh: bool) -> Result<JsValue, JsValue> {
    info!("[wasm] get_opendata force_refresh={force_refresh}");
    let data = {
        let mut store = store_take().await?;
        let source = store
            .get_opendata_source(service_name)
            .map_err(js_err)?
            .ok_or_else(|| {
                JsValue::from_str(&format!("Unknown opendata service: {service_name}"))
            })?;
        let descriptor: ServiceDescriptor =
            serde_json::from_str(&source.descriptor).map_err(|e| {
                JsValue::from_str(&format!("Invalid descriptor for {service_name}: {e}"))
            })?;
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

/// Returns every custom opendata source (name, descriptor, flags).
#[wasm_bindgen]
pub async fn get_opendata_sources() -> Result<JsValue, JsValue> {
    info!("[wasm] get_opendata_sources");
    let store = store_take().await?;
    let sources = store.list_opendata_sources().map_err(js_err)?;
    store_put_back(store);
    serialize_data(&sources)
}

/// Creates or updates a custom opendata source. When `old_name` differs from
/// `name`, the previous source (definition and cached data) is removed first.
#[wasm_bindgen]
pub async fn save_opendata_source(
    name: &str,
    descriptor: &str,
    enabled: bool,
    visible: bool,
    old_name: Option<String>,
) -> Result<(), JsValue> {
    info!("[wasm] save_opendata_source {name} old_name={old_name:?}");
    serde_json::from_str::<ServiceDescriptor>(descriptor)
        .map_err(|e| JsValue::from_str(&format!("Invalid opendata descriptor: {e}")))?;
    let mut store = store_take().await?;
    if let Some(old_name) = &old_name
        && old_name != name
    {
        store.remove_opendata(old_name).map_err(js_err)?;
    }
    store
        .upsert_opendata_source(name, descriptor, enabled, visible)
        .map_err(js_err)?;
    store_put_back(store);
    Ok(())
}

/// Updates the `enabled`/`visible` flags of a custom opendata source.
#[wasm_bindgen]
pub async fn set_opendata_source_flags(
    name: &str,
    enabled: bool,
    visible: bool,
) -> Result<(), JsValue> {
    info!("[wasm] set_opendata_source_flags {name}");
    let mut store = store_take().await?;
    store
        .set_opendata_source_flags(name, enabled, visible)
        .map_err(js_err)?;
    store_put_back(store);
    Ok(())
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

/// Reads a paginated page of `table` for the DB explorer. When the optional
/// `service` is given and `table` has a `service` column, only the rows of that
/// service are returned. When the optional
/// `lat_min`/`lon_min`/`lat_max`/`lon_max` bounds are all given and `table`
/// has `latitude`/`longitude` columns, only the rows inside that rectangle are
/// returned.
#[allow(clippy::too_many_arguments)]
#[wasm_bindgen]
pub async fn get_db_table(
    table: &str,
    offset: u32,
    limit: u32,
    lat_min: Option<f64>,
    lon_min: Option<f64>,
    lat_max: Option<f64>,
    lon_max: Option<f64>,
    service: Option<String>,
) -> Result<JsValue, JsValue> {
    info!(
        "[wasm] get_db_table {table} offset={offset} limit={limit} service={service:?} bbox={lat_min:?}/{lon_min:?}/{lat_max:?}/{lon_max:?}"
    );
    let bbox = match (lat_min, lon_min, lat_max, lon_max) {
        (Some(lat_min), Some(lon_min), Some(lat_max), Some(lon_max)) => {
            Some((lat_min, lon_min, lat_max, lon_max))
        }
        _ => None,
    };
    let store = store_take().await?;
    let data = store
        .table_rows(table, offset as i64, limit as i64, service.as_deref(), bbox)
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
