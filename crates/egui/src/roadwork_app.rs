use crate::gui::about_dialog::AboutDialog;
use crate::gui::db_explorer_dialog::DbExplorerDialog;
use crate::gui::metada_dialog::MetadataDialog;
use crate::gui::roadwork_marker::RoadworkMarker;
use crate::gui::service_helper_dialog::{DataType, ServiceHelperDialog};
use crate::gui::settings_dialog::SettingsDialog;
use crate::gui::status_panel::StatusPanel;
use crate::tools::{latlng_to_position, position_to_latlng};
use crate::waze_livemap::Waze;
use chrono::DateTime;
use eframe::epaint::text::TextWrapMode;
use eframe::{App, Frame, Storage};
use egui::text::LayoutJob;
use egui::{Button, Context, Label, Response, RichText, Ui};
use egui_notify::Toasts;
use log::info;
use roadwork_core::model::opendata_data::OpendataData;
use roadwork_core::model::roadwork_data::RoadworkData;
use roadwork_core::model::service_info::ServiceInfo;
use roadwork_core::opendata::json::model::lat_lng::LatLng;
use roadwork_core::opendata::json::model::metadata::Metadata;
use roadwork_core::opendata::json::model::service_descriptor::ServiceDescriptor;
use roadwork_core::settings::Settings;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use walkers::{HttpTiles, Map, MapMemory, Projector};

#[derive(Default)]
pub struct StartupParams {
    pub service: Option<String>,
    pub open_service_helper: bool,
    pub open_opendata_service_helper: bool,
    pub create_opendata_service: bool,
    /// Save the helper's descriptor as a roadwork service (custom local
    /// descriptor) instead of an opendata source.
    /// todo maybe change type
    pub save_as_roadwork: bool,
    pub opendata_descriptor: Option<String>,
    pub db_explorer_only: bool,
}

/// One custom opendata source as stored in the SQLite `cache` table, mirroring
/// `roadwork_db::OpendataSource` (the `enabled`/`visible` flags are ignored
/// here: the egui app reads/writes the descriptor, the extension owns the flags).
#[derive(serde::Deserialize)]
struct StoredOpendataSource {
    service: String,
    descriptor: String,
}

pub(crate) fn spawn_task<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

