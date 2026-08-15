use super::center_picker_dialog::CenterPickerDialog;
use super::service_helper_form::{FieldsValidation, FieldsValues, PathCandidates};
use crate::tools::{format_bytes, url_params_to_vec};
use egui::{Context, RichText, Ui};
use egui_notify::Toasts;
use roadwork_core::json_tools::{JsonScan, JsonTools};
use roadwork_core::model::opendata::Opendata;
use roadwork_core::model::opendata_data::OpendataData;
use roadwork_core::opendata::json::model::date_parser::DateParser;
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;
use roadwork_core::opendata::json::opendata_service::OpendataService;
use roadwork_core::opendata::json::path_validation::PathValidation;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Bytes consumed by the JSON scan per frame while importing a dropped file.
const SCAN_STEP_BUDGET: usize = 128 * 1024;

/// Opendata elements built per frame by the cooperative import runner.
const IMPORT_FRAME_BUDGET: usize = 1024;

/// Maximum number of elements previewed in the data table. The preview is
/// always built from the first `PREVIEW_MAX_ELEMENTS` of the data array.
const PREVIEW_MAX_ELEMENTS: usize = 100;

/// Maximum characters rendered in a single table cell.
const MAX_CELL_CHARS: usize = 512;

/// The source the helper dialog is currently working on.
///
/// Built-in sources are the services embedded in the extension: they are
/// read-only reference descriptors and are never saved back. Custom sources
/// are user-defined opendata services that can embed data and be saved into
/// the extension settings.
#[derive(Clone)]
enum HelperMode {
    Builtin {
        service: String,
    },
    Custom {
        original_name: String,
        creating: bool,
    },
}

impl Default for HelperMode {
    fn default() -> Self {
        Self::Builtin {
            service: String::new(),
        }
    }
}

#[derive(Clone)]
enum FetchState {
    Connecting,
    Downloading,
    Done(Result<String, String>),
}

#[derive(Default)]
pub(crate) struct ServiceHelperDialog {
    mode: HelperMode,
    descriptor: Option<ServiceDescriptor>,
    descriptor_json: String,
    form_mode: bool,
    url_params: Vec<(String, String)>,
    url: String,
    raw_json: String,
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
    pending_data_applied: bool,
    opendata_bytes: usize,
    opendata_count: usize,
    drop_scan: Option<DropScan>,
    parsed_json: Option<Arc<serde_json::Value>>,
    parsed_opendata: Option<serde_json::Value>,
    processing: Arc<Mutex<ImportProgress>>,
    importing: bool,
    import_runner: Option<ImportRunner>,
}

struct DropScan {
    file_name: String,
    scan: JsonScan,
}

/// Shared, cooperatively-updated state of the background import pipeline.
#[derive(Default)]
struct ImportProgress {
    label: String,
    progress: f32,
    result: Option<ImportPayload>,
    error: Option<String>,
}

struct ImportPayload {
    element_count: usize,
    parsed_opendata: Option<serde_json::Value>,
    opendata_count: usize,
    array_paths: Vec<(String, usize)>,
}

/// Cooperative, frame-polled post-scan import pipeline.
///
/// Built like [`JsonScan`]: each `poll()` call does a bounded amount of work and
/// returns `true` once the whole import (or an error) has been produced. This
/// keeps the UI responsive and animated on wasm without relying on the async
/// executor or timers. Runs on the UI thread, so any panic is caught and
/// surfaced as a visible error instead of silently hanging the dialog.
struct ImportRunner {
    ods: OpendataService,
    value: Arc<serde_json::Value>,
    total: usize,
    index: usize,
    items: Vec<Opendata>,
    array_pointer: Option<String>,
}

impl ImportRunner {
    fn new(ods: OpendataService, value: Arc<serde_json::Value>) -> Self {
        Self {
            ods,
            value,
            total: 0,
            index: 0,
            items: Vec::new(),
            array_pointer: None,
        }
    }

    /// Advances the import by up to `IMPORT_FRAME_BUDGET` elements. Returns
    /// `true` when the import is finished (result or error set in `state`).
    fn poll(&mut self, state: &Arc<Mutex<ImportProgress>>, ctx: &Context) -> bool {
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.poll_inner(state, ctx)));
        match result {
            Ok(done) => done,
            Err(payload) => {
                let message = payload
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_string())
                    .or_else(|| payload.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".to_string());
                log::error!("Import runner panicked: {message}");
                state.lock().unwrap().error = Some(format!("Import failed: {message}"));
                ctx.request_repaint();
                true
            }
        }
    }

    fn poll_inner(&mut self, state: &Arc<Mutex<ImportProgress>>, ctx: &Context) -> bool {
        if self.array_pointer.is_none() {
            if self.ods.service_descriptor.data_array.trim().is_empty() {
                let message = "Unable to parse the data: the descriptor has no data_array path — \
                               fill it in the descriptor form before importing."
                    .to_string();
                log::error!("{message}");
                state.lock().unwrap().error = Some(message);
                ctx.request_repaint();
                return true;
            }
            self.array_pointer = data_array_pointer(&self.ods.service_descriptor.data_array);
            match self.array_pointer.as_ref() {
                Some(pointer) => match self.value.pointer(pointer).and_then(|node| node.as_array())
                {
                    Some(array) => self.total = array.len(),
                    None => {
                        let message = format!(
                            "Unable to parse the data: no array found at {}",
                            self.ods.service_descriptor.data_array
                        );
                        log::error!("{message}");
                        state.lock().unwrap().error = Some(message);
                        ctx.request_repaint();
                        return true;
                    }
                },
                None => match self.ods.roadwork_array(&self.value) {
                    Ok(array) => self.total = array.len(),
                    Err(e) => {
                        log::error!("Unable to query the data array: {e}");
                        state.lock().unwrap().error =
                            Some(format!("Unable to parse the data: {e}"));
                        ctx.request_repaint();
                        return true;
                    }
                },
            }
            if self.total == 0 {
                self.finalize(state, ctx);
                return true;
            }
            self.set_progress(state, ctx, format!("Building… 0/{}", self.total), 0.0);
        }
        let end = (self.index + IMPORT_FRAME_BUDGET).min(self.total);
        if let Some(pointer) = self.array_pointer.as_ref() {
            if let Some(array) = self.value.pointer(pointer).and_then(|node| node.as_array()) {
                for node in &array[self.index..end] {
                    if let Ok(item) = self.ods.build_opendata(node)
                        && OpendataService::is_valid(&item)
                    {
                        self.items.push(item);
                    }
                }
            }
        } else if let Ok(array) = self.ods.roadwork_array(&self.value) {
            for node in &array[self.index..end] {
                if let Ok(item) = self.ods.build_opendata(node)
                    && OpendataService::is_valid(&item)
                {
                    self.items.push(item);
                }
            }
        }
        self.index = end;
        if self.index < self.total {
            self.set_progress(
                state,
                ctx,
                format!("Building… {}/{}", self.index, self.total),
                self.index as f32 / self.total as f32,
            );
            ctx.request_repaint();
            return false;
        }
        self.finalize(state, ctx);
        true
    }

    fn set_progress(
        &self,
        state: &Arc<Mutex<ImportProgress>>,
        ctx: &Context,
        label: String,
        progress: f32,
    ) {
        let mut state = state.lock().unwrap();
        state.label = label;
        state.progress = progress;
        ctx.request_repaint();
    }

    fn finalize(&mut self, state: &Arc<Mutex<ImportProgress>>, ctx: &Context) {
        let data = OpendataData::new(&self.ods.service_name, std::mem::take(&mut self.items));
        state.lock().unwrap().result = Some(ImportPayload {
            element_count: self.total,
            parsed_opendata: serde_json::to_value(&data).ok(),
            opendata_count: data.opendata.len(),
            array_paths: roadwork_core::json_tools::find_json_arrays_value(&self.value),
        });
        let mut state = state.lock().unwrap();
        state.label = "Done".to_string();
        state.progress = 1.0;
        ctx.request_repaint();
    }
}

