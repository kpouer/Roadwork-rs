use egui::Context;

pub(crate) struct AboutDialog<'a> {
    open: &'a mut bool,
}

impl<'a> AboutDialog<'a> {
    pub(crate) fn new(open: &'a mut bool) -> Self {
        Self { open }
    }

    pub(crate) fn show_button(&mut self, ctx: &Context, ui: &mut egui::Ui) {
        if ui.button("?").clicked() {
            *self.open = true;
        }
        if *self.open {
            self.show(ctx);
        }
    }

    fn show(&mut self, ctx: &Context) {
        egui::Window::new("About Disk Mosaic")
            .open(self.open)
            .show(ctx, |ui| {
                ui.label("Disk Mosaic");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label("Created by Matthieu Casanova");
            });
    }
}
