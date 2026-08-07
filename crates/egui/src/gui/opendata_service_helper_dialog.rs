use super::center_picker_dialog::CenterPickerDialog;
use super::opendata_service_helper_form::{FieldsValidation, FieldsValues, PathCandidates};
use super::service_helper_form::{LABEL_WIDTH, validated};
use egui::{Context, RichText, Ui};
use egui_notify::Toasts;
use roadwork_core::opendata::json::model::opendata_service_descriptor::OpendataServiceDescriptor;
use roadwork_core::opendata::json::opendata_service::OpendataService;
use roadwork_core::opendata::json::path_validation::PathValidation;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
enum FetchState {
    Connecting,
    Downloading,
    Done(Result<String, String>),
}

#[derive(Default)]
pub(crate) struct OpendataServiceHelperDialog {
    descriptor: Option<OpendataServiceDescriptor>,
    descriptor_json: String,
    form_mode: bool,
    wizard_mode: bool,
    is_new: bool,
    creating: bool,
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
    current_index: usize,
    element_count: usize,
    validation_report: Option<Vec<PathValidation>>,
    pending_descriptor: Option<String>,
    #[allow(dead_code)]
    original_name: String,
}

impl OpendataServiceHelperDialog {
    pub(crate) fn new(
        pending_descriptor: Option<String>,
        original_name: String,
        creating: bool,
    ) -> Self {
        let mut dialog = Self {
            form_mode: true,
            wizard_mode: true,
            creating,
            pending_descriptor,
            original_name,
            ..Default::default()
        };
        dialog.apply_initial_descriptor();
        dialog
    }

