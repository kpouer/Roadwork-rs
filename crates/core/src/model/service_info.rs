use crate::opendata::json::model::lat_lng::LatLng;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export)]
pub struct ServiceInfo {
    pub name: String,
    pub center: LatLng,
}
