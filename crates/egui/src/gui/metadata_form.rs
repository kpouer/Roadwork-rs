use crate::gui::center_picker_dialog::CenterPickerDialog;
use crate::gui::service_helper_dialog::DataType;
use crate::gui::service_helper_form::{LABEL_WIDTH, center_row, optional_text_row, text_row};
use eframe::epaint::Color32;
use egui::{RichText, Ui};
use roadwork_core::opendata::json::model::metadata::Metadata;

pub struct MetadataForm<'a> {
    metadata: &'a mut Metadata,
    data_type: DataType,
}

impl<'a> MetadataForm<'a> {
    pub const fn new(metadata: &'a mut Metadata, data_type: DataType) -> Self {
        Self {
            metadata,
            data_type,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        center_picker: &mut CenterPickerDialog,
        center_picker_open: &mut bool,
    ) -> bool {
        let mut changed = false;
        ui.heading("Metadata");
        changed |= text_row(ui, "Name", &mut self.metadata.name, None, None);
        changed |= center_row(
            ui,
            &mut self.metadata.center,
            center_picker,
            center_picker_open,
        );

        if matches!(self.data_type, DataType::Roadwork) {
            let mut url_text = self.metadata.url.clone().unwrap_or_default();
            let url_changed = text_row(ui, "URL", &mut url_text, None, None);
            if url_changed {
                self.metadata.url = if url_text.is_empty() {
                    None
                } else {
                    Some(url_text)
                };
                changed = true;
            }

            changed |= ui
                .collapsing("Optional parameters", |ui| {
                    let mut changed = false;
                    changed |=
                        optional_text_row(ui, "Country", &mut self.metadata.country, None, None);
                    changed |=
                        optional_text_row(ui, "Producer", &mut self.metadata.producer, None, None);
                    changed |= optional_text_row(
                        ui,
                        "Licence name",
                        &mut self.metadata.licence_name,
                        None,
                        None,
                    );
                    changed |= optional_text_row(
                        ui,
                        "Licence URL",
                        &mut self.metadata.licence_url,
                        None,
                        None,
                    );
                    changed |= optional_text_row(
                        ui,
                        "Source URL",
                        &mut self.metadata.source_url,
                        None,
                        None,
                    );
                    changed |=
                        optional_text_row(ui, "Locale", &mut self.metadata.locale, None, None);
                    changed |= color_row(ui, &mut self.metadata.color);
                    changed
                })
                .body_returned
                .unwrap_or(false);
        }

        changed
    }
}

fn color_row(ui: &mut Ui, color: &mut Option<String>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized(
            [LABEL_WIDTH, 20.0],
            egui::Label::new(RichText::new("Color").strong()),
        );
        let mut current = color
            .as_deref()
            .and_then(hex_to_color32)
            .unwrap_or(DEFAULT_COLOR);
        let response = ui
            .color_edit_button_srgba(&mut current)
            .on_hover_text("Color used to display this source in WME");
        if response.changed() {
            *color = Some(color32_to_hex(current));
            changed = true;
        }
        if let Some(hex) = color.as_deref() {
            ui.label(format!("({hex})"));
        }
    });
    changed
}

const DEFAULT_COLOR: Color32 = Color32::from_rgb(0x0d, 0x94, 0x88);

fn hex_to_color32(hex: &str) -> Option<Color32> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(Color32::from_rgb(r, g, b))
}

fn color32_to_hex(color: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", color.r(), color.g(), color.b())
}
