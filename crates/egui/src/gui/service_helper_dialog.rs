use egui::{Context, RichText, Ui};
use egui_notify::Toasts;
use roadwork_core::opendata::json::model::date_parser::DateParser;
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;
use roadwork_core::opendata::json::opendata_service::OpendataService;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::center_picker_dialog::CenterPickerDialog;
use super::service_helper_form::{FieldsValidation, FieldsValues, PathCandidates};

#[derive(Clone)]
enum FetchState {
    Connecting,
    Downloading,
    Done(Result<String, String>),
}

#[derive(Default)]
pub(crate) struct ServiceHelperDialog {
    service: String,
    descriptor: Option<ServiceDescriptor>,
    descriptor_json: String,
    form_mode: bool,
    url_params: Vec<(String, String)>,
    url: String,
    raw_json: String,
    result_json: String,
    error: Option<String>,
    dirty: bool,
    fetch_state: Arc<Mutex<Option<FetchState>>>,
    center_picker: CenterPickerDialog,
    center_picker_open: bool,
    was_open: bool,
    array_paths: Vec<(String, usize)>,
}

impl ServiceHelperDialog {
    pub(crate) fn new(service: &str) -> Self {
        let mut dialog = Self {
            service: service.to_string(),
            form_mode: true,
            ..Default::default()
        };
        dialog.reload();
        dialog
    }

    pub(crate) fn show(
        &mut self,
        ctx: &Context,
        open: &mut bool,
        service: &str,
        toasts: &mut Toasts,
    ) {
        self.service = service.to_string();
        let is_open = *open;
        if is_open && !self.was_open {
            self.reload();
            if !self.url.is_empty() {
                self.fetch(ctx.clone());
            }
        }
        self.was_open = is_open;
        let result = match &*self.fetch_state.lock().unwrap() {
            Some(FetchState::Done(result)) => Some(result.clone()),
            _ => None,
        };
        if let Some(result) = result {
            *self.fetch_state.lock().unwrap() = None;
            match result {
                Ok(text) => {
                    self.raw_json = text;
                    self.array_paths =
                        roadwork_core::opendata::json::opendata_service::find_json_arrays(
                            &self.raw_json,
                        );
                    self.error = None;
                    self.dirty = true;
                    toasts.success("Fetch succeeded");
                }
                Err(e) => {
                    self.error = Some(e.clone());
                    self.result_json = format!("Fetch error: {e}");
                    toasts.error(format!("Fetch error: {e}"));
                }
            }
        }
        self.recompute_if_dirty();

        let screen = ctx.content_rect().size();
        let default_size = egui::vec2(
            (screen.x * 0.75).clamp(360.0, 900.0),
            (screen.y * 0.8).clamp(320.0, 650.0),
        );
        let max = egui::vec2(screen.x * 0.96, screen.y * 0.96);
        egui::Window::new("Service helper")
            .open(open)
            .resizable(true)
            .default_size(default_size)
            .max_size(max)
            .show(ctx, |ui| {
                self.show_content(ui);
            });
    }