thread_local! {
    static PENDING_OPENDATA_DATA: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

pub(crate) fn take_pending_opendata_data() -> Option<String> {
    PENDING_OPENDATA_DATA.with(|cell| cell.borrow_mut().take())
}

/// Registers a `message` listener that receives `ROADWORK_HELPER_DATA` posted by the
/// WME extension content script, storing the data for the opendata helper dialog.
pub fn setup_helper_data_listener() {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::Closure;
    let window = web_sys::window().expect("No window");
    let win = window.clone();
    let closure = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        let window = win.clone();
        let Ok(obj) = event.data().dyn_into::<js_sys::Object>() else {
            return;
        };
        let type_value = js_sys::Reflect::get(&obj, &js_sys::JsString::from("type")).ok();
        if type_value.as_ref().and_then(|v| v.as_string()).as_deref()
            != Some("ROADWORK_HELPER_DATA")
        {
            return;
        }
        let data_value = js_sys::Reflect::get(&obj, &js_sys::JsString::from("data")).ok();
        if let Some(json) = data_value.and_then(|v| v.as_string()) {
            PENDING_OPENDATA_DATA.with(|cell| *cell.borrow_mut() = Some(json));
        }
        let ack = js_sys::Object::new();
        let _ = js_sys::Reflect::set(
            &ack,
            &js_sys::JsString::from("type"),
            &js_sys::JsString::from("ROADWORK_HELPER_DATA_ACK"),
        );
        if let Some(parent) = window.parent().ok().flatten() {
            let _ = parent.post_message(&ack, "*");
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);
    window
        .add_event_listener_with_callback("message", closure.as_ref().unchecked_ref())
        .expect("Failed to add message listener");
    closure.forget();
}

/// Tracks the "save to extension" flow progress, driven by `ROADWORK_SAVE_PROGRESS` /
/// `ROADWORK_SAVE_DONE` / `ROADWORK_SAVE_ERROR` messages posted by the WME extension
/// page-world script. Mirrors the `PENDING_OPENDATA_DATA` thread-local pattern.
#[derive(Clone)]
pub(crate) struct SaveProgressState {
    pub active: bool,
    pub stage: String,
    /// -1 means indeterminate.
    pub fraction: f32,
    pub done: Option<String>,
    pub error: Option<String>,
    /// Number of opendata records stored by the extension, when known.
    pub count: Option<u64>,
    pub last_update: f64,
}

impl SaveProgressState {
    fn idle() -> Self {
        Self {
            active: false,
            stage: String::new(),
            fraction: -1.0,
            done: None,
            error: None,
            count: None,
            last_update: js_sys::Date::now(),
        }
    }
}

thread_local! {
    static SAVE_PROGRESS: std::cell::RefCell<SaveProgressState> =
        std::cell::RefCell::new(SaveProgressState::idle());
}

thread_local! {
    /// Set to the saved service name by the `ROADWORK_SAVE_DONE` listener so the
    /// next frame reloads the opendata sources from the store.
    static PENDING_OPENDATA_REFRESH: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

pub(crate) fn take_pending_opendata_refresh() -> Option<String> {
    PENDING_OPENDATA_REFRESH.with(|cell| cell.borrow_mut().take())
}

pub(crate) fn start_save_progress() {
    SAVE_PROGRESS.with(|cell| {
        let mut state = cell.borrow_mut();
        state.active = true;
        state.stage = "Saving descriptor…".to_string();
        state.fraction = -1.0;
        state.done = None;
        state.error = None;
        state.count = None;
        state.last_update = js_sys::Date::now();
    });
}

pub(crate) fn update_save_progress(stage: &str, fraction: f32) {
    SAVE_PROGRESS.with(|cell| {
        let mut state = cell.borrow_mut();
        state.active = true;
        state.stage = stage.to_string();
        state.fraction = fraction;
        state.done = None;
        state.error = None;
        state.count = None;
        state.last_update = js_sys::Date::now();
    });
}

pub(crate) fn save_progress_state() -> SaveProgressState {
    SAVE_PROGRESS.with(|cell| cell.borrow().clone())
}

pub(crate) fn finish_save_progress() {
    SAVE_PROGRESS.with(|cell| *cell.borrow_mut() = SaveProgressState::idle());
}

/// Posts a `ROADWORK_CLOSE_HELPER` message to the parent window so the WME
/// extension hides the helper overlay.
pub(crate) fn close_helper_overlay() {
    let object = js_sys::Object::new();
    js_sys::Reflect::set(
        &object,
        &wasm_bindgen::JsValue::from_str("type"),
        &wasm_bindgen::JsValue::from_str("ROADWORK_CLOSE_HELPER"),
    )
    .ok();
    if let Some(window) = web_sys::window()
        && let Ok(Some(parent)) = window.parent()
    {
        let _ = parent.post_message(&object, "*");
    }
}

/// Registers a `message` listener that receives the save progress / result messages
/// posted by the WME extension page-world script during a "save to extension".
pub fn setup_save_progress_listener() {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::prelude::Closure;
    let window = web_sys::window().expect("No window");
    let closure = Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        let Ok(obj) = event.data().dyn_into::<js_sys::Object>() else {
            return;
        };
        let type_value = js_sys::Reflect::get(&obj, &js_sys::JsString::from("type")).ok();
        let Some(msg_type) = type_value.and_then(|v| v.as_string()) else {
            return;
        };
        match msg_type.as_str() {
            "ROADWORK_SAVE_PROGRESS" => {
                let stage = js_sys::Reflect::get(&obj, &js_sys::JsString::from("stage"))
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_default();
                let fraction = js_sys::Reflect::get(&obj, &js_sys::JsString::from("fraction"))
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(-1.0) as f32;
                update_save_progress(&stage, fraction);
            }
            "ROADWORK_SAVE_DONE" => {
                let name = js_sys::Reflect::get(&obj, &js_sys::JsString::from("name"))
                    .ok()
                    .and_then(|v| v.as_string());
                let count = js_sys::Reflect::get(&obj, &js_sys::JsString::from("count"))
                    .ok()
                    .and_then(|v| v.as_f64())
                    .map(|f| f as u64);
                let target = js_sys::Reflect::get(&obj, &js_sys::JsString::from("target"))
                    .ok()
                    .and_then(|v| v.as_string());
                SAVE_PROGRESS.with(|cell| {
                    let mut state = cell.borrow_mut();
                    state.active = false;
                    state.done = name.clone();
                    state.error = None;
                    state.count = count;
                    state.last_update = js_sys::Date::now();
                });
                if target.as_deref() != Some("roadwork")
                    && let Some(name) = name
                {
                    PENDING_OPENDATA_REFRESH.with(|cell| *cell.borrow_mut() = Some(name));
                }
            }
            "ROADWORK_SAVE_ERROR" => {
                let error = js_sys::Reflect::get(&obj, &js_sys::JsString::from("error"))
                    .ok()
                    .and_then(|v| v.as_string())
                    .unwrap_or_else(|| "Unknown save error".to_string());
                SAVE_PROGRESS.with(|cell| {
                    let mut state = cell.borrow_mut();
                    state.active = false;
                    state.done = None;
                    state.error = Some(error);
                    state.last_update = js_sys::Date::now();
                });
            }
            _ => {}
        }
    }) as Box<dyn FnMut(web_sys::MessageEvent)>);
    window
        .add_event_listener_with_callback("message", closure.as_ref().unchecked_ref())
        .expect("Failed to add save progress listener");
    closure.forget();
}

