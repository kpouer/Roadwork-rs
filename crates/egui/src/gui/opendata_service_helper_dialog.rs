use super::center_picker_dialog::CenterPickerDialog;
use super::opendata_service_helper_form::{FieldsValidation, FieldsValues, PathCandidates};
use super::service_helper_form::{ElementMemo, MemoKey};
use egui::{Context, RichText, Ui};
use egui_notify::Toasts;
use roadwork_core::json_tools::{JsonScan, JsonTools};
use roadwork_core::model::opendata::Opendata;
use roadwork_core::model::opendata_data::OpendataData;
use roadwork_core::opendata::json::model::opendata_service_descriptor::OpendataServiceDescriptor;
use roadwork_core::opendata::json::opendata_service::OpendataService;
use roadwork_core::opendata::json::path_validation::PathValidation;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Bytes consumed by the JSON scan per frame while importing a dropped file.
const SCAN_STEP_BUDGET: usize = 128 * 1024;

/// Opendata elements built per frame by the cooperative import runner.
const IMPORT_FRAME_BUDGET: usize = 1024;

/// Maximum number of elements previewed in the opendata table. The preview is
/// always built from the first `PREVIEW_MAX_ELEMENTS` of the data array.
const PREVIEW_MAX_ELEMENTS: usize = 100;

/// Payload produced by the native "Save" button, persisted into the local
/// settings by the caller.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct SavedOpendata {
    pub name: String,
    pub original_name: String,
    pub descriptor: OpendataServiceDescriptor,
    pub data: Option<OpendataData>,
}

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
    pending_data_applied: bool,
    opendata_bytes: usize,
    opendata_count: usize,
    drop_scan: Option<DropScan>,
    parsed_json: Option<Arc<serde_json::Value>>,
    parsed_opendata: Option<serde_json::Value>,
    processing: Arc<Mutex<ImportProgress>>,
    importing: bool,
    import_runner: Option<ImportRunner>,
    memo: ElementMemo,
    memo_key: MemoKey,
    #[cfg(not(target_arch = "wasm32"))]
    saved: Option<SavedOpendata>,
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
    result_json: String,
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
    elements: Option<Vec<serde_json::Value>>,
    total: usize,
    limit: usize,
    index: usize,
    items: Vec<Opendata>,
}

