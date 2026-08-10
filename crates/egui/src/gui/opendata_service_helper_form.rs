use egui::{Color32, RichText, Ui};
use roadwork_core::opendata::json::model::opendata_service_descriptor::OpendataServiceDescriptor;

use super::center_picker_dialog::CenterPickerDialog;
pub use super::service_helper_form::PathCandidates;
use super::service_helper_form::{
    LABEL_WIDTH, Wand, center_row, optional_text_row, roadwork_array_row, text_row,
    url_params_grid, validated,
};

#[derive(Clone, Copy)]
pub(crate) struct FieldsValidation {
    pub data_array: bool,
    pub id: bool,
    pub latitude: bool,
    pub longitude: bool,
    pub polygon: bool,
    pub description: bool,
}

#[derive(Default, Clone)]
pub(crate) struct FieldsValues {
    pub data_array: Option<String>,
    pub id: Option<String>,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub polygon: Option<String>,
    pub description: Option<String>,
}

impl FieldsValidation {
    pub fn valid() -> Self {
        Self {
            data_array: true,
            id: true,
            latitude: true,
            longitude: true,
            polygon: true,
            description: true,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn show(
    ui: &mut Ui,
    descriptor: &mut OpendataServiceDescriptor,
    url_params: &mut Vec<(String, String)>,
    center_picker: &mut CenterPickerDialog,
    center_picker_open: &mut bool,
    validation: &FieldsValidation,
    values: &FieldsValues,
    array_paths: &[(String, usize)],
    path_candidates: &PathCandidates,
) -> bool {
    let mut changed = false;
    let scalar_wand = Wand {
        scalars: &path_candidates.scalars,
        arrays: None,
        hint: "Fetch the JSON first and make sure dataArray points to the array of opendata.",
    };
    let polygon_wand = Wand {
        scalars: &path_candidates.scalars,
        arrays: Some(&path_candidates.arrays),
        hint: scalar_wand.hint,
    };

    changed |= show_metadata_section(
        ui,
        descriptor,
        url_params,
        center_picker,
        center_picker_open,
    );

    ui.add_space(8.0);
    ui.heading("Fields");
    changed |= roadwork_array_row(
        ui,
        &mut descriptor.data_array,
        validation.data_array,
        values.data_array.as_deref(),
        array_paths,
    );
    changed |= validated(
        ui,
        validation.id,
        "id must point to a scalar value in the fetched JSON",
        |ui, tooltip| text_row(ui, "id", &mut descriptor.id, tooltip, Some(&scalar_wand)),
        values.id.as_deref(),
    );
    changed |= validated(
        ui,
        validation.latitude,
        "latitude must point to a scalar value in the fetched JSON",
        |ui, tooltip| {
            optional_text_row(
                ui,
                "latitude",
                &mut descriptor.latitude,
                tooltip,
                Some(&scalar_wand),
            )
        },
        values.latitude.as_deref(),
    );
    changed |= validated(
        ui,
        validation.longitude,
        "longitude must point to a scalar value in the fetched JSON",
        |ui, tooltip| {
            optional_text_row(
                ui,
                "longitude",
                &mut descriptor.longitude,
                tooltip,
                Some(&scalar_wand),
            )
        },
        values.longitude.as_deref(),
    );
    changed |= validated(
        ui,
        validation.polygon,
        "polygon must point to a value in the fetched JSON",
        |ui, tooltip| {
            optional_text_row(
                ui,
                "polygon",
                &mut descriptor.polygon,
                tooltip,
                Some(&polygon_wand),
            )
        },
        values.polygon.as_deref(),
    );
    changed |= validated(
        ui,
        validation.description,
        "description must point to a scalar value in the fetched JSON",
        |ui, tooltip| {
            optional_text_row(
                ui,
                "description",
                &mut descriptor.description,
                tooltip,
                Some(&scalar_wand),
            )
        },
        values.description.as_deref(),
    );

    changed
}

pub(crate) fn show_metadata_section(
    ui: &mut Ui,
    descriptor: &mut OpendataServiceDescriptor,
    url_params: &mut Vec<(String, String)>,
    center_picker: &mut CenterPickerDialog,
    center_picker_open: &mut bool,
) -> bool {
    let mut changed = false;
    ui.heading("Metadata");
    changed |= text_row(ui, "Country", &mut descriptor.metadata.country, None, None);
    changed |= text_row(ui, "Name", &mut descriptor.metadata.name, None, None);
    changed |= optional_text_row(
        ui,
        "Producer",
        &mut descriptor.metadata.producer,
        None,
        None,
    );
    changed |= optional_text_row(
        ui,
        "Licence name",
        &mut descriptor.metadata.licence_name,
        None,
        None,
    );
    changed |= optional_text_row(
        ui,
        "Licence URL",
        &mut descriptor.metadata.licence_url,
        None,
        None,
    );
    changed |= optional_text_row(
        ui,
        "Source URL",
        &mut descriptor.metadata.source_url,
        None,
        None,
    );
    changed |= optional_text_row(ui, "URL", &mut descriptor.metadata.url, None, None);
    changed |= optional_text_row(ui, "Locale", &mut descriptor.metadata.locale, None, None);
    changed |= center_row(
        ui,
        &mut descriptor.metadata.center,
        center_picker,
        center_picker_open,
    );
    changed |= color_row(ui, &mut descriptor.metadata.color);

    ui.add_space(8.0);
    ui.heading("URL params");
    changed |= url_params_grid(ui, url_params);

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