pub struct RoadworkApp {
    ctx: Context,
    tiles: HttpTiles,
    map_memory: MapMemory,
    position: LatLng,
    services: Vec<ServiceInfo>,
    selected_service: String,
    settings: Settings,
    roadwork_data: Arc<Mutex<Option<RoadworkData>>>,
    current_metadata: Arc<Mutex<Option<Metadata>>>,
    selected_roadwork: Option<String>,
    hide_expired: bool,
    toasts: Toasts,
    show_about_dialog: bool,
    show_settings_dialog: bool,
    show_info_dialog: bool,
    show_service_helper_dialog: bool,
    service_helper: ServiceHelperDialog,
    show_opendata_panel: bool,
    confirm_delete_opendata: Option<String>,
    opendata_data: Arc<Mutex<HashMap<String, OpendataData>>>,
    opendata_sources: Arc<Mutex<HashMap<String, ServiceDescriptor>>>,
    helper_only: bool,
    db_explorer_only: bool,
    show_db_explorer: bool,
    db_explorer: DbExplorerDialog,
}

impl RoadworkApp {
    pub fn new(egui_ctx: Context, params: StartupParams) -> Self {
        let settings = crate::app_settings::load_settings();
        let services = roadwork_service::get_services();
        let creating = params.create_opendata_service || params.opendata_descriptor.is_none();
        let original_name = if creating {
            String::new()
        } else {
            params.service.clone().unwrap_or_default()
        };
        let selected_service = if let Some(service) = params
            .service
            .filter(|s| services.iter().any(|srv| srv.name == *s))
        {
            service
        } else if services.iter().any(|s| s.name == settings.opendata_service) {
            settings.opendata_service.clone()
        } else {
            services.first().map(|s| s.name.clone()).unwrap_or_default()
        };
        let position = services
            .iter()
            .find(|s| s.name == selected_service)
            .map(|s| s.center)
            .unwrap_or_default();
        let mut service_helper = if params.opendata_descriptor.is_some()
            || params.create_opendata_service
            || params.open_opendata_service_helper
        {
            ServiceHelperDialog::for_custom(
                params.opendata_descriptor.clone(),
                original_name,
                creating,
            )
        } else {
            ServiceHelperDialog::for_builtin(&selected_service)
        };
        service_helper.data_type = match params.save_as_roadwork {
            true => DataType::Roadwork,
            false => DataType::Opendata,
        };
        let helper_only = params.open_service_helper
            || params.open_opendata_service_helper
            || params.save_as_roadwork;

        let app = Self {
            ctx: egui_ctx.clone(),
            tiles: HttpTiles::new(Waze, egui_ctx.clone()),
            map_memory: Default::default(),
            position,
            services,
            selected_service,
            settings,
            roadwork_data: Arc::new(Mutex::new(None)),
            current_metadata: Arc::new(Mutex::new(None)),
            selected_roadwork: None,
            hide_expired: false,
            toasts: Toasts::default(),
            show_about_dialog: false,
            show_settings_dialog: false,
            show_info_dialog: false,
            show_service_helper_dialog: params.open_service_helper
                || params.open_opendata_service_helper
                || params.save_as_roadwork,
            service_helper,
            show_opendata_panel: false,
            confirm_delete_opendata: None,
            opendata_data: Arc::new(Mutex::new(HashMap::new())),
            opendata_sources: Arc::new(Mutex::new(HashMap::new())),
            helper_only,
            db_explorer_only: params.db_explorer_only,
            show_db_explorer: false,
            db_explorer: DbExplorerDialog::default(),
        };

        if !helper_only && !params.db_explorer_only {
            app.load_data(false);
            app.reload_opendata_sources();
        }
        app
    }

