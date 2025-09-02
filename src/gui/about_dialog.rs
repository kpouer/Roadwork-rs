use egui::Context;

pub(crate) struct AboutDialog<'a> {
    open: &'a mut bool,
}

impl<'a> AboutDialog<'a> {
    pub(crate) fn new(open: &'a mut bool) -> Self {
        Self { open }
    }

    pub(crate) fn show(&mut self, ctx: &Context) {
        egui::Window::new("About Roadwork")
            .open(self.open)
            .show(ctx, |ui| {
                ui.label("Roadwork");
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
                ui.label("Created by Matthieu Casanova");
            });
    }
}
