use crate::opendata::json::model::metadata::Metadata;
use eframe::epaint::text::TextWrapMode;
use egui::Label;
use egui::{Context, RichText};

pub(crate) struct MetadataDialog<'a> {
    open: &'a mut bool,
    metadata: &'a Metadata,
}

impl<'a> MetadataDialog<'a> {
    pub(crate) fn new(open: &'a mut bool, metadata: &'a Metadata) -> Self {
        Self { open, metadata }
    }

    pub(crate) fn show(&mut self, ctx: &Context) {
        let screen = ctx.screen_rect().size();
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
                        ui.label(RichText::new("Name:").strong());
                        ui.add(Label::new(self.metadata.name()).wrap_mode(TextWrapMode::Wrap));
                        ui.end_row();

                        ui.label(RichText::new("Country:").strong());
                        ui.add(Label::new(self.metadata.country()).wrap_mode(TextWrapMode::Wrap));
                        ui.end_row();

                        if let Some(p) = self.metadata.producer() {
                            ui.label(RichText::new("Producer:").strong());
                            ui.add(Label::new(p).wrap_mode(TextWrapMode::Wrap));
                            ui.end_row();
                        }

                        if let Some(lic) = self.metadata.licence_name() {
                            ui.label(RichText::new("License:").strong());
                            if let Some(url) = self.metadata.licence_url() {
                                // Show license name, and a clickable URL below it
                                ui.vertical(|ui| {
                                    ui.add(Label::new(lic).wrap_mode(TextWrapMode::Wrap));
                                    let link_text = url;
                                    let response = ui.add(
                                        Label::new(
                                            RichText::new(link_text)
                                                .underline()
                                                .color(ui.visuals().hyperlink_color),
                                        )
                                        .wrap_mode(TextWrapMode::Wrap)
                                        .sense(egui::Sense::click()),
                                    );
                                    if response.clicked() {
                                        let _ = open::that(url);
                                    }
                                });
                            } else {
                                ui.add(Label::new(lic).wrap_mode(TextWrapMode::Wrap));
                            }
                            ui.end_row();
                        }

                        ui.label(RichText::new("Source URL:").strong());
                        {
                            let url = self.metadata.source_url();
                            let response = ui.add(
                                Label::new(
                                    RichText::new(url)
                                        .underline()
                                        .color(ui.visuals().hyperlink_color),
                                )
                                .wrap_mode(TextWrapMode::Wrap)
                                .sense(egui::Sense::click()),
                            );
                            if response.clicked() {
                                let _ = open::that(url);
                            }
                        }
                        ui.end_row();

                        ui.label(RichText::new("API URL:").strong());
                        {
                            let url = &self.metadata.url;
                            let response = ui.add(
                                Label::new(
                                    RichText::new(url)
                                        .underline()
                                        .color(ui.visuals().hyperlink_color),
                                )
                                .wrap_mode(TextWrapMode::Wrap)
                                .sense(egui::Sense::click()),
                            );
                            if response.clicked() {
                                let _ = open::that(url);
                            }
                        }
                        ui.end_row();

                        if let Some(locale) = self.metadata.locale_str() {
                            ui.label(RichText::new("Locale:").strong());
                            ui.add(Label::new(locale).wrap_mode(TextWrapMode::Wrap));
                            ui.end_row();
                        }

                        if let Some(ts) = self.metadata.tile_server() {
                            ui.label(RichText::new("Tile server:").strong());
                            ui.add(Label::new(ts).wrap_mode(TextWrapMode::Wrap));
                            ui.end_row();
                        }

                        if let Some(pattern) = &self.metadata.editor_pattern {
                            ui.label(RichText::new("Editor pattern:").strong());
                            ui.add(Label::new(pattern).wrap_mode(TextWrapMode::Wrap));
                            ui.end_row();
                        }

                        ui.label(RichText::new("Center:").strong());
                        ui.add(
                            Label::new(format!(
                                "lat: {:.5}, lon: {:.5}",
                                self.metadata.center.lat, self.metadata.center.lon
                            ))
                            .wrap_mode(TextWrapMode::Wrap),
                        );
                        ui.end_row();
                    });
            });
    }
}