    fn load_data(&self, force_refresh: bool) {
        self.load_roadworks(&self.selected_service, force_refresh);
    }

    fn load_roadworks(&self, service: &str, force_refresh: bool) {
        let roadwork_data = Arc::clone(&self.roadwork_data);
        let current_metadata = Arc::clone(&self.current_metadata);
        let ctx = self.ctx.clone();
        let service = service.to_string();
        let sync_config = crate::app_settings::sync_config(&self.settings);

        spawn_task(async move {
            let fetched = if force_refresh {
                crate::db_rpc::rpc_json::<RoadworkData>(
                    "get_roadworks",
                    vec![
                        wasm_bindgen::JsValue::from_str(&service),
                        wasm_bindgen::JsValue::from_bool(true),
                    ],
                )
                .await
            } else {
                match crate::db_rpc::rpc_json::<RoadworkData>(
                    "get_roadworks_cached",
                    vec![wasm_bindgen::JsValue::from_str(&service)],
                )
                .await
                {
                    Ok(data) => Ok(data),
                    Err(_) => {
                        crate::db_rpc::rpc_json::<RoadworkData>(
                            "get_roadworks",
                            vec![
                                wasm_bindgen::JsValue::from_str(&service),
                                wasm_bindgen::JsValue::from_bool(true),
                            ],
                        )
                        .await
                    }
                }
            };
            match fetched {
                Ok(mut data) => {
                    if let Some(descriptor) = roadwork_service::get_descriptor(&service) {
                        *current_metadata.lock().unwrap() = Some(descriptor.metadata);
                    }
                    if let Some(config) = &sync_config {
                        roadwork_service::synchronize(config, &mut data).await;
                    }
                    *roadwork_data.lock().unwrap() = Some(data);
                }
                Err(e) => log::error!("Failed to fetch roadworks for {service}: {e}"),
            }
            ctx.request_repaint();
        });
    }

    fn load_opendata(&self, name: &str, force_refresh: bool) {
        let Some(descriptor) = self.opendata_sources.lock().unwrap().get(name).cloned() else {
            return;
        };
        let has_url = descriptor
            .metadata
            .url
            .as_deref()
            .is_some_and(|url| !url.trim().is_empty());
        let name = name.to_string();
        let opendata_data = Arc::clone(&self.opendata_data);
        let ctx = self.ctx.clone();
        spawn_task(async move {
            let fetched = if has_url {
                crate::db_rpc::rpc_json::<OpendataData>(
                    "get_opendata",
                    vec![
                        wasm_bindgen::JsValue::from_str(&name),
                        wasm_bindgen::JsValue::from_bool(force_refresh),
                    ],
                )
                .await
            } else {
                crate::db_rpc::rpc_json::<OpendataData>(
                    "get_opendata_cached",
                    vec![wasm_bindgen::JsValue::from_str(&name)],
                )
                .await
            };
            match fetched {
                Ok(data) => {
                    opendata_data.lock().unwrap().insert(name.clone(), data);
                }
                Err(e) => log::error!("Failed to fetch opendata {name}: {e}"),
            }
            ctx.request_repaint();
        });
    }

    /// Loads the custom opendata sources from the store and preloads the cached
    /// data of each source so the source list shows the item counts without
    /// triggering any network fetch. Also called after a save to refresh the map.
    fn reload_opendata_sources(&self) {
        let opendata_sources = Arc::clone(&self.opendata_sources);
        let opendata_data = Arc::clone(&self.opendata_data);
        let ctx = self.ctx.clone();
        spawn_task(async move {
            let sources = match crate::db_rpc::rpc_json::<Vec<StoredOpendataSource>>(
                "get_opendata_sources",
                vec![],
            )
            .await
            {
                Ok(sources) => sources,
                Err(e) => {
                    log::error!("Failed to load opendata sources: {e}");
                    return;
                }
            };
            {
                let mut map = opendata_sources.lock().unwrap();
                map.clear();
                for source in sources {
                    if let Ok(descriptor) =
                        serde_json::from_str::<ServiceDescriptor>(&source.descriptor)
                    {
                        map.insert(source.service, descriptor);
                    }
                }
            }
            let names: Vec<String> = opendata_sources.lock().unwrap().keys().cloned().collect();
            for name in names {
                if let Ok(data) = crate::db_rpc::rpc_json::<OpendataData>(
                    "get_opendata_cached",
                    vec![wasm_bindgen::JsValue::from_str(&name)],
                )
                .await
                {
                    opendata_data.lock().unwrap().insert(name, data);
                }
            }
            ctx.request_repaint();
        });
    }

