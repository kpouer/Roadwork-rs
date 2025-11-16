use crate::opendata::json::model::metadata::Metadata;
use eframe::epaint::text::TextWrapMode;
use egui::{Context, Response, RichText};
use egui::{Label, Ui};

pub(crate) struct MetadataDialog<'a> {
    open: &'a mut bool,
    metadata: &'a Metadata,
}

impl<'a> MetadataDialog<'a> {
    pub(crate) fn new(open: &'a mut bool, metadata: &'a Metadata) -> Self {
        Self { open, metadata }
    }

    pub(crate) fn show(&mut self, ctx: &Context) {
        let screen = ctx.content_rect().size();
        let max = egui::vec2(screen.x * 0.9, screen.y * 0.9);
        egui::Window::new("Source info")
            .open(self.open)
            .resizable(true)
            .max_size(max)
            .show(ctx, |ui| {
                egui::Grid::new("metadata_grid")
                    .num_columns(2)
                    .spacing([6.0, 4.0])
                    .show(ui, |ui| {
                        Self::add_row(ui, "Name:", self.metadata.name());
                        Self::add_row(ui, "Country:", self.metadata.country());

                        if let Some(p) = self.metadata.producer() {
                            Self::add_row(ui, "Producer:", p);
                        }

                        if let Some(lic) = self.metadata.licence_name() {
                            ui.label(RichText::new("License:").strong());
                            if let Some(url) = self.metadata.licence_url() {
                                // Show license name, and a clickable URL below it
                                ui.vertical(|ui| {
                                    ui.add(Label::new(lic).wrap_mode(TextWrapMode::Wrap));
                                    if Self::show_link(ui, url).clicked() {
                                        let _ = open::that(url);
                                    }
                                });
                            } else {
                                ui.add(Label::new(lic).wrap_mode(TextWrapMode::Wrap));
                            }
                            ui.end_row();
                        }

                        Self::add_row_link(ui, "Source URL:", self.metadata.source_url());
                        Self::add_row_link(ui, "API URL:", &self.metadata.url);

                        if let Some(locale) = self.metadata.locale_str() {
                            Self::add_row(ui, "Locale:", locale);
                        }

                        if let Some(ts) = self.metadata.tile_server() {
                            Self::add_row(ui, "Tile server:", ts);
                        }

                        if let Some(pattern) = &self.metadata.editor_pattern {
                            Self::add_row(ui, "Editor pattern:", pattern);
                        }

                        Self::add_row(
                            ui,
                            "Center:",
                            &format!(
                                "lat: {:.5}, lon: {:.5}",
                                self.metadata.center.lat, self.metadata.center.lon
                            ),
                        );
                    });
            });
    }

    fn add_row_link(ui: &mut Ui, label: &str, value: &str) {
        ui.label(RichText::new(label).strong());
        if Self::show_link(ui, value).clicked() {
            let _ = open::that(value);
        }
        ui.end_row();
    }

    fn show_link(ui: &mut Ui, url: &str) -> Response {
        ui.add(
            Label::new(
                RichText::new(url)
                    .underline()
                    .color(ui.visuals().hyperlink_color),
            )
            .wrap_mode(TextWrapMode::Wrap)
            .sense(egui::Sense::click()),
        )
    }

    fn add_row(ui: &mut Ui, label: &str, value: &str) {
        ui.label(RichText::new(label).strong());
        ui.add(Label::new(value).wrap_mode(TextWrapMode::Wrap));
        ui.end_row();
    }
}
