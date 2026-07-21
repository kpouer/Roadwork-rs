use crate::state::AppState;
use axum::Json;
use axum::extract::State;
use log::info;

pub async fn trigger_sync(State(state): State<AppState>) -> Json<serde_json::Value> {
    info!("POST /api/sync");

    let service_name = state
        .default_service
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| "France-Paris".to_string());

    let mut data = state.get_or_fetch_roadworks(&service_name).await;
    if let Some(data) = &mut data {
        state.synchronize(data).await;
        let _ = state.storage.save_cache(&service_name, &data).await;
    }

    Json(serde_json::json!({ "status": "synced", "service": service_name }))
}
