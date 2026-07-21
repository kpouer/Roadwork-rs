use axum::Json;
use axum::extract::Query;
use axum::extract::State;
use log::info;
use roadwork_core::model::roadwork_data::RoadworkData;
use std::collections::HashMap;

use crate::state::AppState;

#[derive(serde::Deserialize)]
pub struct RoadworksQuery {
    pub service: Option<String>,
}

pub async fn get_roadworks(
    State(state): State<AppState>,
    Query(query): Query<RoadworksQuery>,
) -> Json<RoadworkData> {
    let service_name = query.service.unwrap_or_else(|| {
        state
            .default_service
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| "France-Paris".to_string())
    });
    info!("GET /api/roadworks for {service_name}");

    let data = state
        .get_or_fetch_roadworks(&service_name)
        .await
        .unwrap_or(RoadworkData {
            source: service_name.to_string(),
            roadworks: HashMap::new(),
            created: 0,
        });
    Json(data)
}

pub async fn refresh_roadworks(
    State(state): State<AppState>,
    Query(query): Query<RoadworksQuery>,
) -> Json<RoadworkData> {
    let service_name = query.service.unwrap_or_else(|| {
        state
            .default_service
            .lock()
            .unwrap()
            .clone()
            .unwrap_or_else(|| "France-Paris".to_string())
    });

    info!("POST /api/roadworks/refresh for {service_name}");

    let data = state
        .refresh_service(&service_name)
        .await
        .unwrap_or_else(|| RoadworkData {
            source: service_name,
            roadworks: Default::default(),
            created: 0,
        });
    Json(data)
}

pub async fn refresh_all_roadworks(State(state): State<AppState>) -> Json<serde_json::Value> {
    info!("POST /api/roadworks/refresh-all");
    state.refresh_all().await;
    Json(serde_json::json!({ "status": "refreshed" }))
}
