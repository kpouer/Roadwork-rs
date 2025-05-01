use crate::model::roadwork::Roadwork;
use egui::Ui;
use roadwork_sync::Status;

pub(crate) struct StatusPanel<'a> {
    roadwork: &'a mut Roadwork,
}

impl<'a> StatusPanel<'a> {
    pub(crate) fn new(roadwork: &'a mut Roadwork) -> Self {
        Self { roadwork }
    }

    pub(crate) fn show(self, ui: &mut Ui) {
        egui::Grid::new("status_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.radio_value(
                    &mut self.roadwork.sync_data.status,
                    Status::New,
                    Status::New.to_string(),
                );
                ui.radio_value(
                    &mut self.roadwork.sync_data.status,
                    Status::Later,
                    Status::Later.to_string(),
                );
                ui.end_row();
                ui.radio_value(
                    &mut self.roadwork.sync_data.status,
                    Status::Ignored,
                    Status::Ignored.to_string(),
                );
                ui.radio_value(
                    &mut self.roadwork.sync_data.status,
                    Status::Finished,
                    Status::Finished.to_string(),
                );
                ui.end_row();
                ui.radio_value(
                    &mut self.roadwork.sync_data.status,
                    Status::Treated,
                    Status::Treated.to_string(),
                );
                ui.end_row();
            });
    }
}