    fn get_multiline_text(text: &String) -> LayoutJob {
        let mut job = LayoutJob::single_section(
            text.to_owned(),
            egui::TextFormat {
                extra_letter_spacing: 0.,
                line_height: None,
                ..Default::default()
            },
        );
        job.wrap = egui::text::TextWrapping {
            max_rows: 6,
            break_anywhere: false,
            overflow_character: Some('…'),
            ..Default::default()
        };
        job
    }

    fn show_roadwork_detail(&mut self) {
        let selected_id = self.selected_roadwork.clone();
        let sync_config = crate::app_settings::sync_config(&self.settings);

        if let Some(id) = selected_id {
            let mut guard = self.roadwork_data.lock().unwrap();
            if let Some(roadwork_data) = guard.as_mut()
                && let Some(roadwork) = roadwork_data.roadworks.get_mut(&id)
            {
                let mut status_changed = false;
                let mut center_on: Option<LatLng> = None;
                egui::Window::new("Détail du chantier")
                    .id(egui::Id::new("roadwork_detail_window"))
                    .resizable(true)
                    .default_width(260.0)
                    .show(&self.ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Id:").strong());
                            ui.add(
                                Label::new(&roadwork.opendata.id).wrap_mode(TextWrapMode::Truncate),
                            );
                            if ui.button("Cibler").clicked() {
                                center_on = Some(LatLng {
                                    lat: roadwork.opendata.latitude,
                                    lon: roadwork.opendata.longitude,
                                });
                            }
                        });

                        egui::Grid::new("time_grid")
                            .num_columns(2)
                            .spacing([4.0, 4.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new("Start:").strong());
                                ui.label(
                                    DateTime::from_timestamp_millis(roadwork.start)
                                        .expect("Parsed start date")
                                        .format("%d/%m/%Y %H:%M")
                                        .to_string(),
                                );
                                ui.end_row();
                                ui.label(RichText::new("End:").strong());
                                ui.label(
                                    DateTime::from_timestamp_millis(roadwork.end)
                                        .expect("Parsed start date")
                                        .format("%d/%m/%Y %H:%M")
                                        .to_string(),
                                );
                                ui.end_row();
                            });

                        if let Some(text) = &roadwork.road {
                            ui.label(RichText::new("Road:").strong());
                            ui.add(Label::new(text).wrap_mode(TextWrapMode::Truncate));
                        }
                        if let Some(text) = &roadwork.location_details {
                            ui.label(RichText::new("Location details:").strong());
                            ui.label(Self::get_multiline_text(text));
                        }
                        if let Some(text) = &roadwork.impact_circulation_detail {
                            ui.label(RichText::new("Impact:").strong());
                            ui.label(Self::get_multiline_text(text));
                        }
                        if let Some(text) = &roadwork.opendata.description {
                            ui.label(RichText::new("Description:").strong());
                            ui.label(Self::get_multiline_text(text));
                        }

                        status_changed = StatusPanel::new(roadwork).show(ui);
                    });
                if status_changed {
                    let ctx = self.ctx.clone();
                    let data = Arc::clone(&self.roadwork_data);
                    spawn_task(async move {
                        if let Some(config) = sync_config {
                            let mut current = data.lock().unwrap().take();
                            if let Some(current) = current.as_mut() {
                                roadwork_service::synchronize(&config, current).await;
                            }
                            *data.lock().unwrap() = current;
                        }
                        ctx.request_repaint();
                    });
                }
                if let Some(ll) = center_on {
                    self.position = ll;
                    self.map_memory.center_at(latlng_to_position(ll));
                }
            }
        }
    }

    fn show_top_panel(&mut self, ui: &mut Ui) {
        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                let current_service = self.selected_service.clone();
                egui::ComboBox::from_label("")
                    .selected_text(&current_service)
                    .show_ui(ui, |ui| {
                        for service in &self.services {
                            let service_name = service.name.clone();
                            if ui
                                .selectable_value(
                                    &mut self.selected_service,
                                    service.name.clone(),
                                    service.name.clone(),
                                )
                                .changed()
                            {
                                self.position = service.center;
                                self.map_memory
                                    .center_at(latlng_to_position(service.center));
                                self.load_data(false);
                                self.toasts.success(format!("Switching to {service_name}"));
                            }
                        }
                    });
                if ui.button("Reload").clicked() {
                    self.load_data(true);
                }
                ui.checkbox(&mut self.hide_expired, "Hide expired");

                if ui.button("Info").clicked() {
                    self.show_info_dialog = true;
                }
                if ui.button("Settings").clicked() {
                    self.show_settings_dialog = true;
                }
                if ui.button("Service helper").clicked() {
                    self.show_service_helper_dialog = true;
                }
                if ui.button("Opendata").clicked() {
                    self.show_opendata_panel = true;
                }
                if ui.button("Explore DB").clicked() {
                    self.show_db_explorer = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let size = egui::vec2(18.0, 18.0);
                    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                    ui.painter()
                        .rect_filled(rect, 3.0, crate::tools::build_color());
                    if response.clicked() {
                        self.show_about_dialog = true;
                    }
                });
            });
        });

        if self.show_about_dialog {
            AboutDialog::new(&mut self.show_about_dialog).show(ui.ctx());
        }

        if self.show_info_dialog {
            if let Some(md) = self.current_metadata.lock().unwrap().as_ref() {
                MetadataDialog::new(&mut self.show_info_dialog, md).show(ui.ctx());
            } else {
                let screen = ui.ctx().content_rect().size();
                let max = egui::vec2(screen.x * 0.9, screen.y * 0.9);
                egui::Window::new("Source info")
                    .open(&mut self.show_info_dialog)
                    .max_size(max)
                    .show(ui.ctx(), |ui| {
                        ui.add(Label::new("No source selected").wrap_mode(TextWrapMode::Wrap));
                    });
            }
        }

        if self.show_settings_dialog {
            SettingsDialog::new(&mut self.show_settings_dialog, &mut self.settings).show(ui.ctx());
        }
        if self.show_db_explorer {
            self.db_explorer.show(ui.ctx(), &mut self.show_db_explorer);
        }
        {
            let custom_services = self.opendata_sources.lock().unwrap();
            self.service_helper.show(
                ui,
                &mut self.show_service_helper_dialog,
                &mut self.toasts,
                false,
                &custom_services,
            );
        }
        if self.show_opendata_panel {
            self.show_opendata_panel(ui.ctx());
        }
        self.show_opendata_delete_confirm(ui.ctx());
    }

    fn show_opendata_panel(&mut self, ctx: &Context) {
        let screen = ctx.content_rect().size();
        let default_size = egui::vec2(
            (screen.x * 0.45).clamp(360.0, 700.0),
            (screen.y * 0.8).clamp(300.0, 650.0),
        );
        let mut open = self.show_opendata_panel;
        egui::Window::new("Opendata")
            .open(&mut open)
            .resizable(true)
            .default_size(default_size)
            .show(ctx, |ui| {
                let mut names: Vec<String> = self
                    .opendata_sources
                    .lock()
                    .unwrap()
                    .keys()
                    .cloned()
                    .collect();
                names.sort();
                let current = self.settings.selected_opendata_service.clone();
                let selected_text = current
                    .clone()
                    .unwrap_or_else(|| "Aucune source".to_string());
                let mut selected = current.clone();
                let mut changed: Option<Option<String>> = None;
                egui::ComboBox::from_label("")
                    .selected_text(&selected_text)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(&mut selected, None::<String>, "Aucune source")
                            .changed()
                        {
                            changed = Some(None);
                        }
                        for name in &names {
                            let total = self
                                .opendata_data
                                .lock()
                                .unwrap()
                                .get(name)
                                .map(|data| data.opendata.len());
                            let label = match total {
                                Some(total) => format!("{name} ({total})"),
                                None => name.clone(),
                            };
                            if ui
                                .selectable_value(&mut selected, Some(name.clone()), label)
                                .changed()
                            {
                                changed = Some(Some(name.clone()));
                            }
                        }
                    });
                if let Some(selection) = changed {
                    self.settings.selected_opendata_service = selection.clone();
                    crate::app_settings::save_settings(&self.settings);
                    if let Some(name) = selection {
                        self.load_opendata(&name, false);
                    }
                }
                ui.horizontal(|ui| {
                    let has_selection = current.is_some();
                    if ui
                        .add_enabled(has_selection, Button::new("Edit"))
                        .on_hover_text("Open the opendata service helper to edit this source")
                        .clicked()
                        && let Some(name) = &current
                        && let Some(descriptor) = self.opendata_sources.lock().unwrap().get(name)
                        && let Ok(json) = crate::gui::pretty_json_tabs(descriptor)
                    {
                        self.service_helper =
                            ServiceHelperDialog::for_custom(Some(json), name.clone(), false);
                        self.show_service_helper_dialog = true;
                    }
                    if ui
                        .add_enabled(has_selection, Button::new("Delete"))
                        .on_hover_text("Delete this opendata source")
                        .clicked()
                        && let Some(name) = &current
                    {
                        self.confirm_delete_opendata = Some(name.clone());
                    }
                });
                ui.separator();
                let selected_name = self.settings.selected_opendata_service.clone();
                if let Some(name) = &selected_name {
                    let loaded = self
                        .opendata_data
                        .lock()
                        .unwrap()
                        .get(name)
                        .map(|data| data.opendata.len());
                    match loaded {
                        Some(total) => {
                            ui.label(RichText::new(format!("Data ({total}/{total})")).strong());
                            ui.separator();
                            egui::ScrollArea::vertical()
                                .id_salt("opendata_list_scroll")
                                .show(ui, |ui| {
                                    let mut items: Vec<_> = {
                                        let guard = self.opendata_data.lock().unwrap();
                                        guard
                                            .get(name)
                                            .map(|data| data.opendata.values().cloned().collect())
                                            .unwrap_or_default()
                                    };
                                    items.sort_by(|a, b| a.id.cmp(&b.id));
                                    egui::Grid::new("opendata_grid")
                                        .striped(true)
                                        .spacing([12.0, 4.0])
                                        .show(ui, |ui| {
                                            ui.label(RichText::new("ID").strong());
                                            ui.label(RichText::new("Description").strong());
                                            ui.label(RichText::new("Position").strong());
                                            ui.end_row();
                                            for item in items {
                                                ui.label(&item.id);
                                                ui.label(
                                                    item.description.clone().unwrap_or_default(),
                                                );
                                                if item.latitude != 0.0 && item.longitude != 0.0 {
                                                    ui.label(format!(
                                                        "{:.5}, {:.5}",
                                                        item.latitude, item.longitude
                                                    ));
                                                } else if item
                                                    .polygons
                                                    .as_ref()
                                                    .is_some_and(|p| !p.is_empty())
                                                {
                                                    ui.label("polygon");
                                                } else {
                                                    ui.label("-");
                                                }
                                                ui.end_row();
                                            }
                                        });
                                });
                        }
                        None => {
                            ui.label("Aucune donn\u{00e9}e opendata charg\u{00e9}e");
                            if ui.button("Reload").clicked() {
                                self.load_opendata(name, true);
                            }
                        }
                    }
                } else {
                    ui.label("S\u{00e9}lectionnez une source");
                }
            });
        self.show_opendata_panel = open;
    }

    fn show_opendata_delete_confirm(&mut self, ctx: &Context) {
        let Some(name) = self.confirm_delete_opendata.clone() else {
            return;
        };
        let mut confirmed = false;
        let mut cancelled = false;
        egui::Window::new("Delete source")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Delete the opendata source \"{name}\"?"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancelled = true;
                    }
                    if ui.button("Delete").clicked() {
                        confirmed = true;
                    }
                });
            });
        if confirmed {
            self.delete_opendata_service(&name);
        }
        if confirmed || cancelled {
            self.confirm_delete_opendata = None;
        }
    }

    fn delete_opendata_service(&mut self, name: &str) {
        self.opendata_sources.lock().unwrap().remove(name);
        self.opendata_data.lock().unwrap().remove(name);
        if self.settings.selected_opendata_service.as_deref() == Some(name) {
            self.settings.selected_opendata_service = None;
        }
        crate::app_settings::save_settings(&self.settings);
        Self::notify_extension_delete(name);
        self.toasts.success(format!("Source \"{name}\" deleted"));
    }

    /// Posts a `ROADWORK_DELETE_OPENDATA_SERVICE` message to the parent window so
    /// the WME extension removes the source from its own settings.
    fn notify_extension_delete(name: &str) {
        let object = js_sys::Object::new();
        js_sys::Reflect::set(
            &object,
            &wasm_bindgen::JsValue::from_str("type"),
            &wasm_bindgen::JsValue::from_str("ROADWORK_DELETE_OPENDATA_SERVICE"),
        )
        .ok();
        js_sys::Reflect::set(
            &object,
            &wasm_bindgen::JsValue::from_str("name"),
            &wasm_bindgen::JsValue::from_str(name),
        )
        .ok();
        if let Some(window) = web_sys::window()
            && let Ok(Some(parent)) = window.parent()
        {
            let _ = parent.post_message(&object, "*");
        }
    }

    fn draw_zoom_level(&mut self, ui: &mut Ui, response: Response) {
        let painter = ui.painter_at(response.rect);
        let margin = egui::vec2(6.0, 6.0);
        let padding = egui::vec2(6.0, 4.0);

        let text_color = ui.visuals().strong_text_color();

        let galley = ui.fonts_mut(|f| {
            f.layout_no_wrap(
                format!("Zoom: {:.2}", self.map_memory.zoom()),
                egui::TextStyle::Body.resolve(ui.style()),
                text_color,
            )
        });
        let rect_size = galley.size() + 2.0 * padding;

        let bottom_right = response.rect.right_bottom() - margin;
        let rect = egui::Rect::from_min_size(bottom_right - rect_size, rect_size);

        painter.rect_filled(
            rect,
            egui::CornerRadius::same(4u8),
            ui.visuals().code_bg_color,
        );

        let text_pos = rect.min + padding;
        painter.galley(text_pos, galley, text_color);
    }
}

