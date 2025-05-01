use std::string::ToString;

const DEFAULT_OPENDATA_SERVICE: &str = "France-Paris";

#[derive(Debug)]
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

impl Default for Settings {
    fn default() -> Self {
        Self {
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
        }
    }
}
