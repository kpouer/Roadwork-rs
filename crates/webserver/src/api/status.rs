use crate::state::AppState;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use log::info;
use roadwork_core::model::roadwork_data::RoadworkData;
use roadwork_sync::Status;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
pub struct StatusUpdate {
    pub status: Status,
}

pub async fn update_status(
    State(state): State<AppState>,
    Path((team, roadwork_id)): Path<(String, String)>,
    headers: HeaderMap,
    Json(update): Json<StatusUpdate>,
) -> Result<Json<RoadworkData>, StatusCode> {
    info!(
        "PUT /api/roadworks/{team}/{roadwork_id}/status -> {}",
        update.status
    );

    if let Some(sync_config) = &state.sync_config {
        let auth_header = headers.get("authorization").and_then(|v| v.to_str().ok());

        match auth_header {
            Some(header) if header.starts_with("Basic ") => {
                let encoded = &header[6..];
                match BASE64_STANDARD.decode(encoded) {
                    Ok(decoded) => {
                        if let Ok(credentials) = String::from_utf8(decoded) {
                            let parts: Vec<&str> = credentials.splitn(2, ':').collect();
                            if parts.len() == 2
                                && parts[0] == sync_config.login
                                && parts[1] == sync_config.password
                            {
                                // Auth valid, proceed
                            } else {
                                return Err(StatusCode::UNAUTHORIZED);
                            }
                        } else {
                            return Err(StatusCode::UNAUTHORIZED);
                        }
                    }
                    Err(_) => {
                        return Err(StatusCode::UNAUTHORIZED);
                    }
                }
            }
            _ => {
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
    }

    let status = update.status;

    let service_name = state
        .default_service
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| "France-Paris".to_string());

    let data = state.get_or_fetch_roadworks(&service_name).await;

    let data = match data {
        Some(mut data) => {
            if let Some(roadwork) = data.roadworks.get_mut(&roadwork_id) {
                roadwork.sync_data.status = status;
                roadwork.sync_data.set_dirty(true);
                let _ = state.storage.save_cache(&service_name, &data).await;
            }
            data
        }
        None => RoadworkData {
            source: service_name.to_string(),
            roadworks: HashMap::new(),
            created: 0,
        },
    };

    Ok(Json(data))
}
