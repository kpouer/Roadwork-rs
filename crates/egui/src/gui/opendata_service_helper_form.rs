use egui::Ui;
use roadwork_core::opendata::json::model::opendata_service_descriptor::OpendataServiceDescriptor;

use super::center_picker_dialog::CenterPickerDialog;
use super::service_helper_form::{
    Wand, center_row, optional_text_row, roadwork_array_row, text_row, url_params_grid, validated,
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

#[derive(Default)]
pub(crate) struct PathCandidates {
    pub scalars: Vec<(String, String)>,
    pub arrays: Vec<(String, usize)>,
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
    changed |= text_row(
        ui,
        "Source URL",
        &mut descriptor.metadata.source_url,
        None,
        None,
    );
    changed |= text_row(ui, "URL", &mut descriptor.metadata.url, None, None);
    changed |= optional_text_row(ui, "Locale", &mut descriptor.metadata.locale, None, None);
    changed |= optional_text_row(
        ui,
        "Tile server",
        &mut descriptor.metadata.tile_server,
        None,
        None,
    );
    changed |= center_row(
        ui,
        &mut descriptor.metadata.center,
        center_picker,
        center_picker_open,
    );

    ui.add_space(8.0);
    ui.heading("URL params");
    changed |= url_params_grid(ui, url_params);

    changed
}
