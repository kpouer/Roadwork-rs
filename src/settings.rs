use log::info;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::PathBuf;
use std::string::ToString;

const DEFAULT_OPENDATA_SERVICE: &str = "France-Paris";

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Settings {
    pub(crate) opendataService: String,
    pub(crate) synchronizationUrl: String,
    pub(crate) synchronizationTeam: String,
    frameX: u32,
    frameY: u32,
    frameWidth: u16,
    frameHeight: u16,
    pub(crate) synchronizationEnabled: bool,
    pub(crate) synchronizationLogin: String,
    pub(crate) synchronizationPassword: String,

    pub(crate) hide_expired: bool,
}

// todo: load & save
impl Default for Settings {
    fn default() -> Self {
        Self::settings_file()
            .and_then(|settings_file| File::open(settings_file).ok())
            .and_then(|settings_file| serde_json::from_reader::<File, Settings>(settings_file).ok())
            .unwrap_or(Self {
                opendataService: DEFAULT_OPENDATA_SERVICE.to_string(),
                synchronizationUrl: "".to_string(),
                synchronizationTeam: "".to_string(),
                frameX: 0,
                frameY: 0,
                frameWidth: 0,
                frameHeight: 0,
                synchronizationEnabled: false,
                synchronizationLogin: "".to_string(),
                synchronizationPassword: "".to_string(),
                hide_expired: false,
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

    pub(crate) fn settings_folder() -> Option<PathBuf> {
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
