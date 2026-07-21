use egui::Ui;
use roadwork_core::model::roadwork::Roadwork;
use roadwork_sync::Status;

pub struct StatusPanel<'a> {
    roadwork: &'a mut Roadwork,
}

impl<'a> StatusPanel<'a> {
    pub fn new(roadwork: &'a mut Roadwork) -> Self {
        Self { roadwork }
    }

    pub fn show(self, ui: &mut Ui) {
        let roadwork_id = self.roadwork.id.clone();
        egui::Grid::new("status_grid")
            .num_columns(2)
            .show(ui, |ui| {
                if ui
                    .radio_value(
                        &mut self.roadwork.sync_data.status,
                        Status::New,
                        Status::New.to_string(),
                    )
                    .changed()
                {
                    send_status_update(&roadwork_id, "New");
                }
                if ui
                    .radio_value(
                        &mut self.roadwork.sync_data.status,
                        Status::Later,
                        Status::Later.to_string(),
                    )
                    .changed()
                {
                    send_status_update(&roadwork_id, "Later");
                }
                ui.end_row();
                if ui
                    .radio_value(
                        &mut self.roadwork.sync_data.status,
                        Status::Ignored,
                        Status::Ignored.to_string(),
                    )
                    .changed()
                {
                    send_status_update(&roadwork_id, "Ignored");
                }
                if ui
                    .radio_value(
                        &mut self.roadwork.sync_data.status,
                        Status::Finished,
                        Status::Finished.to_string(),
                    )
                    .changed()
                {
                    send_status_update(&roadwork_id, "Finished");
                }
                ui.end_row();
                if ui
                    .radio_value(
                        &mut self.roadwork.sync_data.status,
                        Status::Treated,
                        Status::Treated.to_string(),
                    )
                    .changed()
                {
                    send_status_update(&roadwork_id, "Treated");
                }
                ui.end_row();
            });
    }
}

fn send_status_update(roadwork_id: &str, status: &str) {
    let id = roadwork_id.to_string();
    let status = status.to_string();
    wasm_bindgen_futures::spawn_local(async move {
        let url = format!("/api/roadworks/{}/status", id);
        let client = reqwest::Client::new();
        let body = serde_json::json!({ "status": &status });
        if let Err(e) = client.put(&url).json(&body).send().await {
            log::error!("Failed to update status for {id}: {e}");
        }
    });
}
