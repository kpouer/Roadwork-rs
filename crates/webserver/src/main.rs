mod api;
pub mod descriptor_manager;
mod state;
mod storage;

use crate::descriptor_manager::DescriptorManager;
use crate::storage::StorageError;
use axum::{
    Router,
    body::Body,
    http::{Response, StatusCode, header},
    response::IntoResponse,
    routing::{get, post, put},
};
use roadwork_service::SyncConfig;
use state::AppState;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use storage::SqliteStorage;
use tower_http::cors::{Any, CorsLayer};

include!(concat!(env!("OUT_DIR"), "/static_files.rs"));

#[tokio::main]
async fn main() -> Result<(), StorageError> {
    env_logger::init();

    let data_dir = std::env::var("DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("./data"));

    let storage = Arc::new(SqliteStorage::new(&data_dir).await?);

    let descriptor_manager = DescriptorManager::new(data_dir);

    let sync_config = load_sync_config();

    let state = AppState {
        descriptor_manager: Arc::new(descriptor_manager),
        storage: Arc::clone(&storage),
        sync_config,
        default_service: Arc::new(Mutex::new(Some("France-Paris".to_string()))),
    };

    let refresh_state = state.clone();
    tokio::spawn(async move {
        let interval_secs: u64 = std::env::var("REFRESH_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(86_400);
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        loop {
            interval.tick().await;
            log::info!("Daily refresh triggered");
            refresh_state.refresh_all().await;
        }
    });

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/services", get(api::services::get_services))
        .route("/api/roadworks", get(api::roadworks::get_roadworks))
        .route(
            "/api/roadworks/refresh",
            post(api::roadworks::refresh_roadworks),
        )
        .route(
            "/api/roadworks/refresh-all",
            post(api::roadworks::refresh_all_roadworks),
        )
        .route(
            "/api/roadworks/{team}/{id}/status",
            put(api::status::update_status),
        )
        .route("/api/sync", post(api::sync::trigger_sync))
        .route("/", get(index_handler))
        .fallback(static_handler)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    println!("Listening on {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();

    Ok(())
}

fn load_sync_config() -> Option<SyncConfig> {
    let url = std::env::var("SYNC_URL").ok()?;
    let team = std::env::var("SYNC_TEAM").unwrap_or_default();
    let login = std::env::var("SYNC_LOGIN").unwrap_or_default();
    let password = std::env::var("SYNC_PASSWORD").unwrap_or_default();

    Some(SyncConfig {
        enabled: true,
        url,
        team,
        login,
        password,
    })
}

fn find_file(name: &str) -> Option<&'static [u8]> {
    STATIC_FILES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, data)| *data)
}

async fn index_handler() -> impl IntoResponse {
    match find_file("index.html") {
        Some(data) => Response::builder()
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(data))
            .unwrap(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match find_file(path) {
        Some(data) => {
            let mime = mime_from_path(path);
            Response::builder()
                .header(header::CONTENT_TYPE, mime)
                .body(Body::from(data))
                .unwrap()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn mime_from_path(path: &str) -> &'static str {
    match path.rsplit('.').next() {
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("js") => "application/javascript; charset=utf-8",
        Some("wasm") => "application/wasm",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}
