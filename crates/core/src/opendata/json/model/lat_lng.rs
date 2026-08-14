use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct LatLng {
    pub lat: f64,
    pub lon: f64,
}

impl Default for LatLng {
    fn default() -> Self {
        Self::PARIS
    }
}

const LAT_PARIS: f64 = 48.85337;
const LON_PARIS: f64 = 2.34847;

impl LatLng {
    pub const PARIS: Self = Self {
        lat: LAT_PARIS,
        lon: LON_PARIS,
    };
}
