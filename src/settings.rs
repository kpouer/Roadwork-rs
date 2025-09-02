use crate::opendata::json::model::lat_lng::LatLng;
use log::info;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::PathBuf;
use std::string::ToString;

const DEFAULT_OPENDATA_SERVICE: &str = "France-Paris";

#[derive(Debug, Deserialize, Serialize)]
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

// todo: load & save
impl Default for Settings {
    fn default() -> Self {
        Self::settings_file()
            .and_then(|settings_file| File::open(settings_file).ok())
            .and_then(|settings_file| serde_json::from_reader::<File, Settings>(settings_file).ok())
            .unwrap_or(Self {
                opendata_service: DEFAULT_OPENDATA_SERVICE.to_string(),
                synchronization_url: "".to_string(),
                synchronization_team: "".to_string(),
                synchronization_enabled: false,
                synchronization_login: "".to_string(),
                synchronization_password: "".to_string(),
                hide_expired: false,
                map_center: None,
                map_zoom: None,
            })
    }
}

impl Settings {
    pub(crate) fn save(&self) -> Result<(), std::io::Error> {
        info!("save");
        if let Some(settings_folder) = Self::settings_folder() {
            std::fs::create_dir_all(settings_folder)?;
            if let Some(settings_file) = Self::settings_file() {
                serde_json::to_writer(File::create(settings_file)?, self)?;
            }
        }
        Ok(())
    }

    pub fn settings_folder() -> Option<PathBuf> {
        home::home_dir().map(|mut home| {
            home.push(".roadwork");
            home
        })
    }

    fn settings_file() -> Option<PathBuf> {
        Self::settings_folder().map(|mut settings_folder| {
            settings_folder.push("settings.json");
            settings_folder
        })
    }
}
