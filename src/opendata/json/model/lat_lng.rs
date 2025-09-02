use serde::{Deserialize, Serialize};
use walkers::Position;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub(crate) struct LatLng {
    pub(crate) lat: f64,
    pub(crate) lon: f64,
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

impl From<LatLng> for Position {
    fn from(value: LatLng) -> Self {
        walkers::lat_lon(value.lat, value.lon)
    }
}

impl From<Position> for LatLng {
    fn from(value: Position) -> Self {
        // Assuming Position exposes latitude/longitude accessors
        // If API differs, adjust to correct accessors/fields.
        Self {
            lat: value.y(),
            lon: value.x(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_lat_lng() {
        let lat_lng = LatLng::default();
        assert_eq!(lat_lng.lat, LAT_PARIS);
        assert_eq!(lat_lng.lon, LON_PARIS);
    }

    #[test]
    fn test_from_lat_lng() {
        let lat_lng = LatLng {
            lat: LAT_PARIS,
            lon: LON_PARIS,
        };
        let position: Position = lat_lng.into();
        assert_eq!(position, walkers::lat_lon(LAT_PARIS, LON_PARIS));
    }

    #[test]
    fn test_deserialize_lat_lng() {
        let json = r#"{"lat": 48.85337, "lon": 2.34847}"#;
        let lat_lng: LatLng = serde_json::from_str(json).unwrap();
        assert_eq!(lat_lng.lat, LAT_PARIS);
        assert_eq!(lat_lng.lon, LON_PARIS);
    }
}
