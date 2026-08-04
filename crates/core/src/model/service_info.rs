use crate::opendata::json::model::lat_lng::LatLng;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ServiceInfo {
    pub name: String,
    pub center: LatLng,
}
