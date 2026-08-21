pub mod http_service;
pub mod json_tools;
pub mod model;
pub mod opendata;
pub mod settings;

pub fn now_millis() -> u64 {
    chrono::Utc::now().timestamp_millis() as u64
}
