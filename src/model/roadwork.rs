use crate::model::wkt::polygon::Polygon;
use roadwork_sync::SyncData;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Roadwork structure
/// it is serialized as a cache on disk
#[derive(Default, Debug, Deserialize, Serialize)]
pub(crate) struct Roadwork {
    pub(crate) id: String,
    pub(crate) latitude: f64,
    pub(crate) longitude: f64,
    pub(crate) polygons: Option<Vec<Polygon>>,
    pub(crate) start: i64,
    pub(crate) end: i64,
    pub(crate) road: Option<String>,
    #[serde(rename = "locationDetails")]
    pub(crate) location_details: Option<String>,
    #[serde(rename = "impactCirculationDetail")]
    pub(crate) impact_circulation_detail: Option<String>,
    pub(crate) description: Option<String>,
    #[serde(rename = "syncData")]
    pub(crate) sync_data: SyncData,
    pub(crate) url: String,
}

impl Roadwork {
    pub(crate) fn is_expired(&self) -> bool {
        Duration::from_millis(self.end as u64) < SystemTime::now().duration_since(UNIX_EPOCH).unwrap()
    }
}
