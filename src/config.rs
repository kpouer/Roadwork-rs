use home::home_dir;
use log::info;
use std::path::PathBuf;

pub(crate) struct Config {
    pub(crate) dataPath: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        info!("Config start");
        let dataPath = if let Some(mut user_home) = home_dir() {
            user_home.push(".roadwork");
            user_home
        } else {
            PathBuf::from("data")
        };

        Self { dataPath }
    }
}
