use egui::Context;
use egui::RichText;
use roadwork_core::settings::Settings;

pub(crate) struct SettingsDialog<'a> {
    open: &'a mut bool,
    settings: &'a mut Settings,
}

impl<'a> SettingsDialog<'a> {
    pub(crate) fn new(open: &'a mut bool, settings: &'a mut Settings) -> Self {
        Self { open, settings }
    }

    pub(crate) fn show(&mut self, ctx: &Context) {
        let screen = ctx.content_rect().size();
        let max = egui::vec2(screen.x * 0.6, screen.y * 0.8);
        let mut save = false;
        egui::Window::new("Settings")
            .open(self.open)
            .max_size(max)
            .show(ctx, |ui| {
                ui.label(RichText::new("Default service").strong());
                ui.text_edit_singleline(&mut self.settings.opendata_service);

                ui.separator();
                ui.label(RichText::new("Synchronization").strong());
                ui.checkbox(&mut self.settings.synchronization_enabled, "Enabled");
                egui::Grid::new("settings_grid")
                    .num_columns(2)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label("URL:");
                        ui.text_edit_singleline(&mut self.settings.synchronization_url);
                        ui.end_row();
                        ui.label("Team:");
                        ui.text_edit_singleline(&mut self.settings.synchronization_team);
                        ui.end_row();
                        ui.label("Login:");
                        ui.text_edit_singleline(&mut self.settings.synchronization_login);
                        ui.end_row();
                        ui.label("Password:");
                        ui.text_edit_singleline(&mut self.settings.synchronization_password);
                        ui.end_row();
                    });

                ui.separator();
                if ui.button("Save").clicked() {
                    crate::app_settings::save_settings(self.settings);
                    save = true;
                }
            });
        if save {
            *self.open = false;
        }
    }
}
