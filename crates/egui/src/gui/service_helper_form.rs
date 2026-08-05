use egui::{DragValue, Grid, RichText, TextEdit, Ui};
use roadwork_core::opendata::json::model::date_parser::DateParser;
use roadwork_core::opendata::json::model::lat_lng::LatLng;
use roadwork_core::opendata::json::model::parser::Parser;
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;

use super::center_picker_dialog::CenterPickerDialog;

const LABEL_WIDTH: f32 = 150.0;

#[derive(Clone, Copy)]
pub(crate) struct FieldsValidation {
    pub roadwork_array: bool,
    pub id: bool,
    pub latitude: bool,
    pub longitude: bool,
    pub polygon: bool,
    pub road: bool,
    pub description: bool,
    pub location_details: bool,
    pub impact_circulation_detail: bool,
    pub from_path: bool,
    pub to_path: bool,
}

#[derive(Default, Clone)]
pub(crate) struct FieldsValues {
    pub roadwork_array: Option<String>,
    pub id: Option<String>,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub polygon: Option<String>,
    pub road: Option<String>,
    pub description: Option<String>,
    pub location_details: Option<String>,
    pub impact_circulation_detail: Option<String>,
    pub url: Option<String>,
    pub from_path: Option<String>,
    pub to_path: Option<String>,
}

