use eframe::epaint::text::TextWrapMode;
use egui::Context;
use egui::Label;

pub(crate) struct AboutDialog<'a> {
    open: &'a mut bool,
}

impl<'a> AboutDialog<'a> {
    pub(crate) fn new(open: &'a mut bool) -> Self {
        Self { open }
    }

    pub(crate) fn show(&mut self, ctx: &Context) {
        let screen = ctx.screen_rect().size();
        let max = egui::vec2(screen.x * 0.9, screen.y * 0.9);
        egui::Window::new("About Roadwork")
            .open(self.open)
            .max_size(max)
            .show(ctx, |ui| {
                ui.add(Label::new("Roadwork").wrap_mode(TextWrapMode::Wrap));
                ui.add(
                    Label::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                        .wrap_mode(TextWrapMode::Wrap),
                );
                ui.add(Label::new("Created by Matthieu Casanova").wrap_mode(TextWrapMode::Wrap));
            });
    }
}