    fn show_content(&mut self, ui: &mut Ui) {
        let (fetching, step) = match &*self.fetch_state.lock().unwrap() {
            Some(FetchState::Connecting) => (true, "Connecting…"),
            Some(FetchState::Downloading) => (true, "Getting data…"),
            _ => (false, ""),
        };
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.form_mode, "Form mode");
            ui.separator();
            if ui
                .add_enabled(!fetching, egui::Button::new("Fetch"))
                .clicked()
            {
                self.fetch(ui.ctx().clone());
            }
            if ui.button("Reload").clicked() {
                self.reload();
            }
        });
        ui.separator();
        let available_height = ui.available_height();
        let theme = egui_extras::syntax_highlighting::CodeTheme::from_style(ui.style());
        let mut json_layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
            let mut job = egui_extras::syntax_highlighting::highlight(
                ui.ctx(),
                ui.style(),
                &theme,
                text.as_str(),
                "json",
            );
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let screen = ui.ctx().content_rect().size();
        egui::Panel::left("helper_left")
            .resizable(true)
            .default_size(380.0)
            .size_range(240.0..=(screen.x * 0.45))
            .show_inside(ui, |ui| {
                if self.form_mode {
                    ui.label(RichText::new("Service descriptor (form)").strong());
                    self.show_form(ui, available_height);
                } else {
                    ui.label(RichText::new("Service descriptor (JSON)").strong());
                    egui::ScrollArea::vertical()
                        .id_salt("descriptor_scroll")
                        .max_height(available_height - 24.0)
                        .show(ui, |ui| {
                            let response = ui.add(
                                egui::TextEdit::multiline(&mut self.descriptor_json)
                                    .code_editor()
                                    .interactive(true)
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(30)
                                    .layouter(&mut json_layouter),
                            );
                            if response.changed() {
                                self.apply_descriptor_json();
                            }
                        });
                }
            });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.label(RichText::new("URL").strong());
            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.url)
                        .hint_text("https://...")
                        .desired_width(f32::INFINITY),
                );
                if response.changed() {
                    self.propagate_url();
                }
                if ui
                    .add_enabled(!fetching, egui::Button::new("Fetch"))
                    .clicked()
                {
                    self.fetch(ui.ctx().clone());
                }
                if fetching {
                    ui.spinner();
                    ui.label(step);
                }
            });
            if let Some(error) = &self.error {
                ui.colored_label(ui.visuals().error_fg_color, error);
            }

            ui.label(RichText::new("Fetched JSON").strong());
            egui::ScrollArea::vertical()
                .id_salt("raw_json_scroll")
                .max_height((available_height - 100.0) * 0.55)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.raw_json)
                            .code_editor()
                            .interactive(false)
                            .desired_width(f32::INFINITY)
                            .desired_rows(15)
                            .layouter(&mut json_layouter),
                    );
                });

            ui.label(RichText::new("Result JSON").strong());
            egui::ScrollArea::vertical()
                .id_salt("result_json_scroll")
                .max_height((available_height - 100.0) * 0.4)
                .show(ui, |ui| {
                    ui.add(
                        egui::TextEdit::multiline(&mut self.result_json)
                            .code_editor()
                            .interactive(false)
                            .desired_width(f32::INFINITY)
                            .desired_rows(15)
                            .layouter(&mut json_layouter),
                    );
                });
        });
    }

    fn show_form(&mut self, ui: &mut Ui, available_height: f32) {
        let field_validation = self.field_validation();
        let field_values = self.field_values();
        let path_candidates = self.path_candidates();
        let Self {
            descriptor,
            descriptor_json,
            dirty,
            url_params,
            center_picker,
            center_picker_open,
            url,
            ..
        } = self;
        egui::ScrollArea::vertical()
            .id_salt("descriptor_form_scroll")
            .max_height(available_height - 24.0)
            .show(ui, |ui| match descriptor {
                Some(descriptor) => {
                    let changed = crate::gui::service_helper_form::show(
                        ui,
                        descriptor,
                        url_params,
                        center_picker,
                        center_picker_open,
                        &field_validation,
                        &field_values,
                        &self.array_paths,
                        &path_candidates,
                    );
                    if changed {
                        let params: HashMap<String, String> = url_params
                            .iter()
                            .filter(|(key, _)| !key.trim().is_empty())
                            .map(|(key, value)| (key.trim().to_string(), value.clone()))
                            .collect();
                        descriptor.metadata.url_params = if params.is_empty() {
                            None
                        } else {
                            Some(params)
                        };
                        *dirty = true;
                        *descriptor_json =
                            serde_json::to_string_pretty(descriptor).unwrap_or_default();
                        *url = descriptor.metadata.url.clone();
                    }
                }
                None => {
                    ui.colored_label(ui.visuals().error_fg_color, "No descriptor available");
                }
            });
        if *center_picker_open
            && let Some(center) = center_picker.show(ui.ctx(), center_picker_open)
            && let Some(descriptor) = descriptor
        {
            descriptor.metadata.center = center;
            *dirty = true;
            *descriptor_json = serde_json::to_string_pretty(descriptor).unwrap_or_default();
        }
    }

    fn propagate_url(&mut self) {
        let mut changed = false;
        if let Some(descriptor) = &mut self.descriptor
            && descriptor.metadata.url != self.url
        {
            descriptor.metadata.url = self.url.clone();
            self.descriptor_json = serde_json::to_string_pretty(descriptor).unwrap_or_default();
            changed = true;
        }
        if changed {
            self.raw_json.clear();
            self.result_json.clear();
            self.array_paths.clear();
            self.error = None;
            self.dirty = true;
        }
    }

    fn apply_descriptor_json(&mut self) {
        if self.descriptor_json.trim().is_empty() {
            return;
        }
        match serde_json::from_str::<ServiceDescriptor>(&self.descriptor_json) {
            Ok(descriptor) => {
                self.url = descriptor.metadata.url.clone();
                self.url_params = url_params_to_vec(&descriptor.metadata.url_params);
                self.descriptor = Some(descriptor);
                self.raw_json.clear();
                self.result_json.clear();
                self.array_paths.clear();
                self.error = None;
                self.dirty = true;
            }
            Err(e) => self.error = Some(format!("Invalid descriptor JSON: {e}")),
        }
    }

    fn reload(&mut self) {
        self.descriptor = roadwork_service::get_descriptor(&self.service);
        self.descriptor_json = match &self.descriptor {
            Some(descriptor) => {
                self.url = descriptor.metadata.url.clone();
                self.url_params = url_params_to_vec(&descriptor.metadata.url_params);
                serde_json::to_string_pretty(descriptor).unwrap_or_default()
            }
            None => String::new(),
        };
        self.dirty = true;
        self.array_paths = Vec::new();
    }

    fn fetch(&mut self, ctx: Context) {
        let url = self.url.clone();
        let fetch_state = Arc::clone(&self.fetch_state);
        crate::roadwork_app::spawn_task(async move {
            *fetch_state.lock().unwrap() = Some(FetchState::Connecting);
            ctx.request_repaint();
            let result = async {
                let response = reqwest::get(&url).await.map_err(|e| e.to_string())?;
                *fetch_state.lock().unwrap() = Some(FetchState::Downloading);
                ctx.request_repaint();
                response
                    .text()
                    .await
                    .map_err(|e| e.to_string())
                    .map(|text| {
                        serde_json::from_str::<serde_json::Value>(&text)
                            .map(|value| {
                                serde_json::to_string_pretty(&value).unwrap_or(text.clone())
                            })
                            .unwrap_or(text)
                    })
            }
            .await;
            *fetch_state.lock().unwrap() = Some(FetchState::Done(result));
            ctx.request_repaint();
        });
    }

    fn roadwork_array_is_valid(&self) -> bool {
        if self.raw_json.trim().is_empty() {
            return true;
        }
        match &self.descriptor {
            Some(descriptor) => {
                let ods = OpendataService::new("Service helper".to_string(), descriptor.clone());
                ods.roadwork_array_targets_array(&self.raw_json)
            }
            None => false,
        }
    }

    fn field_values(&self) -> FieldsValues {
        let Some(descriptor) = &self.descriptor else {
            return FieldsValues::default();
        };
        if self.raw_json.trim().is_empty() {
            return FieldsValues::default();
        }
        let ods = OpendataService::new("Service helper".to_string(), descriptor.clone());
        let path = |path: &str| ods.path_fetched_value(&self.raw_json, path);
        let optional_path = |optional: &Option<String>| optional.as_deref().and_then(path);
        let date_path = |date: &Option<DateParser>| {
            date.as_ref()
                .map(|date| date.path.clone())
                .as_deref()
                .and_then(path)
        };
        FieldsValues {
            roadwork_array: path(&descriptor.roadwork_array),
            id: path(&descriptor.id),
            latitude: optional_path(&descriptor.latitude),
            longitude: optional_path(&descriptor.longitude),
            polygon: optional_path(&descriptor.polygon),
            road: optional_path(&descriptor.road),
            description: optional_path(&descriptor.description),
            location_details: optional_path(&descriptor.location_details),
            impact_circulation_detail: optional_path(&descriptor.impact_circulation_detail),
            url: optional_path(&descriptor.url),
            from_path: date_path(&descriptor.from),
            to_path: date_path(&descriptor.to),
        }
    }

    fn path_candidates(&self) -> PathCandidates {
        let Some(descriptor) = &self.descriptor else {
            return PathCandidates::default();
        };
        if self.raw_json.trim().is_empty() {
            return PathCandidates::default();
        }
        let array_path = &descriptor.roadwork_array;
        PathCandidates {
            scalars: roadwork_core::opendata::json::opendata_service::find_element_scalar_paths(
                &self.raw_json,
                array_path,
            ),
            arrays: roadwork_core::opendata::json::opendata_service::find_element_array_paths(
                &self.raw_json,
                array_path,
            ),
        }
    }

    fn field_validation(&self) -> FieldsValidation {
        let Some(descriptor) = &self.descriptor else {
            return FieldsValidation::valid();
        };
        if self.raw_json.trim().is_empty() {
            return FieldsValidation::valid();
        }
        let ods = OpendataService::new("Service helper".to_string(), descriptor.clone());
        FieldsValidation {
            roadwork_array: self.roadwork_array_is_valid(),
            id: ods.path_points_to_scalar(&self.raw_json, &descriptor.id),
            latitude: descriptor
                .latitude
                .as_deref()
                .map(|path| ods.path_points_to_scalar(&self.raw_json, path))
                .unwrap_or(true),
            longitude: descriptor
                .longitude
                .as_deref()
                .map(|path| ods.path_points_to_scalar(&self.raw_json, path))
                .unwrap_or(true),
            polygon: descriptor
                .polygon
                .as_deref()
                .map(|path| ods.path_points_to_scalar_or_array(&self.raw_json, path))
                .unwrap_or(true),
            road: descriptor
                .road
                .as_deref()
                .map(|path| ods.path_points_to_scalar(&self.raw_json, path))
                .unwrap_or(true),
            description: descriptor
                .description
                .as_deref()
                .map(|path| ods.path_points_to_scalar(&self.raw_json, path))
                .unwrap_or(true),
            location_details: descriptor
                .location_details
                .as_deref()
                .map(|path| ods.path_points_to_scalar(&self.raw_json, path))
                .unwrap_or(true),
            impact_circulation_detail: descriptor
                .impact_circulation_detail
                .as_deref()
                .map(|path| ods.path_points_to_scalar(&self.raw_json, path))
                .unwrap_or(true),
            from_path: descriptor
                .from
                .as_ref()
                .map(|date| ods.path_points_to_scalar(&self.raw_json, &date.path))
                .unwrap_or(true),
            to_path: descriptor
                .to
                .as_ref()
                .map(|date| ods.path_points_to_scalar(&self.raw_json, &date.path))
                .unwrap_or(true),
        }
    }

    fn recompute_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if self.raw_json.is_empty() {
            self.result_json.clear();
            return;
        }
        match &self.descriptor {
            Some(descriptor) => {
                let ods = OpendataService::new("Service helper".to_string(), descriptor.clone());
                match ods.parse_json(&self.raw_json) {
                    Ok(data) => {
                        self.result_json = serde_json::to_string_pretty(&data).unwrap_or_default();
                    }
                    Err(e) => self.result_json = format!("Parse error: {e}"),
                }
            }
            None => self.result_json = "No descriptor".to_string(),
        }
    }
}

fn url_params_to_vec(params: &Option<HashMap<String, String>>) -> Vec<(String, String)> {
    let mut vec: Vec<(String, String)> = params
        .iter()
        .flat_map(|map| map.iter())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    vec.sort_by(|a, b| a.0.cmp(&b.0));
    vec
}
