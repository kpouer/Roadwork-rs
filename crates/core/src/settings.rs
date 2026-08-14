use crate::opendata::json::model::lat_lng::LatLng;
use crate::opendata::json::model::service_descriptor::ServiceDescriptor;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const DEFAULT_OPENDATA_SERVICE: &str = "France-Paris";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    #[serde(rename = "opendataService")]
    pub opendata_service: String,
    #[serde(rename = "synchronizationUrl")]
    pub synchronization_url: String,
    #[serde(rename = "synchronizationTeam")]
    pub synchronization_team: String,
    #[serde(rename = "synchronizationEnabled")]
    pub synchronization_enabled: bool,
    #[serde(rename = "synchronizationLogin")]
    pub synchronization_login: String,
    #[serde(rename = "synchronizationPassword")]
    pub synchronization_password: String,

    #[serde(rename = "hide_expired")]
    pub hide_expired: bool,

    #[serde(rename = "mapCenter", default)]
    pub map_center: Option<LatLng>,

    #[serde(rename = "mapZoom", default)]
    pub map_zoom: Option<f64>,

    #[serde(rename = "opendataServices", default)]
    pub opendata_services: HashMap<String, ServiceDescriptor>,

    #[serde(rename = "selectedOpendataService", default)]
    pub selected_opendata_service: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            opendata_service: DEFAULT_OPENDATA_SERVICE.to_string(),
            synchronization_url: "".to_string(),
            synchronization_team: "".to_string(),
            synchronization_enabled: false,
            synchronization_login: "".to_string(),
            synchronization_password: "".to_string(),
            hide_expired: false,
            map_center: None,
            map_zoom: None,
            opendata_services: HashMap::new(),
            selected_opendata_service: None,
        }
    }
}