/// Converts an opendata `data_array` path (`$.a.b[*]`) into a JSON Pointer
/// (`/a/b`) when the path is simple (a single trailing `[*]` over dotted keys
/// without brackets in the base). Returns `None` for exotic paths, in which
/// case the import falls back to jsonpath querying per chunk.
fn data_array_pointer(path: &str) -> Option<String> {
    let base = path.trim().strip_suffix("[*]")?;
    if base.is_empty() || base.contains('[') || base.contains(']') {
        return None;
    }
    let base = base.strip_prefix("$.").unwrap_or(base);
    let base = base.strip_prefix('$').unwrap_or(base);
    if base.is_empty() {
        return None;
    }
    Some(format!("/{}", base.replace('.', "/")))
}

/// Columns shown in the preview table. `id` is always present (required
/// field); the optional ones appear only when selected in the descriptor.
struct TableColumns {
    id: bool,
    latitude: bool,
    longitude: bool,
    polygon: bool,
    description: bool,
}

impl From<&ServiceDescriptor> for TableColumns {
    fn from(descriptor: &ServiceDescriptor) -> Self {
        Self {
            id: true,
            latitude: descriptor.latitude.is_some(),
            longitude: descriptor.longitude.is_some(),
            polygon: descriptor.polygon.is_some(),
            description: descriptor.description.is_some(),
        }
    }
}

/// A single row of the preview table, pre-formatted for display.
struct PreviewRow {
    id: String,
    latitude: String,
    longitude: String,
    polygon_count: usize,
    description: String,
}

impl PreviewRow {
    /// Built directly from a raw element of the fetched JSON, so the preview
    /// shows values even before the item has a valid location.
    fn from_raw(
        ods: &OpendataService,
        descriptor: &ServiceDescriptor,
        element: &serde_json::Value,
    ) -> Self {
        let fetched = |path: &Option<String>| {
            path.as_deref()
                .and_then(|path| ods.path_fetched_value_in(element, path))
                .unwrap_or_default()
        };
        Self {
            id: ods
                .path_fetched_value_in(element, &descriptor.id)
                .unwrap_or_default(),
            latitude: fetched(&descriptor.latitude),
            longitude: fetched(&descriptor.longitude),
            polygon_count: descriptor
                .polygon
                .as_deref()
                .and_then(|path| element.get_path_as_polygons(path))
                .map(|polygons| polygons.len())
                .unwrap_or(0),
            description: fetched(&descriptor.description),
        }
    }

    fn from_parsed(item: &serde_json::Value) -> Self {
        let fetched = |v: Option<&serde_json::Value>| -> String {
            match v {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(serde_json::Value::Number(n)) => n.to_string(),
                Some(serde_json::Value::Bool(b)) => b.to_string(),
                _ => String::new(),
            }
        };
        Self {
            id: fetched(item.get("id")),
            latitude: fetched(item.get("latitude")),
            longitude: fetched(item.get("longitude")),
            polygon_count: item
                .get("polygons")
                .and_then(|p| p.as_array())
                .map(|p| p.len())
                .unwrap_or(0),
            description: fetched(item.get("description")),
        }
    }
}

fn display_cell(text: &str) -> String {
    if text.is_empty() {
        "—".to_string()
    } else if text.chars().count() > MAX_CELL_CHARS {
        text.chars().take(MAX_CELL_CHARS).collect::<String>() + "…"
    } else {
        text.to_string()
    }
}

/// A source picked from the dialog combo.
enum SourceSelection {
    Builtin(String),
    Custom(String),
    New,
}

impl ServiceHelperDialog {
    /// Opens the helper on a built-in service (read-only descriptor).
    pub(crate) fn for_builtin(service: &str) -> Self {
        let mut dialog = Self {
            mode: HelperMode::Builtin {
                service: service.to_string(),
            },
            form_mode: true,
            ..Default::default()
        };
        dialog.reload();
        dialog
    }

