use serde::Deserialize;
use walkers::{Position, lat_lon};

#[derive(Debug, Clone, Copy, Deserialize)]
pub(crate) struct LatLng {
    lat: f64,
    lon: f64,
}

impl Default for LatLng {
    fn default() -> Self {
        Self::PARIS
    }
}

impl LatLng {
    pub const PARIS: Self = Self {
        lat: 48.85337,
        lon: 2.34847,
    };
}

impl From<LatLng> for Position {
    fn from(value: LatLng) -> Self {
        lat_lon(value.lat, value.lon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_lat_lng() {
        let lat_lng = LatLng::default();
        assert_eq!(lat_lng.lat, 48.85337);
        assert_eq!(lat_lng.lon, 2.34847);
    }

    #[test]
    fn test_from_lat_lng() {
        let lat_lng = LatLng { lat: 48.85337, lon: 2.34847 };
        let position: Position = lat_lng.into();
        assert_eq!(position, lat_lon(48.85337, 2.34847));
    }

    #[test]
    fn test_deserialize_lat_lng() {
        let json = r#"{"lat": 48.85337, "lon": 2.34847}"#;
        let lat_lng: LatLng = serde_json::from_str(json).unwrap();
        assert_eq!(lat_lng.lat, 48.85337);
        assert_eq!(lat_lng.lon, 2.34847);
    }
}