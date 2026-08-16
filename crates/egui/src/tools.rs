use roadwork_core::opendata::json::model::lat_lng::LatLng;
use std::collections::HashMap;
use walkers::Position;

pub fn url_params_to_vec(params: &Option<HashMap<String, String>>) -> Vec<(String, String)> {
    let mut vec: Vec<(String, String)> = params
        .iter()
        .flat_map(|map| map.iter())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    vec.sort_by(|a, b| a.0.cmp(&b.0));
    vec
}

pub fn format_bytes(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    if bytes < KB as usize {
        format!("{bytes} B")
    } else if (bytes as f64) < KB * KB {
        format!("{:.1} KB", bytes as f64 / KB)
    } else {
        format!("{:.1} MB", bytes as f64 / (KB * KB))
    }
}

include!(concat!(env!("OUT_DIR"), "/build_color.rs"));

pub(crate) fn build_color() -> egui::Color32 {
    egui::Color32::from_rgb(BUILD_COLOR_R, BUILD_COLOR_G, BUILD_COLOR_B)
}

pub fn latlng_to_position(ll: LatLng) -> Position {
    walkers::lat_lon(ll.lat, ll.lon)
}

pub fn position_to_latlng(p: Position) -> LatLng {
    LatLng {
        lat: p.y(),
        lon: p.x(),
    }
}
