use egui::Context;

pub(crate) struct LogsPanel<'a> {
    open: &'a mut bool,
}

impl<'a> LogsPanel<'a> {
    pub(crate) fn new(open: &'a mut bool) -> Self {
        Self { open }
    }

    pub(crate) fn show_button(&mut self, ui: &mut egui::Ui) {
        if ui.button("Logs (browser console)").clicked() {
            *self.open = true;
        }
        if *self.open {
            self.show(ui.ctx());
        }
    }

    pub(crate) fn show(&mut self, ctx: &Context) {
        egui::Window::new("Logs").open(self.open).show(ctx, |ui| {
            ui.label("Logs are available in the browser developer console (F12).");
            ui.label("Use LevelFilter::Info or higher to see logs.");
        });
    }
}