    /// Opens the helper on a user-defined opendata service.
    pub(crate) fn for_custom(
        pending_descriptor: Option<String>,
        original_name: String,
        creating: bool,
    ) -> Self {
        let mut dialog = Self {
            mode: HelperMode::Custom {
                original_name,
                creating,
            },
            form_mode: true,
            pending_descriptor,
            ..Default::default()
        };
        dialog.apply_initial_descriptor();
        dialog
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        open: &mut bool,
        toasts: &mut Toasts,
        standalone: bool,
        custom_services: &HashMap<String, ServiceDescriptor>,
    ) {
        let is_open = *open;
        if is_open && !self.was_open {
            if self.creating() {
                self.reset();
            }
            if !self.url.is_empty() {
                self.fetch(ui.ctx().clone());
            }
        }
        self.was_open = is_open;
        if is_open && !self.pending_data_applied && !self.is_builtin() {
            self.pending_data_applied = self.apply_pending_data();
        }
        let result = match &*self.fetch_state.lock().unwrap() {
            Some(FetchState::Done(result)) => Some(result.clone()),
            _ => None,
        };
        if let Some(result) = result {
            *self.fetch_state.lock().unwrap() = None;
            match result {
                Ok(text) => {
                    self.raw_json = text;
                    self.parsed_json = None;
                    self.array_paths = roadwork_core::json_tools::find_json_arrays(&self.raw_json);
                    self.auto_fill_data_array();
                    self.error = None;
                    self.dirty = true;
                    self.validation_report = None;
                    toasts.success("Fetch succeeded");
                }
                Err(e) => {
                    self.error = Some(e.clone());
                    toasts.error(format!("Fetch error: {e}"));
                }
            }
        }
        let dropped_files: Vec<egui::DroppedFile> = ui.ctx().input(|i| i.raw.dropped_files.clone());
        for file in dropped_files {
            if let Err(e) = self.start_dropped_import(file) {
                toasts.error(format!("Import failed: {e}"));
            }
        }
        self.step_drop_scan(ui.ctx(), toasts);
        if self.importing {
            let done = self
                .import_runner
                .as_mut()
                .map(|runner| runner.poll(&self.processing, ui.ctx()))
                .unwrap_or(true);
            if done {
                self.import_runner = None;
            }
            self.poll_import_result(ui.ctx(), toasts);
        }
        self.recompute_if_dirty();

        if standalone {
            self.show_content(ui, toasts, custom_services);
        } else {
            let screen = ui.ctx().content_rect().size();
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
                .show(ui.ctx(), |ui| {
                    self.show_content(ui, toasts, custom_services);
                });
        }
        self.show_save_progress_modal(ui.ctx());
    }

