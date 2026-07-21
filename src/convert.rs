use roadwork_core::opendata::json::model::lat_lng::LatLng;
use walkers::Position;

pub fn latlng_to_position(ll: LatLng) -> Position {
    walkers::lat_lon(ll.lat, ll.lon)
}

pub fn position_to_latlng(p: Position) -> LatLng {
    LatLng {
        lat: p.y(),
        lon: p.x(),
    }
}