    pub(crate) fn show(&mut self, ctx: &Context, open: &mut bool, toasts: &mut Toasts) {
        let is_open = *open;
        if is_open && !self.was_open {
            if self.is_new {
                self.wizard_mode = true;
                self.reset();
            } else {
                self.wizard_mode = false;
            }
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
                    self.array_paths = roadwork_core::json_tools::find_json_arrays(&self.raw_json);
                    self.error = None;
                    self.dirty = true;
                    self.validation_report = None;
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
        egui::Window::new("Opendata service helper")
            .open(open)
            .resizable(true)
            .default_size(default_size)
            .max_size(max)
            .show(ctx, |ui| {
                self.show_content(ui, toasts);
            });
    }

    fn show_content(&mut self, ui: &mut Ui, toasts: &mut Toasts) {
        if self.wizard_mode {
            self.show_wizard_content(ui);
        } else {
            self.show_helper_content(ui, toasts);
        }
    }

    fn show_wizard_content(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.heading("Step 1 of 2: Service name and URL");
        });
        ui.separator();
        egui::Panel::bottom("opendata_wizard_footer").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                let can_proceed = self.name_is_valid() && self.url_is_valid();
                if ui
                    .add_enabled(can_proceed, egui::Button::new("Next"))
                    .on_hover_text("Proceed to the opendata service helper")
                    .clicked()
                {
                    self.wizard_mode = false;
                }
            });
        });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.set_max_width(600.0);
            self.show_wizard_form(ui);
        });
    }

    fn show_helper_content(&mut self, ui: &mut Ui, toasts: &mut Toasts) {
        let fetching = matches!(
            &*self.fetch_state.lock().unwrap(),
            Some(FetchState::Connecting) | Some(FetchState::Downloading)
        );
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.form_mode, "Form mode");
            ui.separator();
            if ui
                .add_enabled(!fetching, egui::Button::new("Fetch"))
                .clicked()
            {
                self.fetch(ui.ctx().clone());
            }
            ui.separator();
            let can_validate = !fetching && !self.raw_json.trim().is_empty();
            if ui
                .add_enabled(can_validate, egui::Button::new("Validate"))
                .on_hover_text(
                    "Validate every JSON path against all elements of the opendata array",
                )
                .clicked()
            {
                self.validate_all(toasts);
            }
            ui.separator();
            #[cfg(target_arch = "wasm32")]
            if ui
                .add_enabled(
                    !self.descriptor_json.trim().is_empty(),
                    egui::Button::new("Save to extension"),
                )
                .on_hover_text(
                    "Save the descriptor into the Roadwork WME extension and enable the service",
                )
                .clicked()
            {
                match self.save_to_extension() {
                    Ok(name) => {
                        toasts.success(format!("Service \"{name}\" saved to extension"));
                    }
                    Err(e) => {
                        toasts.error(format!("Save failed: {e}"));
                    }
                }
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
        egui::Panel::left("opendata_helper_left")
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
                        .id_salt("opendata_descriptor_scroll")
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
        egui::Panel::bottom("opendata_helper_result_json")
            .resizable(true)
            .default_size(((available_height - 100.0) * 0.4).max(60.0))
            .min_size(60.0)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Result JSON").strong());
                    if self.element_count > 0 {
                        ui.separator();
                        let up = ui
                            .add_enabled(self.current_index > 0, egui::Button::new("⬆"))
                            .on_hover_text("Previous element");
                        let down = ui
                            .add_enabled(
                                self.current_index + 1 < self.element_count,
                                egui::Button::new("⬇"),
                            )
                            .on_hover_text("Next element");
                        ui.label(format!("{}/{}", self.current_index + 1, self.element_count));
                        if up.clicked() {
                            self.current_index -= 1;
                            self.update_result_json();
                        }
                        if down.clicked() {
                            self.current_index += 1;
                            self.update_result_json();
                        }
                    }
                });
                if let Some(report) = &self.validation_report {
                    ui.separator();
                    self.show_validation_report(ui, report);
                }
                egui::ScrollArea::vertical()
                    .id_salt("opendata_result_json_scroll")
                    .auto_shrink([false, false])
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
        self.show_url_fetch_panel(ui);
    }

    fn show_url_fetch_panel(&mut self, ui: &mut Ui) {
        let (fetching, step) = match &*self.fetch_state.lock().unwrap() {
            Some(FetchState::Connecting) => (true, "Connecting…"),
            Some(FetchState::Downloading) => (true, "Getting data…"),
            _ => (false, ""),
        };
        let mut json_layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
            let theme = egui_extras::syntax_highlighting::CodeTheme::from_style(ui.style());
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
                let can_fetch = !fetching && self.url_is_valid();
                if ui
                    .add_enabled(can_fetch, egui::Button::new("Fetch"))
                    .on_hover_text("URL must start with http:// or https://")
                    .clicked()
                {
                    self.fetch(ui.ctx().clone());
                }
                if !self.url.is_empty() && !self.url_is_valid() {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        "URL must start with http:// or https://",
                    );
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
                .id_salt("opendata_raw_json_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut self.raw_json)
                            .code_editor()
                            .interactive(true)
                            .desired_width(f32::INFINITY)
                            .desired_rows(15)
                            .layouter(&mut json_layouter),
                    );
                    if response.changed() {
                        self.validation_report = None;
                    }
                });
        });
    }

    fn show_wizard_form(&mut self, ui: &mut Ui) {
        let Self {
            descriptor,
            descriptor_json,
            dirty,
            url,
            raw_json,
            result_json,
            array_paths,
            error,
            validation_report,
            ..
        } = self;
        let Some(descriptor) = descriptor else {
            ui.colored_label(ui.visuals().error_fg_color, "No descriptor available");
            return;
        };
        let mut name_changed = false;
        ui.horizontal(|ui| {
            ui.add_sized(
                [LABEL_WIDTH, 20.0],
                egui::Label::new(RichText::new("Name").strong()),
            );
            name_changed = ui
                .add(
                    egui::TextEdit::singleline(&mut descriptor.metadata.name)
                        .desired_width(f32::INFINITY),
                )
                .changed();
        });
        let url_changed = validated(
            ui,
            url.trim().is_empty() || Self::is_valid_http_url(url),
            "URL must start with http:// or https://",
            |ui, _| {
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [LABEL_WIDTH, 20.0],
                        egui::Label::new(RichText::new("URL").strong()),
                    );
                    let response =
                        ui.add(egui::TextEdit::singleline(url).desired_width(f32::INFINITY));
                    response.changed()
                })
                .inner
            },
            None,
        );
        if url_changed {
            descriptor.metadata.url = url.clone();
            raw_json.clear();
            result_json.clear();
            array_paths.clear();
            *error = None;
            *validation_report = None;
        }
        if name_changed || url_changed {
            *dirty = true;
            *descriptor_json = serde_json::to_string_pretty(descriptor).unwrap_or_default();
        }
    }

    fn name_is_valid(&self) -> bool {
        self.descriptor
            .as_ref()
            .is_some_and(|descriptor| !descriptor.metadata.name.trim().is_empty())
    }

    fn url_is_valid(&self) -> bool {
        Self::is_valid_http_url(&self.url)
    }

    fn is_valid_http_url(url: &str) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
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
            .id_salt("opendata_descriptor_form_scroll")
            .max_height(available_height - 24.0)
            .show(ui, |ui| match descriptor {
                Some(descriptor) => {
                    let changed = crate::gui::opendata_service_helper_form::show(
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
            self.validation_report = None;
        }
    }

    fn apply_descriptor_json(&mut self) -> bool {
        if self.descriptor_json.trim().is_empty() {
            return false;
        }
        match serde_json::from_str::<OpendataServiceDescriptor>(&self.descriptor_json) {
            Ok(descriptor) => {
                self.url = descriptor.metadata.url.clone();
                self.url_params = url_params_to_vec(&descriptor.metadata.url_params);
                self.descriptor = Some(descriptor);
                self.raw_json.clear();
                self.result_json.clear();
                self.array_paths.clear();
                self.error = None;
                self.dirty = true;
                self.validation_report = None;
                true
            }
            Err(e) => {
                self.error = Some(format!("Invalid descriptor JSON: {e}"));
                false
            }
        }
    }

    fn apply_initial_descriptor(&mut self) {
        self.is_new = self.creating;
        if self.is_new {
            self.reset();
            return;
        }
        match &self.pending_descriptor {
            Some(json) if !json.trim().is_empty() => {
                self.descriptor_json = json.clone();
                self.apply_descriptor_json();
            }
            _ => self.reset(),
        }
    }

    fn reset(&mut self) {
        let descriptor = OpendataServiceDescriptor::default();
        self.descriptor = Some(descriptor);
        self.url.clear();
        self.url_params.clear();
        self.descriptor_json = self
            .descriptor
            .as_ref()
            .map(|descriptor| serde_json::to_string_pretty(descriptor).unwrap_or_default())
            .unwrap_or_default();
        self.raw_json.clear();
        self.result_json.clear();
        self.error = None;
        self.dirty = true;
        self.array_paths = Vec::new();
        self.current_index = 0;
        self.element_count = 0;
        self.validation_report = None;
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

    fn opendata_array_is_valid(&self) -> bool {
        if self.raw_json.trim().is_empty() {
            return true;
        }
        match &self.descriptor {
            Some(descriptor) => {
                let ods = OpendataService {
                    service_name: "Opendata service helper".to_string(),
                    service_descriptor: descriptor.clone(),
                };
                ods.roadwork_array_targets_array(&self.raw_json)
            }
            None => false,
        }
    }

    fn validate_all(&mut self, toasts: &mut Toasts) {
        let Some(descriptor) = &self.descriptor else {
            self.error = Some("No descriptor available".to_string());
            return;
        };
        if self.raw_json.trim().is_empty() {
            self.error = Some("Fetch the JSON first".to_string());
            return;
        }
        let ods = OpendataService {
            service_name: "Opendata service helper".to_string(),
            service_descriptor: descriptor.clone(),
        };
        let report = ods.validate(&self.raw_json);
        let valid = report
            .iter()
            .filter(|validation| validation.is_valid())
            .count();
        let total = report.len();
        let failed = total - valid;
        self.validation_report = Some(report);
        if failed == 0 {
            toasts.success(format!("Validation: all {total} checks passed"));
        } else {
            let message = format!("Validation: {failed}/{total} checks failed");
            self.error = Some(message.clone());
            toasts.error(message);
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn save_to_extension(&self) -> Result<String, String> {
        let descriptor: OpendataServiceDescriptor =
            serde_json::from_str(&self.descriptor_json).map_err(|e| e.to_string())?;
        let name = descriptor.metadata.name.clone();
        let object = js_sys::Object::new();
        js_sys::Reflect::set(
            &object,
            &wasm_bindgen::JsValue::from_str("type"),
            &wasm_bindgen::JsValue::from_str("ROADWORK_SAVE_OPENDATA_DESCRIPTOR"),
        )
        .map_err(|e| format!("{e:?}"))?;
        js_sys::Reflect::set(
            &object,
            &wasm_bindgen::JsValue::from_str("name"),
            &wasm_bindgen::JsValue::from_str(&name),
        )
        .map_err(|e| format!("{e:?}"))?;
        if !self.original_name.is_empty() && self.original_name != name {
            js_sys::Reflect::set(
                &object,
                &wasm_bindgen::JsValue::from_str("oldName"),
                &wasm_bindgen::JsValue::from_str(&self.original_name),
            )
            .map_err(|e| format!("{e:?}"))?;
        }
        js_sys::Reflect::set(
            &object,
            &wasm_bindgen::JsValue::from_str("descriptor"),
            &wasm_bindgen::JsValue::from_str(&self.descriptor_json),
        )
        .map_err(|e| format!("{e:?}"))?;
        let window = web_sys::window().ok_or("No window available")?;
        let parent = window.parent().map_err(|e| format!("{e:?}"))?;
        let parent = parent.ok_or("No parent window")?;
        parent
            .post_message(&object, "*")
            .map_err(|e| format!("{e:?}"))?;
        Ok(name)
    }

    fn show_validation_report(&self, ui: &mut Ui, report: &[PathValidation]) {
        let valid = report
            .iter()
            .filter(|validation| validation.is_valid())
            .count();
        let total = report.len();
        let summary_color = if valid == total {
            ui.visuals().hyperlink_color
        } else {
            ui.visuals().error_fg_color
        };
        ui.label(
            RichText::new(format!("Validation: {valid}/{total} checks passed"))
                .color(summary_color)
                .strong(),
        );
        egui::ScrollArea::vertical()
            .id_salt("opendata_validation_scroll")
            .max_height(120.0)
            .show(ui, |ui| {
                for validation in report {
                    ui.horizontal(|ui| {
                        if validation.is_valid() {
                            ui.label(RichText::new("✓").color(ui.visuals().hyperlink_color));
                            ui.label(format!(
                                "{} → {} ({})",
                                validation.label, validation.path, validation.expected
                            ));
                        } else {
                            ui.label(RichText::new("✗").color(ui.visuals().error_fg_color));
                            let message = match validation.message {
                                Some(message) => message.to_string(),
                                None => {
                                    let indices: Vec<String> = validation
                                        .failures
                                        .iter()
                                        .map(|index| index.to_string())
                                        .collect();
                                    format!(
                                        "{} → {}: failed in {}/{} elements ({})",
                                        validation.label,
                                        validation.path,
                                        validation.failures.len(),
                                        validation.element_count,
                                        indices.join(", ")
                                    )
                                }
                            };
                            ui.colored_label(ui.visuals().error_fg_color, message);
                        }
                    });
                }
            });
    }

    fn field_values(&self) -> FieldsValues {
        let Some(descriptor) = &self.descriptor else {
            return FieldsValues::default();
        };
        if self.raw_json.trim().is_empty() {
            return FieldsValues::default();
        }
        let ods = OpendataService {
            service_name: "Opendata service helper".to_string(),
            service_descriptor: descriptor.clone(),
        };
        let Some(element) = ods.element_at(&self.raw_json, self.current_index) else {
            return FieldsValues::default();
        };
        let path = |path: &str| ods.path_fetched_value_in(&element, path);
        let optional_path = |optional: &Option<String>| optional.as_deref().and_then(path);
        FieldsValues {
            data_array: path(&descriptor.data_array),
            id: path(&descriptor.id),
            latitude: optional_path(&descriptor.latitude),
            longitude: optional_path(&descriptor.longitude),
            polygon: optional_path(&descriptor.polygon),
            description: optional_path(&descriptor.description),
        }
    }

    fn path_candidates(&self) -> PathCandidates {
        let Some(descriptor) = &self.descriptor else {
            return PathCandidates::default();
        };
        if self.raw_json.trim().is_empty() {
            return PathCandidates::default();
        }
        let ods = OpendataService {
            service_name: "Opendata service helper".to_string(),
            service_descriptor: descriptor.clone(),
        };
        let Some(element) = ods.element_at(&self.raw_json, self.current_index) else {
            return PathCandidates::default();
        };
        PathCandidates {
            scalars: roadwork_core::json_tools::element_scalar_paths(&element),
            arrays: roadwork_core::json_tools::element_array_paths(&element),
        }
    }

    fn field_validation(&self) -> FieldsValidation {
        let Some(descriptor) = &self.descriptor else {
            return FieldsValidation::valid();
        };
        if self.raw_json.trim().is_empty() {
            return FieldsValidation::valid();
        }
        let ods = OpendataService {
            service_name: "Opendata service helper".to_string(),
            service_descriptor: descriptor.clone(),
        };
        let Some(element) = ods.element_at(&self.raw_json, self.current_index) else {
            return FieldsValidation::valid();
        };
        FieldsValidation {
            data_array: self.opendata_array_is_valid(),
            id: ods.path_points_to_scalar_in(&element, &descriptor.id),
            latitude: descriptor
                .latitude
                .as_deref()
                .map(|path| ods.path_points_to_scalar_in(&element, path))
                .unwrap_or(true),
            longitude: descriptor
                .longitude
                .as_deref()
                .map(|path| ods.path_points_to_scalar_in(&element, path))
                .unwrap_or(true),
            polygon: descriptor
                .polygon
                .as_deref()
                .map(|path| ods.path_points_to_scalar_or_array_in(&element, path))
                .unwrap_or(true),
            description: descriptor
                .description
                .as_deref()
                .map(|path| ods.path_points_to_scalar_in(&element, path))
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
            self.element_count = 0;
            self.current_index = 0;
            return;
        }
        match &self.descriptor {
            Some(descriptor) => {
                let ods = OpendataService {
                    service_name: "Opendata service helper".to_string(),
                    service_descriptor: descriptor.clone(),
                };
                self.element_count = ods.element_count(&self.raw_json);
                if self.element_count == 0 {
                    self.current_index = 0;
                } else {
                    self.current_index = self.current_index.min(self.element_count - 1);
                }
                self.update_result_json();
            }
            None => {
                self.element_count = 0;
                self.current_index = 0;
                self.result_json = "No descriptor".to_string();
            }
        }
    }

    fn update_result_json(&mut self) {
        let Some(descriptor) = &self.descriptor else {
            self.result_json = "No descriptor".to_string();
            return;
        };
        if self.raw_json.is_empty() {
            self.result_json.clear();
            return;
        }
        let ods = OpendataService {
            service_name: "Opendata service helper".to_string(),
            service_descriptor: descriptor.clone(),
        };
        match ods.extract_roadwork_array(&self.raw_json) {
            Ok(array) => {
                let element = array
                    .as_array()
                    .and_then(|elements| elements.get(self.current_index))
                    .cloned()
                    .unwrap_or(array);
                self.result_json = serde_json::to_string_pretty(&element).unwrap_or_default();
            }
            Err(e) => self.result_json = format!("Parse error: {e}"),
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