impl FieldsValidation {
    pub fn valid() -> Self {
        Self {
            roadwork_array: true,
            id: true,
            latitude: true,
            longitude: true,
            polygon: true,
            road: true,
            description: true,
            location_details: true,
            impact_circulation_detail: true,
            from_path: true,
            to_path: true,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn show(
    ui: &mut Ui,
    descriptor: &mut ServiceDescriptor,
    url_params: &mut Vec<(String, String)>,
    center_picker: &mut CenterPickerDialog,
    center_picker_open: &mut bool,
    validation: &FieldsValidation,
    values: &FieldsValues,
    array_paths: &[(String, usize)],
) -> bool {
    let mut changed = false;

    ui.heading("Metadata");
    changed |= text_row(ui, "Country", &mut descriptor.metadata.country, None);
    changed |= text_row(ui, "Name", &mut descriptor.metadata.name, None);
    changed |= optional_text_row(ui, "Producer", &mut descriptor.metadata.producer, None);
    changed |= optional_text_row(
        ui,
        "Licence name",
        &mut descriptor.metadata.licence_name,
        None,
    );
    changed |= optional_text_row(
        ui,
        "Licence URL",
        &mut descriptor.metadata.licence_url,
        None,
    );
    changed |= text_row(ui, "Source URL", &mut descriptor.metadata.source_url, None);
    changed |= text_row(ui, "URL", &mut descriptor.metadata.url, None);
    changed |= optional_text_row(ui, "Locale", &mut descriptor.metadata.locale, None);
    changed |= optional_text_row(
        ui,
        "Tile server",
        &mut descriptor.metadata.tile_server,
        None,
    );
    changed |= optional_text_row(
        ui,
        "Editor pattern",
        &mut descriptor.metadata.editor_pattern,
        None,
    );
    changed |= center_row(
        ui,
        &mut descriptor.metadata.center,
        center_picker,
        center_picker_open,
    );

    ui.add_space(8.0);
    ui.heading("Fields");
    changed |= roadwork_array_row(
        ui,
        &mut descriptor.roadwork_array,
        validation.roadwork_array,
        values.roadwork_array.as_deref(),
        array_paths,
    );
    changed |= validated(
        ui,
        validation.id,
        "id must point to a scalar value in the fetched JSON",
        |ui, tooltip| text_row(ui, "id", &mut descriptor.id, tooltip),
        values.id.as_deref(),
    );
    changed |= validated(
        ui,
        validation.latitude,
        "latitude must point to a scalar value in the fetched JSON",
        |ui, tooltip| optional_text_row(ui, "latitude", &mut descriptor.latitude, tooltip),
        values.latitude.as_deref(),
    );
    changed |= validated(
        ui,
        validation.longitude,
        "longitude must point to a scalar value in the fetched JSON",
        |ui, tooltip| optional_text_row(ui, "longitude", &mut descriptor.longitude, tooltip),
        values.longitude.as_deref(),
    );
    changed |= validated(
        ui,
        validation.polygon,
        "polygon must point to a value in the fetched JSON",
        |ui, tooltip| optional_text_row(ui, "polygon", &mut descriptor.polygon, tooltip),
        values.polygon.as_deref(),
    );
    changed |= validated(
        ui,
        validation.road,
        "road must point to a scalar value in the fetched JSON",
        |ui, tooltip| optional_text_row(ui, "road", &mut descriptor.road, tooltip),
        values.road.as_deref(),
    );
    changed |= validated(
        ui,
        validation.description,
        "description must point to a scalar value in the fetched JSON",
        |ui, tooltip| optional_text_row(ui, "description", &mut descriptor.description, tooltip),
        values.description.as_deref(),
    );
    changed |= validated(
        ui,
        validation.location_details,
        "locationDetails must point to a scalar value in the fetched JSON",
        |ui, tooltip| {
            optional_text_row(
                ui,
                "locationDetails",
                &mut descriptor.location_details,
                tooltip,
            )
        },
        values.location_details.as_deref(),
    );
    changed |= validated(
        ui,
        validation.impact_circulation_detail,
        "impactCirculationDetail must point to a scalar value in the fetched JSON",
        |ui, tooltip| {
            optional_text_row(
                ui,
                "impactCirculationDetail",
                &mut descriptor.impact_circulation_detail,
                tooltip,
            )
        },
        values.impact_circulation_detail.as_deref(),
    );
    changed |= optional_text_row(ui, "url", &mut descriptor.url, values.url.as_deref());

    ui.add_space(8.0);
    ui.heading("Dates");
    changed |= date_section(
        ui,
        "from",
        &mut descriptor.from,
        validation.from_path,
        values.from_path.as_deref(),
    );
    changed |= date_section(
        ui,
        "to",
        &mut descriptor.to,
        validation.to_path,
        values.to_path.as_deref(),
    );

    ui.add_space(8.0);
    ui.heading("URL params");
    changed |= url_params_grid(ui, url_params);

    changed
}

fn roadwork_array_row(
    ui: &mut Ui,
    value: &mut String,
    valid: bool,
    tooltip: Option<&str>,
    array_paths: &[(String, usize)],
) -> bool {
    validated(
        ui,
        valid,
        "roadworkArray must point to an array in the fetched JSON",
        |ui, tooltip| {
            let mut changed = false;
            ui.horizontal(|ui| {
                let label = ui.add_sized(
                    [LABEL_WIDTH, 20.0],
                    egui::Label::new(RichText::new("roadworkArray").strong()),
                );
                let width = ui.available_width() - 30.0;
                let mut response = ui.add(TextEdit::singleline(value).desired_width(width));
                if let Some(tooltip) = tooltip {
                    label.on_hover_text(tooltip);
                    response = response.on_hover_text(tooltip);
                }
                changed |= response.changed();
                let wand = ui
                    .add(egui::Button::new("✨"))
                    .on_hover_text("Magic wand: pick an array path from the fetched JSON");
                let _ = egui::Popup::menu(&wand).show(|ui| {
                    if array_paths.is_empty() {
                        ui.label("No array found in the fetched JSON. Fetch the JSON first.");
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("wand_arrays_scroll")
                            .max_height(300.0)
                            .show(ui, |ui| {
                                for (path, count) in array_paths {
                                    let selector = format!("{path}[*]");
                                    if ui.button(format!("{selector} ({count} items)")).clicked() {
                                        *value = selector;
                                        changed = true;
                                    }
                                }
                            });
                    }
                });
            });
            changed
        },
        tooltip,
    )
}

fn validated<F>(
    ui: &mut Ui,
    valid: bool,
    error_tooltip: &str,
    inner: F,
    value_tooltip: Option<&str>,
) -> bool
where
    F: FnOnce(&mut Ui, Option<&str>) -> bool,
{
    if valid {
        return inner(ui, value_tooltip);
    }
    let mut changed = false;
    let response = egui::Frame::default()
        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::RED))
        .inner_margin(4.0)
        .show(ui, |ui| {
            changed = inner(ui, None);
        })
        .response;
    response.on_hover_text(error_tooltip);
    changed
}

fn text_row(ui: &mut Ui, label: &str, value: &mut String, tooltip: Option<&str>) -> bool {
    ui.horizontal(|ui| {
        let label = ui.add_sized(
            [LABEL_WIDTH, 20.0],
            egui::Label::new(RichText::new(label).strong()),
        );
        let width = ui.available_width();
        let mut response = ui.add(TextEdit::singleline(value).desired_width(width));
        if let Some(tooltip) = tooltip {
            label.on_hover_text(tooltip);
            response = response.on_hover_text(tooltip);
        }
        response.changed()
    })
    .inner
}

fn optional_text_row(
    ui: &mut Ui,
    label: &str,
    value: &mut Option<String>,
    tooltip: Option<&str>,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let label = ui.add_sized(
            [LABEL_WIDTH, 20.0],
            egui::Label::new(RichText::new(label).strong()),
        );
        let mut present = value.is_some();
        if ui.checkbox(&mut present, "").changed() {
            *value = if present { Some(String::new()) } else { None };
            changed = true;
        }
        let mut text = value.clone().unwrap_or_default();
        let width = ui.available_width();
        let mut response = ui.add_enabled(
            present,
            TextEdit::singleline(&mut text).desired_width(width),
        );
        if let Some(tooltip) = tooltip {
            label.on_hover_text(tooltip);
            response = response.on_hover_text(tooltip);
        }
        if response.changed() {
            *value = Some(text);
            changed = true;
        }
    });
    changed
}