impl App for RoadworkApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        if let Some(name) = take_pending_opendata_refresh() {
            self.reload_opendata_sources();
            self.load_opendata(&name, false);
        }
        let dropped_files: Vec<egui::DroppedFile> = ui.ctx().input(|i| i.raw.dropped_files.clone());
        if !dropped_files.is_empty() && !self.show_service_helper_dialog {
            self.toasts
                .info("Drop a data file to import it, after opening the service helper");
        }

        if self.db_explorer_only {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                self.db_explorer.show_fullscreen(ui);
            });
        } else if self.helper_only {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                if self.show_service_helper_dialog {
                    let custom_services = self.opendata_sources.lock().unwrap();
                    self.service_helper.show(
                        ui,
                        &mut self.show_service_helper_dialog,
                        &mut self.toasts,
                        true,
                        &custom_services,
                    );
                }
            });
            if !self.show_service_helper_dialog {
                close_helper_overlay();
            }
        } else {
            self.show_top_panel(ui);
            self.show_roadwork_detail();

            egui::CentralPanel::default().show_inside(ui, |ui| {
                let map = Map::new(
                    Some(&mut self.tiles),
                    &mut self.map_memory,
                    latlng_to_position(self.position),
                )
                .zoom_gesture(true)
                .double_click_to_zoom(true)
                .zoom_with_ctrl(false);
                let response = ui.add(map);

                if let Some(center) = self.map_memory.detached() {
                    self.position = position_to_latlng(center);
                }

                if let Some(roadwork_data) = self.roadwork_data.lock().unwrap().as_ref() {
                    let projector = Projector::new(
                        response.rect,
                        &self.map_memory,
                        latlng_to_position(self.position),
                    );
                    if response.clicked() {
                        self.selected_roadwork = None;
                    }
                    for (id, marker) in roadwork_data.roadworks.iter() {
                        if self.hide_expired && marker.is_expired() {
                            continue;
                        }
                        if ui
                            .add(RoadworkMarker::new(marker, &projector, response.clicked()))
                            .changed()
                        {
                            self.selected_roadwork = Some(id.to_owned());
                        }
                    }
                }

                self.draw_zoom_level(ui, response);
            });
        }
        self.toasts.show(ui.ctx());
    }

    fn save(&mut self, _storage: &mut dyn Storage) {
        info!("Saving data (frontend state is managed by backend)");
    }

    fn persist_egui_memory(&self) -> bool {
        true
    }
}
