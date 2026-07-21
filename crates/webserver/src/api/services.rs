use axum::Json;
use axum::extract::State;
use log::info;

use crate::state::AppState;

pub async fn get_services(State(state): State<AppState>) -> Json<Vec<String>> {
    info!("GET /api/services");
    let mut names = state.descriptor_manager.descriptor_names();
    names.sort();
    Json(names)
}
