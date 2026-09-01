use crate::opendata::json::model::lat_lng::LatLng;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ServiceInfo {
    /// Stable identity of the service (descriptor key).
    pub name: String,
    /// Human-readable display label (`<country> - <name>`).
    pub label: String,
    pub center: LatLng,
}
