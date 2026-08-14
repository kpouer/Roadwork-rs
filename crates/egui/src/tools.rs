use std::collections::HashMap;

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
