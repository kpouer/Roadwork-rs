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

    pub fn show(&mut self, ui: &mut Ui) -> bool {
        let mut changed = false;
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
                    changed = true;
                }
                if ui
                    .radio_value(
                        &mut self.roadwork.sync_data.status,
                        Status::Later,
                        Status::Later.to_string(),
                    )
                    .changed()
                {
                    changed = true;
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
                    changed = true;
                }
                if ui
                    .radio_value(
                        &mut self.roadwork.sync_data.status,
                        Status::Finished,
                        Status::Finished.to_string(),
                    )
                    .changed()
                {
                    changed = true;
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
                    changed = true;
                }
                ui.end_row();
            });
        if changed {
            self.roadwork.sync_data.set_dirty(true);
        }
        changed
    }
}
