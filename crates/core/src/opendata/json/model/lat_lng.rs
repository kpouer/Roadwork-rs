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
    fn test_deserialize_lat_lng() {
        let json = r#"{"lat": 48.85337, "lon": 2.34847}"#;
        let lat_lng: LatLng = serde_json::from_str(json).unwrap();
        assert_eq!(lat_lng.lat, LAT_PARIS);
        assert_eq!(lat_lng.lon, LON_PARIS);
    }
}
