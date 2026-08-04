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

use roadwork_core::model::service_info::ServiceInfo;
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;

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
pub async fn get_roadworks(service_name: &str) -> Result<JsValue, JsValue> {
    info!("[wasm] get_roadworks");
    let descriptors = all_descriptors();
    let descriptor = descriptors
        .get(service_name)
        .ok_or_else(|| JsValue::from_str(&format!("Unknown service: {service_name}")))?;

    let data = roadwork_service::fetch_roadworks(service_name, descriptor)
        .await
        .map_err(|e| JsValue::from_str(&format!("Fetch error: {e}")))?;
    info!("[wasm] get_roadworks: data loaded {}", data.roadworks.len());
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    data.serialize(&serializer)
        .map_err(|e| JsValue::from_str(&format!("Serialize error: {e}")))
}
