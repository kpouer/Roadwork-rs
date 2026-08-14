use crate::model::opendata::Opendata;
use crate::now_millis;
use roadwork_sync::SyncData;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Deserialize, Serialize)]
pub struct Roadwork {
    pub opendata: Opendata,
    pub start: i64,
    pub end: i64,
    pub road: Option<String>,
    #[serde(rename = "locationDetails")]
    pub location_details: Option<String>,
    #[serde(rename = "impactCirculationDetail")]
    pub impact_circulation_detail: Option<String>,
    #[serde(rename = "syncData")]
    pub sync_data: SyncData,
}

impl Roadwork {
    pub fn is_expired(&self) -> bool {
        (self.end as u64) < now_millis()
    }
}
