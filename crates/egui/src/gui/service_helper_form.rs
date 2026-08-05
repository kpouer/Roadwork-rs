use egui::{DragValue, Grid, RichText, TextEdit, Ui};
use roadwork_core::opendata::json::model::date_parser::DateParser;
use roadwork_core::opendata::json::model::lat_lng::LatLng;
use roadwork_core::opendata::json::model::parser::Parser;
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;

use super::center_picker_dialog::CenterPickerDialog;

const LABEL_WIDTH: f32 = 150.0;

pub(crate) fn show(
    ui: &mut Ui,
    descriptor: &mut ServiceDescriptor,
    url_params: &mut Vec<(String, String)>,
    center_picker: &mut CenterPickerDialog,
    center_picker_open: &mut bool,
) -> bool {
    let mut changed = false;

    ui.heading("Metadata");
    changed |= text_row(ui, "Country", &mut descriptor.metadata.country);
    changed |= text_row(ui, "Name", &mut descriptor.metadata.name);
    changed |= optional_text_row(ui, "Producer", &mut descriptor.metadata.producer);
    changed |= optional_text_row(ui, "Licence name", &mut descriptor.metadata.licence_name);
    changed |= optional_text_row(ui, "Licence URL", &mut descriptor.metadata.licence_url);
    changed |= text_row(ui, "Source URL", &mut descriptor.metadata.source_url);
    changed |= text_row(ui, "URL", &mut descriptor.metadata.url);
    changed |= optional_text_row(ui, "Locale", &mut descriptor.metadata.locale);
    changed |= optional_text_row(ui, "Tile server", &mut descriptor.metadata.tile_server);
    changed |= optional_text_row(
        ui,
        "Editor pattern",
        &mut descriptor.metadata.editor_pattern,
    );
    changed |= center_row(
        ui,
        &mut descriptor.metadata.center,
        center_picker,
        center_picker_open,
    );

    ui.add_space(8.0);
    ui.heading("Fields");
    changed |= text_row(ui, "roadworkArray", &mut descriptor.roadwork_array);
    changed |= text_row(ui, "id", &mut descriptor.id);
    changed |= optional_text_row(ui, "latitude", &mut descriptor.latitude);
    changed |= optional_text_row(ui, "longitude", &mut descriptor.longitude);
    changed |= optional_text_row(ui, "polygon", &mut descriptor.polygon);
    changed |= optional_text_row(ui, "road", &mut descriptor.road);
    changed |= optional_text_row(ui, "description", &mut descriptor.description);
    changed |= optional_text_row(ui, "locationDetails", &mut descriptor.location_details);
    changed |= optional_text_row(
        ui,
        "impactCirculationDetail",
        &mut descriptor.impact_circulation_detail,
    );
    changed |= optional_text_row(ui, "url", &mut descriptor.url);

    ui.add_space(8.0);
    ui.heading("Dates");
    changed |= date_section(ui, "from", &mut descriptor.from);
    changed |= date_section(ui, "to", &mut descriptor.to);

    ui.add_space(8.0);
    ui.heading("URL params");
    changed |= url_params_grid(ui, url_params);

    changed
}

fn text_row(ui: &mut Ui, label: &str, value: &mut String) -> bool {
    ui.horizontal(|ui| {
        ui.add_sized(
            [LABEL_WIDTH, 20.0],
            egui::Label::new(RichText::new(label).strong()),
        );
        let width = ui.available_width();
        ui.add(TextEdit::singleline(value).desired_width(width))
            .changed()
    })
    .inner
}

fn optional_text_row(ui: &mut Ui, label: &str, value: &mut Option<String>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized(
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
        let response = ui.add_enabled(
            present,
            TextEdit::singleline(&mut text).desired_width(width),
        );
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

fn date_section(ui: &mut Ui, title: &str, date: &mut Option<DateParser>) -> bool {
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
            ui.horizontal(|ui| {
                ui.add_sized(
                    [LABEL_WIDTH, 20.0],
                    egui::Label::new(RichText::new("path").strong()),
                );
                let width = ui.available_width();
                changed |= ui
                    .add(TextEdit::singleline(&mut date.path).desired_width(width))
                    .changed();
            });
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
