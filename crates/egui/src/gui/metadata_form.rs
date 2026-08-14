use crate::gui::center_picker_dialog::CenterPickerDialog;
use crate::gui::service_helper_form::{center_row, optional_text_row, text_row};
use egui::Ui;
use roadwork_core::opendata::json::model::metadata::Metadata;

pub struct MetadataForm<'a> {
    metadata: &'a mut Metadata,
}

impl<'a> MetadataForm<'a> {
    pub fn new(metadata: &'a mut Metadata) -> Self {
        Self { metadata }
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
        changed |= optional_text_row(ui, "Country", &mut self.metadata.country, None, None);
        changed |= optional_text_row(ui, "Producer", &mut self.metadata.producer, None, None);
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
        changed |= optional_text_row(ui, "Source URL", &mut self.metadata.source_url, None, None);
        changed |= optional_text_row(ui, "URL", &mut self.metadata.url, None, None);
        changed |= optional_text_row(ui, "Locale", &mut self.metadata.locale, None, None);
        changed |=
            crate::gui::opendata_service_helper_form::color_row(ui, &mut self.metadata.color);

        changed
    }
}
