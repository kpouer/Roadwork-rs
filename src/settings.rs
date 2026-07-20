use crate::opendata::json::model::lat_lng::LatLng;
use serde::{Deserialize, Serialize};

const DEFAULT_OPENDATA_SERVICE: &str = "France-Paris";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    #[serde(rename = "opendataService")]
    pub(crate) opendata_service: String,
    #[serde(rename = "synchronizationUrl")]
    pub(crate) synchronization_url: String,
    #[serde(rename = "synchronizationTeam")]
    pub(crate) synchronization_team: String,
    #[serde(rename = "synchronizationEnabled")]
    pub(crate) synchronization_enabled: bool,
    #[serde(rename = "synchronizationLogin")]
    pub(crate) synchronization_login: String,
    #[serde(rename = "synchronizationPassword")]
    pub(crate) synchronization_password: String,

    #[serde(rename = "hide_expired")]
    pub(crate) hide_expired: bool,

    #[serde(rename = "mapCenter", default)]
    pub(crate) map_center: Option<LatLng>,

    #[serde(rename = "mapZoom", default)]
    pub(crate) map_zoom: Option<f64>,
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
        }
    }
}
