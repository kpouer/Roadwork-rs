use log::info;
use serde::Serialize;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

use roadwork_core::opendata::json::model::lat_lng::LatLng;
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;
use roadwork_core::opendata::json::opendata_service::OpendataService;

#[derive(Serialize)]
struct ServiceInfo {
    name: String,
    center: LatLng,
}

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

mod descriptors {
    pub const BELGIUM_LIEGE: &str = include_str!("../../../opendata/json/Belgium-Liege.json");
    pub const FRANCE_44: &str =
        include_str!("../../../opendata/json/France-44-Loire-Atlantique.json");
    pub const FRANCE_72: &str = include_str!("../../../opendata/json/France-72-Sarthe.json");
    pub const FRANCE_AVIGNON: &str = include_str!("../../../opendata/json/France-Avignon.json");
    pub const FRANCE_BORDEAUX: &str = include_str!("../../../opendata/json/France-Bordeaux.json");
    pub const FRANCE_ISSY: &str =
        include_str!("../../../opendata/json/France-Issy-les-Moulineaux.json");
    pub const FRANCE_LYON: &str = include_str!("../../../opendata/json/France-Lyon.json");
    pub const FRANCE_MONTPELLIER: &str =
        include_str!("../../../opendata/json/France-Montpellier.json");
    pub const FRANCE_PARIS: &str = include_str!("../../../opendata/json/France-Paris.json");
    pub const FRANCE_PORNICHET: &str = include_str!("../../../opendata/json/France-Pornichet.json");
    pub const FRANCE_RENNES: &str = include_str!("../../../opendata/json/France-Rennes.json");
    pub const FRANCE_ROUEN: &str = include_str!("../../../opendata/json/France-Rouen.json");
    pub const FRANCE_TOULOUSE: &str = include_str!("../../../opendata/json/France-Toulouse.json");
    pub const GERMANY_BERLIN: &str = include_str!("../../../opendata/json/Germany-Berlin.json");
    pub const USA_SF: &str = include_str!("../../../opendata/json/USA-CA-SanFrancisco.json");
    pub const USA_CHICAGO: &str = include_str!("../../../opendata/json/USA-IL-Chicago.json");
}

fn load_descriptors() -> HashMap<String, ServiceDescriptor> {
    let raw: &[(&str, &str)] = &[
        ("Belgium-Liege", descriptors::BELGIUM_LIEGE),
        ("France-44-Loire-Atlantique", descriptors::FRANCE_44),
        ("France-72-Sarthe", descriptors::FRANCE_72),
        ("France-Avignon", descriptors::FRANCE_AVIGNON),
        ("France-Bordeaux", descriptors::FRANCE_BORDEAUX),
        ("France-Issy-les-Moulineaux", descriptors::FRANCE_ISSY),
        ("France-Lyon", descriptors::FRANCE_LYON),
        ("France-Montpellier", descriptors::FRANCE_MONTPELLIER),
        ("France-Paris", descriptors::FRANCE_PARIS),
        ("France-Pornichet", descriptors::FRANCE_PORNICHET),
        ("France-Rennes", descriptors::FRANCE_RENNES),
        ("France-Rouen", descriptors::FRANCE_ROUEN),
        ("France-Toulouse", descriptors::FRANCE_TOULOUSE),
        ("Germany-Berlin", descriptors::GERMANY_BERLIN),
        ("USA-CA-SanFrancisco", descriptors::USA_SF),
        ("USA-IL-Chicago", descriptors::USA_CHICAGO),
    ];

    let mut map = HashMap::new();
    for (name, json) in raw {
        match serde_json::from_str::<ServiceDescriptor>(json) {
            Ok(descriptor) => {
                map.insert(name.to_string(), descriptor);
            }
            Err(e) => {
                log::error!("Failed to parse descriptor {name}: {e}");
            }
        }
    }
    map
}

#[wasm_bindgen]
pub fn get_services() -> JsValue {
    info!("[wasm] get_services");
    let descriptors = load_descriptors();
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
    let descriptors = load_descriptors();
    let descriptor = descriptors
        .get(service_name)
        .ok_or_else(|| JsValue::from_str(&format!("Unknown service: {service_name}")))?;

    let ods = OpendataService::new(service_name.to_string(), descriptor.clone());
    let mut data = ods
        .get_data()
        .await
        .map_err(|e| JsValue::from_str(&format!("Fetch error: {e}")))?;
    info!("[wasm] get_roadworks: data loaded {}", data.roadworks.len());
    data.apply_finished_status();
    let serializer = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    data.serialize(&serializer)
        .map_err(|e| JsValue::from_str(&format!("Serialize error: {e}")))
}
