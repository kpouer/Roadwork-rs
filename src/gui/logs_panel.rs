use egui::Context;
use egui_logger::LoggerUi;

pub(crate) struct LogsPanel<'a> {
    open: &'a mut bool,
}

impl<'a> LogsPanel<'a> {
    pub(crate) fn new(open: &'a mut bool) -> Self {
        Self { open }
    }

    pub(crate) fn show_button(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        if ui.button("Logs panel").clicked() {
            *self.open = true;
        }
        if *self.open {
            self.show(ctx);
        }
    }
    
    pub(crate) fn show(&mut self, ctx: &Context) {
        egui::Window::new("Logs panel")
            .open(self.open)
            .show(ctx, |ui| {
                LoggerUi::default()
                    .show(ui);
            });
    }
}