fn center_row(
    ui: &mut Ui,
    center: &mut LatLng,
    center_picker: &mut CenterPickerDialog,
    center_picker_open: &mut bool,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized(
            [LABEL_WIDTH, 20.0],
            egui::Label::new(RichText::new("Center").strong()),
        );
        ui.label("lat");
        changed |= ui
            .add(DragValue::new(&mut center.lat).speed(0.0001))
            .changed();
        ui.label("lon");
        changed |= ui
            .add(DragValue::new(&mut center.lon).speed(0.0001))
            .changed();
        if ui.button("Pick on map").clicked() {
            center_picker.open(*center);
            *center_picker_open = true;
        }
    });
    changed
}

fn date_section(
    ui: &mut Ui,
    title: &str,
    date: &mut Option<DateParser>,
    path_valid: bool,
    path_tooltip: Option<&str>,
) -> bool {
    let mut changed = false;
    let mut present = date.is_some();
    ui.horizontal(|ui| {
        ui.add_sized(
            [LABEL_WIDTH, 20.0],
            egui::Label::new(RichText::new(title).strong()),
        );
        if ui.checkbox(&mut present, "enabled").changed() {
            *date = if present {
                Some(DateParser::default())
            } else {
                None
            };
            changed = true;
        }
    });
    if let Some(date) = date {
        ui.push_id(title, |ui| {
            changed |= validated(
                ui,
                path_valid,
                &format!("{title} path must point to a scalar value in the fetched JSON"),
                |ui, tooltip| {
                    ui.horizontal(|ui| {
                        let label = ui.add_sized(
                            [LABEL_WIDTH, 20.0],
                            egui::Label::new(RichText::new("path").strong()),
                        );
                        let width = ui.available_width();
                        let mut response =
                            ui.add(TextEdit::singleline(&mut date.path).desired_width(width));
                        if let Some(tooltip) = tooltip {
                            label.on_hover_text(tooltip);
                            response = response.on_hover_text(tooltip);
                        }
                        response.changed()
                    })
                    .inner
                },
                path_tooltip,
            );
            changed |= parsers_grid(ui, &mut date.parsers);
        });
    }
    changed
}

fn parsers_grid(ui: &mut Ui, parsers: &mut Vec<Parser>) -> bool {
    let mut changed = false;
    let mut remove: Option<usize> = None;
    let col_width = ((ui.available_width() - 300.0) / 2.0).max(80.0);
    Grid::new("parsers_grid")
        .num_columns(6)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for (i, parser) in parsers.iter_mut().enumerate() {
                changed |= ui
                    .add(TextEdit::singleline(&mut parser.matcher).desired_width(col_width))
                    .changed();
                let mut present = parser.format.is_some();
                if ui.checkbox(&mut present, "").changed() {
                    parser.format = if present { Some(String::new()) } else { None };
                    changed = true;
                }
                let mut format_text = parser.format.clone().unwrap_or_default();
                let response = ui.add_enabled(
                    present,
                    TextEdit::singleline(&mut format_text).desired_width(col_width),
                );
                if response.changed() {
                    parser.format = if format_text.is_empty() {
                        None
                    } else {
                        Some(format_text)
                    };
                    changed = true;
                }
                changed |= ui.checkbox(&mut parser.add_year, "addYear").changed();
                changed |= ui.checkbox(&mut parser.reset_hour, "resetHour").changed();
                if ui.button("Remove").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove {
        parsers.remove(i);
        changed = true;
    }
    if ui.button("+ Add parser").clicked() {
        parsers.push(Parser::default());
        changed = true;
    }
    changed
}

fn url_params_grid(ui: &mut Ui, params: &mut Vec<(String, String)>) -> bool {
    let mut changed = false;
    let mut remove: Option<usize> = None;
    let col_width = ((ui.available_width() - 130.0) / 2.0).max(100.0);
    Grid::new("url_params_grid")
        .num_columns(3)
        .spacing([8.0, 4.0])
        .show(ui, |ui| {
            for (i, (key, value)) in params.iter_mut().enumerate() {
                changed |= ui
                    .add(TextEdit::singleline(key).desired_width(col_width))
                    .changed();
                changed |= ui
                    .add(TextEdit::singleline(value).desired_width(col_width))
                    .changed();
                if ui.button("Remove").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
    if let Some(i) = remove {
        params.remove(i);
        changed = true;
    }
    if ui.button("+ Add param").clicked() {
        params.push((String::new(), String::new()));
        changed = true;
    }
    changed
}