impl ImportRunner {
    fn new(ods: OpendataService, value: Arc<serde_json::Value>) -> Self {
        Self {
            ods,
            value,
            elements: None,
            total: 0,
            limit: 0,
            index: 0,
            items: Vec::new(),
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
        if self.elements.is_none() {
            if self.ods.service_descriptor.data_array.trim().is_empty() {
                let message = "Unable to parse the data: the descriptor has no dataArray path — \
                               fill it in the descriptor form before importing."
                    .to_string();
                log::error!("{message}");
                state.lock().unwrap().error = Some(message);
                ctx.request_repaint();
                return true;
            }
            match self.ods.roadwork_array(&self.value) {
                Ok(array) => {
                    self.total = array.len();
                    self.limit = self.total.min(PREVIEW_MAX_ELEMENTS);
                    self.elements = Some(array.into_iter().take(self.limit).cloned().collect());
                }
                Err(e) => {
                    log::error!("Unable to query the data array: {e}");
                    state.lock().unwrap().error = Some(format!("Unable to parse the data: {e}"));
                    ctx.request_repaint();
                    return true;
                }
            }
            if self.total == 0 {
                self.finalize(state, ctx);
                return true;
            }
            self.set_progress(state, ctx, format!("Building… 0/{}", self.limit), 0.0);
        }
        let end = (self.index + IMPORT_FRAME_BUDGET).min(self.limit);
        if let Some(elements) = self.elements.as_ref() {
            for node in &elements[self.index..end] {
                if let Ok(item) = self.ods.build_opendata(node)
                    && OpendataService::is_valid(&item)
                {
                    self.items.push(item);
                }
            }
        }
        self.index = end;
        if self.index < self.limit {
            self.set_progress(
                state,
                ctx,
                format!("Building… {}/{}", self.index, self.limit),
                self.index as f32 / self.limit as f32,
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
        let result_json = self
            .elements
            .as_ref()
            .and_then(|elements| elements.first())
            .map(|element| super::pretty_json_tabs(element).unwrap_or_default())
            .unwrap_or_else(|| "[]".to_string());
        state.lock().unwrap().result = Some(ImportPayload {
            element_count: self.total,
            parsed_opendata: serde_json::to_value(&data).ok(),
            opendata_count: data.opendata.len(),
            result_json,
            array_paths: roadwork_core::json_tools::find_json_arrays_value(&self.value),
        });
        let mut state = state.lock().unwrap();
        state.label = "Done".to_string();
        state.progress = 1.0;
        ctx.request_repaint();
    }
}

/// Columns shown in the opendata preview table. `id` is always present
/// (required field); the optional ones appear only when selected in the
/// descriptor.
struct TableColumns {
    id: bool,
    latitude: bool,
    longitude: bool,
    polygon: bool,
    description: bool,
}

impl TableColumns {
    fn from_descriptor(descriptor: &OpendataServiceDescriptor) -> Self {
        Self {
            id: true,
            latitude: descriptor.latitude.is_some(),
            longitude: descriptor.longitude.is_some(),
            polygon: descriptor.polygon.is_some(),
            description: descriptor.description.is_some(),
        }
    }
}

/// A single row of the opendata preview table, pre-formatted for display.
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
        descriptor: &OpendataServiceDescriptor,
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

/// Maximum characters rendered in a single table cell.
const MAX_CELL_CHARS: usize = 512;

/// Above this size, raw JSON editors are not shown at all to avoid
/// laying out a huge document overflowing egui.
const MAX_EDITABLE_JSON_BYTES: usize = 4 * 1024 * 1024;

impl OpendataServiceHelperDialog {
    pub(crate) fn new(
        pending_descriptor: Option<String>,
        original_name: String,
        creating: bool,
    ) -> Self {
        let mut dialog = Self {
            form_mode: true,
            creating,
            pending_descriptor,
            original_name,
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
    ) {
        let is_open = *open;
        if is_open && !self.was_open {
            if self.is_new {
                self.reset();
            }
            if !self.url.is_empty() {
                self.fetch(ui.ctx().clone());
            }
        }
        self.was_open = is_open;
        if is_open && !self.pending_data_applied {
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
                    self.result_json = format!("Fetch error: {e}");
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
            self.show_content(ui, toasts);
        } else {
            let screen = ui.ctx().content_rect().size();
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
                .show(ui.ctx(), |ui| {
                    self.show_content(ui, toasts);
                });
        }
    }

    fn show_content(&mut self, ui: &mut Ui, toasts: &mut Toasts) {
        self.show_helper_content(ui, toasts);
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
                .add_enabled(!fetching && self.url_is_valid(), egui::Button::new("Fetch"))
                .on_hover_text("Fetch the data from the service URL (requires an http(s) URL)")
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
            let can_save = {
                let has_data = !self.raw_json.trim().is_empty() || self.parsed_opendata.is_some();
                !self.descriptor_json.trim().is_empty() && has_data
            };
            #[cfg(target_arch = "wasm32")]
            if ui
                .add_enabled(can_save, egui::Button::new("Save to extension"))
                .on_hover_text(
                    "Save the descriptor and the opendata into the Roadwork WME extension \
                     and enable the service",
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
            #[cfg(not(target_arch = "wasm32"))]
            if ui
                .add_enabled(
                    !self.descriptor_json.trim().is_empty(),
                    egui::Button::new("Save"),
                )
                .on_hover_text("Save the descriptor and the opendata into the local settings")
                .clicked()
            {
                match self.prepare_save() {
                    Ok(saved) => {
                        self.saved = Some(saved);
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
                    ui.label(RichText::new("Sample data").strong());
                    if self.opendata_count > 0 {
                        ui.separator();
                        if self.element_count > PREVIEW_MAX_ELEMENTS {
                            ui.label(format!(
                                "first {} of {} items · {}",
                                self.opendata_count,
                                self.element_count,
                                format_bytes(self.opendata_bytes)
                            ));
                        } else {
                            ui.label(format!(
                                "{} items · {}",
                                self.opendata_count,
                                format_bytes(self.opendata_bytes)
                            ));
                        }
                    }
                });
                if let Some(report) = &self.validation_report {
                    ui.separator();
                    self.show_validation_report(ui, report);
                }
                ui.separator();
                if self.fields_chosen() {
                    self.show_opendata_table(ui);
                } else {
                    self.show_data_drop_zone(ui);
                }
            });
        self.show_url_fetch_panel(ui);
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
                        ui.add(
                            egui::ProgressBar::new(progress)
                                .text(label),
                        );
                        ui.label(format!("{:.0}%", progress * 100.0));
                    } else if hovering {
                        ui.colored_label(
                            ui.visuals().hyperlink_color,
                            "Drop the file to import it",
                        );
                    } else if !self.raw_json.trim().is_empty() {
                        ui.colored_label(
                            ui.visuals().hyperlink_color,
                            "JSON loaded — pick the dataArray path in the descriptor (✨) \
                             to build the opendata.",
                        );
                    } else {
                        ui.label("Drop a .json data file here to import it, or fetch from the URL above.");
                    }
                });
            });
    }

    fn show_opendata_table(&self, ui: &mut Ui) {
        let Some(descriptor) = &self.descriptor else {
            return;
        };
        let columns = TableColumns::from_descriptor(descriptor);
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

    fn preview_rows(&self, descriptor: &OpendataServiceDescriptor) -> Vec<PreviewRow> {
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

    fn show_url_fetch_panel(&mut self, ui: &mut Ui) {
        let (fetching, step) = match &*self.fetch_state.lock().unwrap() {
            Some(FetchState::Connecting) => (true, "Connecting…".to_string()),
            Some(FetchState::Downloading) => (true, "Getting data…".to_string()),
            _ => (false, String::new()),
        };
        let mut json_layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
            super::layout_json_text(ui, text, wrap_width)
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
                    let editable = self.raw_json.len() <= MAX_EDITABLE_JSON_BYTES;
                    if !editable {
                        ui.colored_label(
                            ui.visuals().error_fg_color,
                            format!(
                                "Document too large to edit ({}) — no preview available",
                                format_bytes(self.raw_json.len())
                            ),
                        );
                    }
                    if editable {
                        let response = ui.add(
                            egui::TextEdit::multiline(&mut self.raw_json)
                                .code_editor()
                                .interactive(true)
                                .desired_width(f32::INFINITY)
                                .desired_rows(15)
                                .layouter(&mut json_layouter),
                        );
                        if response.changed() {
                            self.parsed_json = None;
                            self.validation_report = None;
                        }
                    }
                });
        });
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
                        *descriptor_json = super::pretty_json_tabs(descriptor).unwrap_or_default();
                        *url = descriptor.metadata.url.clone().unwrap_or_default();
                        self.parsed_opendata = None;
                        self.opendata_bytes = 0;
                        self.opendata_count = 0;
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
            self.result_json.clear();
            self.array_paths.clear();
            self.error = None;
            self.dirty = true;
            self.validation_report = None;
            self.parsed_opendata = None;
            self.opendata_bytes = 0;
            self.opendata_count = 0;
        }
    }

    fn apply_descriptor_json(&mut self) -> bool {
        if self.descriptor_json.trim().is_empty() {
            return false;
        }
        match serde_json::from_str::<OpendataServiceDescriptor>(&self.descriptor_json) {
            Ok(descriptor) => {
                self.url = descriptor.metadata.url.clone().unwrap_or_default();
                self.url_params = url_params_to_vec(&descriptor.metadata.url_params);
                self.descriptor = Some(descriptor);
                self.raw_json.clear();
                self.parsed_json = None;
                self.result_json.clear();
                self.array_paths.clear();
                self.error = None;
                self.dirty = true;
                self.validation_report = None;
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
            .map(|descriptor| super::pretty_json_tabs(descriptor).unwrap_or_default())
            .unwrap_or_default();
        self.raw_json.clear();
        self.parsed_json = None;
        self.result_json.clear();
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

    #[cfg(target_arch = "wasm32")]
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
                    self.result_json.clear();
                    self.error = None;
                    self.dirty = true;
                    self.validation_report = None;
                    self.set_opendata_data_value(value);
                    toasts.success(format!("Data imported from {file_name}"));
                } else {
                    self.raw_json = super::pretty_json_tabs(&value).unwrap_or(source);
                    let value = Arc::new(value);
                    self.parsed_json = Some(Arc::clone(&value));
                    self.array_paths = roadwork_core::json_tools::find_json_arrays_value(&value);
                    self.result_json.clear();
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
                            "JSON imported from {file_name} — pick the dataArray path (✨) to build the opendata"
                        ));
                    }
                }
            }
        }
    }

    /// Fills `dataArray` with the unique array found in the fetched/imported
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
        self.result_json = payload.result_json;
        self.array_paths = payload.array_paths;
    }

    /// Applies data posted by the extension content script (`ROADWORK_HELPER_DATA`)
    /// once it arrives. Returns `true` when data has been consumed.
    fn apply_pending_data(&mut self) -> bool {
        #[cfg(target_arch = "wasm32")]
        {
            let Some(json) = crate::roadwork_app::take_pending_opendata_data() else {
                return false;
            };
            if is_opendata_data(&json) {
                self.raw_json.clear();
                self.parsed_json = None;
                self.array_paths = Vec::new();
                self.set_opendata_data(json);
            } else {
                self.raw_json = serde_json::from_str::<serde_json::Value>(&json)
                    .map(|value| super::pretty_json_tabs(&value).unwrap_or(json.clone()))
                    .unwrap_or(json);
                self.parsed_json = None;
                self.array_paths = roadwork_core::json_tools::find_json_arrays(&self.raw_json);
                self.parsed_opendata = None;
                self.opendata_bytes = 0;
                self.opendata_count = 0;
            }
            self.result_json.clear();
            self.error = None;
            self.dirty = true;
            self.validation_report = None;
            true
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            false
        }
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
        if let Some(data) = &self.parsed_opendata {
            let data = serde_json::to_string(data).map_err(|e| e.to_string())?;
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

    #[cfg(not(target_arch = "wasm32"))]
    fn prepare_save(&self) -> Result<SavedOpendata, String> {
        let descriptor: OpendataServiceDescriptor =
            serde_json::from_str(&self.descriptor_json).map_err(|e| e.to_string())?;
        let data = self
            .parsed_opendata
            .as_ref()
            .map(|value| {
                serde_json::from_value::<OpendataData>(value.clone()).map_err(|e| e.to_string())
            })
            .transpose()?;
        Ok(SavedOpendata {
            name: descriptor.metadata.name.clone(),
            original_name: self.original_name.clone(),
            descriptor,
            data,
        })
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn take_saved(&mut self) -> Option<SavedOpendata> {
        self.saved.take()
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
                let ods = OpendataService::from(descriptor);
                if self.parsed_json.is_none() {
                    self.parsed_json = serde_json::from_str(&self.raw_json).ok().map(Arc::new);
                }
                let data = if let Some(value) = self.parsed_json.as_deref() {
                    self.element_count = ods.element_count_value(value);
                    ods.parse_value_preview(value, PREVIEW_MAX_ELEMENTS).ok()
                } else {
                    self.element_count = 0;
                    None
                };
                if self.element_count == 0 {
                    self.current_index = 0;
                } else {
                    self.current_index = self.current_index.min(self.element_count - 1);
                }
                self.update_result_json();
                self.parsed_opendata = data
                    .as_ref()
                    .and_then(|data| serde_json::to_value(data).ok());
                self.opendata_bytes = self
                    .parsed_opendata
                    .as_ref()
                    .and_then(|value| serde_json::to_string(value).ok())
                    .map(|json| json.len())
                    .unwrap_or(0);
                self.opendata_count = data.as_ref().map(|data| data.opendata.len()).unwrap_or(0);
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
        if self.element_count == 0 {
            self.result_json.clear();
            return;
        }
        let ods = OpendataService::from(descriptor);
        let array = if let Some(value) = self.parsed_json.as_deref() {
            ods.extract_value(value)
        } else {
            ods.extract_roadwork_array(&self.raw_json)
        };
        match array {
            Ok(array) => {
                let element = array
                    .as_array()
                    .and_then(|elements| elements.get(self.current_index))
                    .cloned()
                    .unwrap_or(array);
                self.result_json = super::pretty_json_tabs(&element).unwrap_or_default();
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

fn format_bytes(bytes: usize) -> String {
    const KB: f64 = 1024.0;
    if bytes < KB as usize {
        format!("{bytes} B")
    } else if (bytes as f64) < KB * KB {
        format!("{:.1} KB", bytes as f64 / KB)
    } else {
        format!("{:.1} MB", bytes as f64 / (KB * KB))
    }
}

#[cfg(target_arch = "wasm32")]
fn is_opendata_data(json: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .is_some_and(|value| value.get("opendata").is_some_and(|o| o.is_object()))
}

fn is_opendata_data_value(value: &serde_json::Value) -> bool {
    value.get("opendata").is_some_and(|o| o.is_object())
}