    /// Renders the "saving to extension" progress modal. Shown while a save is in
    /// flight (with a 120s watchdog), then replaced by a success or error state with
    /// an OK button that closes the helper overlay.
    fn show_save_progress_modal(&self, ctx: &Context) {
        let mut state = crate::roadwork_app::save_progress_state();
        if !state.active && state.done.is_none() && state.error.is_none() {
            return;
        }
        if state.active && js_sys::Date::now() - state.last_update > 120_000.0 {
            state.active = false;
            state.error = Some("Save timed out after 120 seconds".to_string());
        }
        let mut ok = false;
        egui::Modal::new(egui::Id::new("save_progress_modal")).show(ctx, |ui| {
            ui.set_min_width(340.0);
            if state.active {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Saving to extension").strong());
                });
                ui.add_space(8.0);
                ui.label(&state.stage);
                ui.add_space(4.0);
                if state.fraction < 0.0 {
                    ui.add(egui::ProgressBar::new(0.5).animate(true));
                } else {
                    ui.add(egui::ProgressBar::new(state.fraction).show_percentage());
                }
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            } else if let Some(name) = &state.done {
                ui.label(
                    RichText::new("Saved")
                        .strong()
                        .color(ui.visuals().hyperlink_color),
                );
                ui.add_space(4.0);
                ui.label(format!("Service \"{name}\" saved to extension"));
                ui.add_space(8.0);
                if ui.button("OK").clicked() {
                    ok = true;
                }
            } else if let Some(error) = &state.error {
                ui.label(
                    RichText::new("Save failed")
                        .strong()
                        .color(ui.visuals().error_fg_color),
                );
                ui.add_space(4.0);
                ui.label(error);
                ui.add_space(8.0);
                if ui.button("OK").clicked() {
                    ok = true;
                }
            }
        });
        if ok {
            crate::roadwork_app::finish_save_progress();
            crate::roadwork_app::close_helper_overlay();
        }
    }

    fn is_builtin(&self) -> bool {
        matches!(self.mode, HelperMode::Builtin { .. })
    }

    /// The helper edits either a built-in roadwork service (full Roadwork model:
    /// road, location_details, impact_circulation_detail, dates) or a plain opendata
    /// source (only id/latitude/longitude/polygon/description).
    fn is_opendata_mode(&self) -> bool {
        !self.is_builtin()
    }

    fn creating(&self) -> bool {
        matches!(self.mode, HelperMode::Custom { creating: true, .. })
    }

    fn original_name(&self) -> &str {
        match &self.mode {
            HelperMode::Builtin { .. } => "",
            HelperMode::Custom { original_name, .. } => original_name,
        }
    }

    fn builtin_service(&self) -> Option<&str> {
        match &self.mode {
            HelperMode::Builtin { service } => Some(service),
            HelperMode::Custom { .. } => None,
        }
    }

    fn source_label(&self) -> String {
        match &self.mode {
            HelperMode::Builtin { service } => service.clone(),
            HelperMode::Custom {
                original_name,
                creating,
            } => {
                if !creating && !original_name.is_empty() {
                    original_name.clone()
                } else {
                    self.descriptor
                        .as_ref()
                        .map(|d| d.metadata.name.clone())
                        .filter(|n| !n.trim().is_empty())
                        .unwrap_or_else(|| "New opendata source".to_string())
                }
            }
        }
    }

    fn show_source_combo(
        &mut self,
        ui: &mut Ui,
        custom_services: &HashMap<String, ServiceDescriptor>,
    ) {
        let builtin_names: Vec<String> = roadwork_service::get_services()
            .into_iter()
            .map(|s| s.name)
            .collect();
        let mut custom_names: Vec<String> = custom_services.keys().cloned().collect();
        custom_names.sort();
        let mut switch: Option<SourceSelection> = None;
        egui::ComboBox::from_label("Source")
            .selected_text(self.source_label())
            .show_ui(ui, |ui| {
                let current_mode = &self.mode;
                ui.label(RichText::new("Built-in services").weak());
                for name in &builtin_names {
                    let selected = matches!(
                        current_mode,
                        HelperMode::Builtin { service } if service == name
                    );
                    if ui.selectable_label(selected, name).clicked() {
                        switch = Some(SourceSelection::Builtin(name.clone()));
                    }
                }
                ui.separator();
                ui.label(RichText::new("Opendata sources").weak());
                for name in &custom_names {
                    let selected = matches!(
                        current_mode,
                        HelperMode::Custom { original_name, creating: false }
                            if original_name == name
                    );
                    if ui.selectable_label(selected, name).clicked() {
                        switch = Some(SourceSelection::Custom(name.clone()));
                    }
                }
                ui.separator();
                let is_new = matches!(current_mode, HelperMode::Custom { creating: true, .. });
                if ui
                    .selectable_label(is_new, "+ New opendata source")
                    .clicked()
                {
                    switch = Some(SourceSelection::New);
                }
            });
        if let Some(selection) = switch {
            self.apply_source_selection(selection, custom_services);
        }
    }

    fn apply_source_selection(
        &mut self,
        selection: SourceSelection,
        custom_services: &HashMap<String, ServiceDescriptor>,
    ) {
        self.pending_data_applied = false;
        match selection {
            SourceSelection::Builtin(name) => {
                self.mode = HelperMode::Builtin { service: name };
                self.reload();
            }
            SourceSelection::Custom(name) => {
                let Some(descriptor) = custom_services.get(&name) else {
                    return;
                };
                let Ok(json) = super::pretty_json_tabs(descriptor) else {
                    return;
                };
                self.mode = HelperMode::Custom {
                    original_name: name,
                    creating: false,
                };
                self.descriptor_json = json;
                self.apply_descriptor_json();
            }
            SourceSelection::New => {
                self.mode = HelperMode::Custom {
                    original_name: String::new(),
                    creating: true,
                };
                self.reset();
            }
        }
    }

    fn show_content(
        &mut self,
        ui: &mut Ui,
        toasts: &mut Toasts,
        custom_services: &HashMap<String, ServiceDescriptor>,
    ) {
        let fetching = matches!(
            &*self.fetch_state.lock().unwrap(),
            Some(FetchState::Connecting) | Some(FetchState::Downloading)
        );
        ui.horizontal(|ui| {
            self.show_source_combo(ui, custom_services);
            ui.separator();
            let (mode_text, mode_color) = if self.is_builtin() {
                ("Mode: Roadwork", ui.visuals().hyperlink_color)
            } else {
                ("Mode: Opendata", ui.visuals().warn_fg_color)
            };
            ui.label(RichText::new(mode_text).strong().color(mode_color));
            ui.separator();
            ui.checkbox(&mut self.form_mode, "Form mode");
            ui.separator();
            if ui
                .add_enabled(!fetching && self.url_is_valid(), egui::Button::new("Fetch"))
                .on_hover_text("Fetch the data from the service URL (requires an http(s) URL)")
                .clicked()
            {
                self.fetch(ui.ctx().clone());
            }
            if self.is_builtin()
                && ui
                    .button("Reload")
                    .on_hover_text("Reload the built-in descriptor")
                    .clicked()
            {
                self.reload();
            }
            ui.separator();
            let can_validate = !fetching && !self.raw_json.trim().is_empty();
            if ui
                .add_enabled(can_validate, egui::Button::new("Validate"))
                .on_hover_text("Validate every JSON path against all elements of the data array")
                .clicked()
            {
                self.validate_all(toasts);
            }
            ui.separator();
            let can_save = {
                let has_data = !self.raw_json.trim().is_empty() || self.parsed_opendata.is_some();
                !self.is_builtin()
                    && !self.descriptor_json.trim().is_empty()
                    && has_data
                    && self.has_name()
            };
            if ui
                .add_enabled(can_save, egui::Button::new("Save to extension"))
                .on_hover_text(if self.importing {
                    "Save the descriptor and the data into the Roadwork WME extension \
                     (the data will be built from the imported JSON on save)"
                } else {
                    "Save the descriptor and the data into the Roadwork WME extension \
                     and enable the service"
                })
                .on_disabled_hover_text(if self.is_builtin() {
                    "Built-in services are read-only"
                } else {
                    "A service name and imported data are required"
                })
                .clicked()
            {
                crate::roadwork_app::start_save_progress();
                if let Err(e) = self.save_to_extension() {
                    crate::roadwork_app::finish_save_progress();
                    toasts.error(format!("Save failed: {e}"));
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
                        .id_salt("helper_descriptor_scroll")
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
        self.show_center_panel(ui);
    }

    fn show_data_drop_zone(&mut self, ui: &mut Ui) {
        let hovering = ui.ctx().input(|i| !i.raw.hovered_files.is_empty());
        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::symmetric(8, 14))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    if let Some(scan) = &self.drop_scan {
                        ui.add(
                            egui::ProgressBar::new(scan.scan.progress())
                                .text(format!("Importing {}…", scan.file_name)),
                        );
                        ui.label(format!("{:.0}%", scan.scan.progress() * 100.0));
                    } else if self.importing {
                        let (label, progress) = {
                            let state = self.processing.lock().unwrap();
                            (state.label.clone(), state.progress)
                        };
                        ui.add(egui::ProgressBar::new(progress).text(label));
                        ui.label(format!("{:.0}%", progress * 100.0));
                    } else if hovering {
                        ui.colored_label(
                            ui.visuals().hyperlink_color,
                            "Drop the file to import it",
                        );
                    } else if !self.raw_json.trim().is_empty() {
                        ui.colored_label(
                            ui.visuals().hyperlink_color,
                            "JSON loaded — pick the data_array path in the descriptor (✨) \
                             to build the data.",
                        );
                    } else {
                        ui.label("Drop a .json data file here to import it, or fetch from the URL above.");
                    }
                });
            });
    }

    fn show_preview_table(&self, ui: &mut Ui) {
        let Some(descriptor) = &self.descriptor else {
            return;
        };
        let columns = TableColumns::from(descriptor);
        let rows = self.preview_rows(descriptor);
        let mut table = egui_extras::TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(egui_extras::Column::auto().at_least(36.0));
        if columns.id {
            table = table.column(egui_extras::Column::remainder().at_least(80.0));
        }
        if columns.latitude {
            table = table.column(egui_extras::Column::auto().at_least(70.0));
        }
        if columns.longitude {
            table = table.column(egui_extras::Column::auto().at_least(70.0));
        }
        if columns.polygon {
            table = table.column(egui_extras::Column::auto().at_least(70.0));
        }
        if columns.description {
            table = table.column(egui_extras::Column::remainder().at_least(80.0));
        }
        table
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("#");
                });
                if columns.id {
                    header.col(|ui| {
                        ui.strong("id");
                    });
                }
                if columns.latitude {
                    header.col(|ui| {
                        ui.strong("latitude");
                    });
                }
                if columns.longitude {
                    header.col(|ui| {
                        ui.strong("longitude");
                    });
                }
                if columns.polygon {
                    header.col(|ui| {
                        ui.strong("polygones");
                    });
                }
                if columns.description {
                    header.col(|ui| {
                        ui.strong("description");
                    });
                }
            })
            .body(|body| {
                body.rows(18.0, rows.len(), |mut row| {
                    let index = row.index();
                    let item = &rows[index];
                    row.col(|ui| {
                        ui.label((index + 1).to_string());
                    });
                    if columns.id {
                        row.col(|ui| {
                            ui.label(display_cell(&item.id));
                        });
                    }
                    if columns.latitude {
                        row.col(|ui| {
                            ui.label(display_cell(&item.latitude));
                        });
                    }
                    if columns.longitude {
                        row.col(|ui| {
                            ui.label(display_cell(&item.longitude));
                        });
                    }
                    if columns.polygon {
                        row.col(|ui| {
                            ui.label(item.polygon_count.to_string());
                        });
                    }
                    if columns.description {
                        row.col(|ui| {
                            ui.label(display_cell(&item.description));
                        });
                    }
                });
            });
    }

    fn preview_rows(&self, descriptor: &ServiceDescriptor) -> Vec<PreviewRow> {
        if let Some(value) = self.parsed_json.as_deref() {
            let ods = OpendataService::from(descriptor);
            ods.roadwork_array(value)
                .unwrap_or_default()
                .into_iter()
                .take(PREVIEW_MAX_ELEMENTS)
                .map(|element| PreviewRow::from_raw(&ods, descriptor, element))
                .collect()
        } else {
            let opendata = self
                .parsed_opendata
                .as_ref()
                .and_then(|value| value.get("opendata"))
                .and_then(|o| o.as_object())
                .map(|o| o.values().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            opendata
                .into_iter()
                .take(PREVIEW_MAX_ELEMENTS)
                .map(|item| PreviewRow::from_parsed(&item))
                .collect()
        }
    }

    fn show_center_panel(&mut self, ui: &mut Ui) {
        let (fetching, step) = match &*self.fetch_state.lock().unwrap() {
            Some(FetchState::Connecting) => (true, "Connecting…".to_string()),
            Some(FetchState::Downloading) => (true, "Getting data…".to_string()),
            _ => (false, String::new()),
        };
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.vertical(|ui| {
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

                if !self.raw_json.is_empty() {
                    ui.label(format!(
                        "Fetched JSON · {}",
                        format_bytes(self.raw_json.len())
                    ));
                }

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Sample data").strong());
                    if let Some(count) = self.data_count() {
                        ui.separator();
                        if self.opendata_bytes > 0 {
                            ui.label(format!(
                                "{count} data · {}",
                                format_bytes(self.opendata_bytes)
                            ));
                        } else {
                            ui.label(format!("{count} data"));
                        }
                    }
                });
                if let Some(report) = &self.validation_report {
                    ui.separator();
                    self.show_validation_report(ui, report);
                }
                ui.separator();
                if self.fields_chosen() {
                    self.show_preview_table(ui);
                } else {
                    self.show_data_drop_zone(ui);
                }
            });
        });
    }

    fn url_is_valid(&self) -> bool {
        self.url.starts_with("http://") || self.url.starts_with("https://")
    }

    /// Total number of data elements in the current data source, regardless of
    /// how many rows the preview table displays.
    fn data_count(&self) -> Option<usize> {
        if self.parsed_opendata.is_some() {
            Some(self.opendata_count)
        } else if self.parsed_json.is_some() {
            Some(self.element_count)
        } else {
            None
        }
    }

    fn show_form(&mut self, ui: &mut Ui, available_height: f32) {
        let field_validation = self.field_validation();
        let field_values = self.field_values();
        let path_candidates = self.path_candidates();
        let opendata_mode = self.is_opendata_mode();
        let Self {
            descriptor,
            descriptor_json,
            dirty,
            url_params,
            center_picker,
            center_picker_open,
            url,
            importing,
            import_runner,
            processing,
            ..
        } = self;
        egui::ScrollArea::vertical()
            .id_salt("helper_form_scroll")
            .max_height(available_height - 24.0)
            .show(ui, |ui| match descriptor {
                Some(descriptor) => {
                    let data_signature_before = data_signature(descriptor);
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
                        opendata_mode,
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
                        *descriptor_json = super::pretty_json_tabs(descriptor).unwrap_or_default();
                        *url = descriptor.metadata.url.clone().unwrap_or_default();
                        if data_signature_before != data_signature(descriptor) {
                            *dirty = true;
                            *importing = false;
                            *import_runner = None;
                            *processing.lock().unwrap() = ImportProgress::default();
                            self.parsed_opendata = None;
                            self.opendata_bytes = 0;
                            self.opendata_count = 0;
                        }
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
            *descriptor_json = super::pretty_json_tabs(descriptor).unwrap_or_default();
        }
    }

    fn propagate_url(&mut self) {
        let mut changed = false;
        if let Some(descriptor) = &mut self.descriptor {
            let new_url = if self.url.trim().is_empty() {
                None
            } else {
                Some(self.url.clone())
            };
            if descriptor.metadata.url != new_url {
                descriptor.metadata.url = new_url;
                self.descriptor_json = super::pretty_json_tabs(descriptor).unwrap_or_default();
                changed = true;
            }
        }
        if changed {
            self.raw_json.clear();
            self.parsed_json = None;
            self.array_paths.clear();
            self.error = None;
            self.dirty = true;
            self.validation_report = None;
            self.cancel_import();
            self.parsed_opendata = None;
            self.opendata_bytes = 0;
            self.opendata_count = 0;
        }
    }

    fn apply_descriptor_json(&mut self) -> bool {
        if self.descriptor_json.trim().is_empty() {
            return false;
        }
        match serde_json::from_str::<ServiceDescriptor>(&self.descriptor_json) {
            Ok(descriptor) => {
                self.url = descriptor.metadata.url.clone().unwrap_or_default();
                self.url_params = url_params_to_vec(&descriptor.metadata.url_params);
                self.descriptor = Some(descriptor);
                self.raw_json.clear();
                self.parsed_json = None;
                self.array_paths.clear();
                self.error = None;
                self.dirty = true;
                self.validation_report = None;
                self.cancel_import();
                self.parsed_opendata = None;
                self.opendata_bytes = 0;
                self.opendata_count = 0;
                true
            }
            Err(e) => {
                self.error = Some(format!("Invalid descriptor JSON: {e}"));
                false
            }
        }
    }

    fn apply_initial_descriptor(&mut self) {
        if self.creating() {
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

    /// Reloads the descriptor of the built-in service currently edited.
    fn reload(&mut self) {
        let Some(service) = self.builtin_service() else {
            return;
        };
        self.descriptor = roadwork_service::get_descriptor(service);
        self.descriptor_json = match &self.descriptor {
            Some(descriptor) => {
                self.url = descriptor.metadata.url.clone().unwrap_or_default();
                self.url_params = url_params_to_vec(&descriptor.metadata.url_params);
                super::pretty_json_tabs(descriptor).unwrap_or_default()
            }
            None => String::new(),
        };
        self.dirty = true;
        self.array_paths = Vec::new();
        self.validation_report = None;
    }

    fn reset(&mut self) {
        let descriptor = ServiceDescriptor::default();
        self.descriptor = Some(descriptor);
        self.url.clear();
        self.url_params.clear();
        self.descriptor_json = self
            .descriptor
            .as_ref()
            .map(|descriptor| super::pretty_json_tabs(descriptor).unwrap_or_default())
            .unwrap_or_default();
        self.raw_json.clear();
        self.parsed_json = None;
        self.error = None;
        self.dirty = true;
        self.array_paths = Vec::new();
        self.current_index = 0;
        self.element_count = 0;
        self.validation_report = None;
        self.parsed_opendata = None;
        self.opendata_bytes = 0;
        self.opendata_count = 0;
        self.importing = false;
        self.import_runner = None;
        self.drop_scan = None;
        *self.processing.lock().unwrap() = ImportProgress::default();
    }

    fn set_opendata_data(&mut self, json: String) {
        let value = serde_json::from_str(&json).unwrap_or(serde_json::Value::Null);
        self.set_opendata_data_value(value);
        self.opendata_bytes = json.len();
    }

    fn set_opendata_data_value(&mut self, value: serde_json::Value) {
        self.opendata_count = value
            .get("opendata")
            .and_then(|o| o.as_object())
            .map(|map| map.len())
            .unwrap_or(0);
        self.opendata_bytes = serde_json::to_string(&value).map(|s| s.len()).unwrap_or(0);
        self.parsed_opendata = Some(value);
    }

    fn start_dropped_import(&mut self, file: egui::DroppedFile) -> Result<(), String> {
        let name = file.name.clone();
        let content = if let Some(path) = file.path {
            std::fs::read_to_string(&path)
                .map_err(|e| format!("Failed to read {}: {e}", path.display()))?
        } else if let Some(bytes) = file.bytes {
            String::from_utf8(bytes.to_vec()).map_err(|e| format!("File is not UTF-8: {e}"))?
        } else {
            return Err(format!("No readable content in dropped file \"{name}\""));
        };
        if content.trim().is_empty() {
            return Err(format!("Dropped file \"{name}\" is empty"));
        }
        self.drop_scan = Some(DropScan {
            file_name: name,
            scan: JsonScan::new(&content),
        });
        self.error = None;
        Ok(())
    }

    fn step_drop_scan(&mut self, ctx: &Context, toasts: &mut Toasts) {
        let Some(mut job) = self.drop_scan.take() else {
            return;
        };
        if !job.scan.is_done() {
            job.scan.step(SCAN_STEP_BUDGET);
        }
        if job.scan.is_done() {
            self.apply_scan_result(job, toasts);
        } else {
            self.drop_scan = Some(job);
            ctx.request_repaint();
        }
    }

    fn apply_scan_result(&mut self, job: DropScan, toasts: &mut Toasts) {
        let file_name = job.file_name;
        let (source, result) = job.scan.into_parts();
        match result {
            Err(e) => {
                self.error = Some(format!("Invalid JSON in \"{file_name}\": {e}"));
                toasts.error(format!("Import failed: {e}"));
            }
            Ok(value) => {
                if let Some(name) = value
                    .get("metadata")
                    .and_then(|m| m.get("name"))
                    .and_then(|n| n.as_str())
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                {
                    self.descriptor_json = super::pretty_json_tabs(&value).unwrap_or(source);
                    self.apply_descriptor_json();
                    toasts.success(format!("Descriptor \"{name}\" loaded from {file_name}"));
                } else if is_opendata_data_value(&value) {
                    self.raw_json.clear();
                    self.error = None;
                    self.dirty = true;
                    self.validation_report = None;
                    self.cancel_import();
                    self.set_opendata_data_value(value);
                    toasts.success(format!("Data imported from {file_name}"));
                } else {
                    self.raw_json = super::pretty_json_tabs(&value).unwrap_or(source);
                    let value = Arc::new(value);
                    self.parsed_json = Some(Arc::clone(&value));
                    self.array_paths = roadwork_core::json_tools::find_json_arrays_value(&value);
                    self.error = None;
                    self.validation_report = None;
                    self.parsed_opendata = None;
                    self.opendata_bytes = 0;
                    self.opendata_count = 0;
                    self.element_count = 0;
                    self.current_index = 0;
                    self.auto_fill_data_array();
                    let has_data_array = self
                        .descriptor
                        .as_ref()
                        .is_some_and(|d| !d.data_array.trim().is_empty());
                    if has_data_array {
                        self.importing = self.start_import_processing(value);
                        toasts.success(format!("Data imported from {file_name}"));
                    } else {
                        toasts.success(format!(
                            "JSON imported from {file_name} — pick the data_array path (✨) to build the data"
                        ));
                    }
                }
            }
        }
    }

    /// Fills `data_array` with the unique array found in the fetched/imported
    /// JSON, if any. Returns `true` when the path was filled.
    fn auto_fill_data_array(&mut self) -> bool {
        if self.array_paths.len() != 1 {
            return false;
        }
        let Some(descriptor) = self.descriptor.as_mut() else {
            return false;
        };
        if !descriptor.data_array.trim().is_empty() {
            return false;
        }
        descriptor.data_array = format!("{}[*]", self.array_paths[0].0);
        self.descriptor_json = super::pretty_json_tabs(descriptor).unwrap_or_default();
        true
    }

    /// Starts the cooperative import for the dropped raw data. Returns `false`
    /// when no descriptor is loaded, so nothing can be built.
    fn start_import_processing(&mut self, value: Arc<serde_json::Value>) -> bool {
        let Some(descriptor) = &self.descriptor else {
            return false;
        };
        let ods = OpendataService::from(descriptor);
        *self.processing.lock().unwrap() = ImportProgress {
            label: "Parsing…".to_string(),
            progress: 0.0,
            result: None,
            error: None,
        };
        self.import_runner = Some(ImportRunner::new(ods, value));
        true
    }

    fn poll_import_result(&mut self, ctx: &Context, toasts: &mut Toasts) {
        let mut state = self.processing.lock().unwrap();
        if let Some(e) = state.error.take() {
            self.importing = false;
            self.error = Some(e.clone());
            toasts.error(format!("Import failed: {e}"));
            return;
        }
        let Some(payload) = state.result.take() else {
            ctx.request_repaint();
            return;
        };
        self.importing = false;
        self.element_count = payload.element_count;
        if self.element_count == 0 {
            self.current_index = 0;
        } else {
            self.current_index = self.current_index.min(self.element_count - 1);
        }
        self.parsed_opendata = payload.parsed_opendata;
        self.opendata_count = payload.opendata_count;
        self.opendata_bytes = self
            .parsed_opendata
            .as_ref()
            .and_then(|value| serde_json::to_string(value).ok())
            .map(|json| json.len())
            .unwrap_or(0);
        self.array_paths = payload.array_paths;
    }

    /// Applies data posted by the extension content script (`ROADWORK_HELPER_DATA`)
    /// once it arrives. Returns `true` when data has been consumed.
    fn apply_pending_data(&mut self) -> bool {
        let Some(json) = crate::roadwork_app::take_pending_opendata_data() else {
            return false;
        };
        if is_opendata_data(&json) {
            self.raw_json.clear();
            self.parsed_json = None;
            self.array_paths = Vec::new();
            self.cancel_import();
            self.set_opendata_data(json);
        } else {
            self.raw_json = serde_json::from_str::<serde_json::Value>(&json)
                .map(|value| super::pretty_json_tabs(&value).unwrap_or(json.clone()))
                .unwrap_or(json);
            self.parsed_json = None;
            self.array_paths = roadwork_core::json_tools::find_json_arrays(&self.raw_json);
            self.cancel_import();
            self.parsed_opendata = None;
            self.opendata_bytes = 0;
            self.opendata_count = 0;
        }
        self.error = None;
        self.dirty = true;
        self.validation_report = None;
        true
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
                            .map(|value| super::pretty_json_tabs(&value).unwrap_or(text.clone()))
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
        let Some(descriptor) = &self.descriptor else {
            return false;
        };
        let ods = OpendataService::from(descriptor);
        if let Some(value) = &self.parsed_json {
            ods.roadwork_array_targets_array_value(value)
        } else {
            ods.roadwork_array_targets_array(&self.raw_json)
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
        let ods = OpendataService::from(descriptor);
        let report = ods.validate_roadworks(&self.raw_json);
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

    fn save_to_extension(&self) -> Result<String, String> {
        let descriptor: ServiceDescriptor =
            serde_json::from_str(&self.descriptor_json).map_err(|e| e.to_string())?;
        let name = descriptor.metadata.name.clone();
        if name.trim().is_empty() {
            return Err("A service name is required".to_string());
        }
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
        let original_name = self.original_name();
        if !original_name.is_empty() && original_name != name {
            js_sys::Reflect::set(
                &object,
                &wasm_bindgen::JsValue::from_str("oldName"),
                &wasm_bindgen::JsValue::from_str(original_name),
            )
            .map_err(|e| format!("{e:?}"))?;
        }
        js_sys::Reflect::set(
            &object,
            &wasm_bindgen::JsValue::from_str("descriptor"),
            &wasm_bindgen::JsValue::from_str(&self.descriptor_json),
        )
        .map_err(|e| format!("{e:?}"))?;
        let data = match &self.parsed_opendata {
            Some(data) => serde_json::to_string(data).map_err(|e| e.to_string())?,
            None => match self.build_opendata_sync() {
                Some(data) => serde_json::to_string(&data).map_err(|e| e.to_string())?,
                None => String::new(),
            },
        };
        if !data.is_empty() {
            js_sys::Reflect::set(
                &object,
                &wasm_bindgen::JsValue::from_str("data"),
                &wasm_bindgen::JsValue::from_str(&data),
            )
            .map_err(|e| format!("{e:?}"))?;
        }
        let window = web_sys::window().ok_or("No window available")?;
        let parent = window.parent().map_err(|e| format!("{e:?}"))?;
        let parent = parent.ok_or("No parent window")?;
        parent
            .post_message(&object, "*")
            .map_err(|e| format!("{e:?}"))?;
        Ok(name)
    }

    /// Synchronously builds the opendata payload from the current raw JSON and
    /// descriptor, mirroring the cooperative [`ImportRunner`] finalization.
    /// Used by the save flow when a background import has not finished yet, so
    /// saving always ships data even mid-rebuild.
    fn build_opendata_sync(&self) -> Option<OpendataData> {
        let descriptor = self.descriptor.as_ref()?;
        if self.raw_json.trim().is_empty() {
            return None;
        }
        let value: serde_json::Value = serde_json::from_str(&self.raw_json).ok()?;
        let ods = OpendataService::from(descriptor);
        let items: Vec<Opendata> = match data_array_pointer(&descriptor.data_array) {
            Some(pointer) => value
                .pointer(&pointer)
                .and_then(|node| node.as_array())
                .map(|array| {
                    array
                        .iter()
                        .filter_map(|node| ods.build_opendata(node).ok())
                        .filter(OpendataService::is_valid)
                        .collect()
                })
                .unwrap_or_default(),
            None => ods
                .roadwork_array(&value)
                .ok()
                .map(|array| {
                    array
                        .iter()
                        .filter_map(|node| ods.build_opendata(node).ok())
                        .filter(OpendataService::is_valid)
                        .collect()
                })
                .unwrap_or_default(),
        };
        Some(OpendataData::new(&ods.service_name, items))
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
            .id_salt("helper_validation_scroll")
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

    fn current_element(&self) -> Option<serde_json::Value> {
        let descriptor = self.descriptor.as_ref()?;
        let ods = OpendataService::from(descriptor);
        if let Some(value) = self.parsed_json.as_deref() {
            ods.element_at_value(value, self.current_index)
        } else if !self.raw_json.trim().is_empty() {
            ods.element_at(&self.raw_json, self.current_index)
        } else {
            None
        }
    }

    fn fields_chosen(&self) -> bool {
        let Some(descriptor) = &self.descriptor else {
            return false;
        };
        !descriptor.data_array.trim().is_empty()
            || !descriptor.id.trim().is_empty()
            || descriptor.latitude.is_some()
            || descriptor.longitude.is_some()
            || descriptor.polygon.is_some()
            || descriptor.description.is_some()
    }

    fn field_values(&self) -> FieldsValues {
        let Some(descriptor) = &self.descriptor else {
            return FieldsValues::default();
        };
        let Some(element) = self.current_element() else {
            return FieldsValues::default();
        };
        let ods = OpendataService::from(descriptor);
        let path = |path: &str| ods.path_fetched_value_in(&element, path);
        let optional_path = |optional: &Option<String>| optional.as_deref().and_then(path);
        let date_path = |date: &Option<DateParser>| {
            date.as_ref()
                .map(|date| date.path.clone())
                .as_deref()
                .and_then(path)
        };
        FieldsValues {
            data_array: path(&descriptor.data_array),
            id: path(&descriptor.id),
            latitude: optional_path(&descriptor.latitude),
            longitude: optional_path(&descriptor.longitude),
            polygon: optional_path(&descriptor.polygon),
            road: optional_path(&descriptor.road),
            description: optional_path(&descriptor.description),
            location_details: optional_path(&descriptor.location_details),
            impact_circulation_detail: optional_path(&descriptor.impact_circulation_detail),
            from_path: date_path(&descriptor.from),
            to_path: date_path(&descriptor.to),
        }
    }

    fn path_candidates(&self) -> PathCandidates {
        let Some(element) = self.current_element() else {
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
        let Some(element) = self.current_element() else {
            return FieldsValidation::valid();
        };
        let ods = OpendataService::from(descriptor);
        let scalar = |path: &Option<String>| {
            path.as_deref()
                .map(|path| ods.path_points_to_scalar_in(&element, path))
                .unwrap_or(true)
        };
        FieldsValidation {
            data_array: self.opendata_array_is_valid(),
            id: ods.path_points_to_scalar_in(&element, &descriptor.id),
            latitude: scalar(&descriptor.latitude),
            longitude: scalar(&descriptor.longitude),
            polygon: descriptor
                .polygon
                .as_deref()
                .map(|path| ods.path_points_to_scalar_or_array_in(&element, path))
                .unwrap_or(true),
            road: scalar(&descriptor.road),
            description: scalar(&descriptor.description),
            location_details: scalar(&descriptor.location_details),
            impact_circulation_detail: scalar(&descriptor.impact_circulation_detail),
            from_path: descriptor
                .from
                .as_ref()
                .map(|date| ods.path_points_to_scalar_in(&element, &date.path))
                .unwrap_or(true),
            to_path: descriptor
                .to
                .as_ref()
                .map(|date| ods.path_points_to_scalar_in(&element, &date.path))
                .unwrap_or(true),
        }
    }

    fn recompute_if_dirty(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        if self.raw_json.is_empty() {
            self.element_count = 0;
            self.current_index = 0;
            return;
        }
        match &self.descriptor {
            Some(descriptor) => {
                let ods = OpendataService::from(descriptor);
                if self.parsed_json.is_none() {
                    self.parsed_json = serde_json::from_str(&self.raw_json).ok().map(Arc::new);
                }
                if let Some(value) = self.parsed_json.as_deref() {
                    self.element_count = ods.element_count_value(value);
                } else {
                    self.element_count = 0;
                }
                if self.element_count == 0 {
                    self.current_index = 0;
                } else {
                    self.current_index = self.current_index.min(self.element_count - 1);
                }
                self.cancel_import();
                self.parsed_opendata = None;
                self.opendata_bytes = 0;
                self.opendata_count = 0;
                self.ensure_build_started();
            }
            None => {
                self.element_count = 0;
                self.current_index = 0;
            }
        }
    }

    /// Starts the cooperative import on the current raw JSON when the descriptor
    /// and data array are ready and nothing is already importing or fully built.
    fn ensure_build_started(&mut self) {
        if self.importing || self.import_runner.is_some() {
            return;
        }
        if self.parsed_opendata.is_some() {
            return;
        }
        if self.raw_json.trim().is_empty() || self.element_count == 0 {
            return;
        }
        let Some(value) = self.parsed_json.clone() else {
            return;
        };
        let Some(descriptor) = &self.descriptor else {
            return;
        };
        if descriptor.data_array.trim().is_empty() {
            return;
        }
        self.importing = self.start_import_processing(value);
    }

    /// Stops a running import (data or descriptor changed, so any in-flight
    /// result would be stale) and clears its shared progress state.
    fn cancel_import(&mut self) {
        self.importing = false;
        self.import_runner = None;
        *self.processing.lock().unwrap() = ImportProgress::default();
    }

    fn has_name(&self) -> bool {
        self.descriptor
            .as_ref()
            .map(|descriptor| !descriptor.metadata.name.trim().is_empty())
            .unwrap_or(false)
    }
}

fn is_opendata_data(json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .is_some_and(|value| value.get("opendata").is_some_and(|o| o.is_object()))
}

fn is_opendata_data_value(value: &serde_json::Value) -> bool {
    value.get("opendata").is_some_and(|o| o.is_object())
}

/// Compact signature of the data-affecting fields of a descriptor. Metadata
/// edits (name, url, center…) leave it unchanged, so the form only restarts
/// the import when the paths that shape the parsed data actually change.
fn data_signature(descriptor: &ServiceDescriptor) -> String {
    let mut value = serde_json::to_value(descriptor).unwrap_or_default();
    if let Some(object) = value.as_object_mut() {
        object.remove("metadata");
    }
    value.to_string()
}
