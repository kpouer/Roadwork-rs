use crate::convert::{latlng_to_position, position_to_latlng};
use crate::gui::about_dialog::AboutDialog;
use crate::gui::metada_dialog::MetadataDialog;
use crate::gui::opendata_service_helper_dialog::OpendataServiceHelperDialog;
use crate::gui::roadwork_marker::RoadworkMarker;
use crate::gui::service_helper_dialog::ServiceHelperDialog;
use crate::gui::settings_dialog::SettingsDialog;
use crate::gui::status_panel::StatusPanel;
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
use roadwork_core::opendata::json::opendata_service::OpendataService;
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
    pub opendata_descriptor: Option<String>,
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn spawn_task<F>(future: F)
where
    F: std::future::Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static PENDING_OPENDATA_DATA: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn take_pending_opendata_data() -> Option<String> {
    PENDING_OPENDATA_DATA.with(|cell| cell.borrow_mut().take())
}

/// Registers a `message` listener that receives `ROADWORK_HELPER_DATA` posted by the
/// WME extension content script, storing the data for the opendata helper dialog.
#[cfg(target_arch = "wasm32")]
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

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn spawn_task<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    std::thread::spawn(move || pollster::block_on(future));
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
    show_opendata_service_helper_dialog: bool,
    opendata_service_helper: OpendataServiceHelperDialog,
    show_opendata_panel: bool,
    confirm_delete_opendata: Option<String>,
    opendata_data: Arc<Mutex<HashMap<String, OpendataData>>>,
    helper_only: bool,
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
        let service_helper = ServiceHelperDialog::new(&selected_service);
        let opendata_service_helper = OpendataServiceHelperDialog::new(
            params.opendata_descriptor.clone(),
            original_name,
            creating,
        );
        let helper_only = params.open_service_helper || params.open_opendata_service_helper;

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
            show_service_helper_dialog: params.open_service_helper,
            service_helper,
            show_opendata_service_helper_dialog: params.open_opendata_service_helper,
            opendata_service_helper,
            show_opendata_panel: false,
            confirm_delete_opendata: None,
            opendata_data: Arc::new(Mutex::new(crate::app_settings::load_opendata_cache())),
            helper_only,
        };

        if !helper_only {
            app.load_data();
        }
        app
    }

    fn load_data(&self) {
        self.load_roadworks(&self.selected_service);
    }

    fn load_roadworks(&self, service: &str) {
        let roadwork_data = Arc::clone(&self.roadwork_data);
        let current_metadata = Arc::clone(&self.current_metadata);
        let ctx = self.ctx.clone();
        let service = service.to_string();
        let sync_config = crate::app_settings::sync_config(&self.settings);

        spawn_task(async move {
            match roadwork_service::get_roadworks(&service).await {
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

    fn load_opendata(&self, name: &str) {
        let Some(descriptor) = self.settings.opendata_services.get(name).cloned() else {
            return;
        };
        let has_url = descriptor
            .metadata
            .url
            .as_deref()
            .is_some_and(|url| !url.trim().is_empty());
        let name = name.to_string();
        if !has_url {
            if let Some(data) = crate::app_settings::load_opendata_cache().remove(&name) {
                self.opendata_data.lock().unwrap().insert(name, data);
            }
            return;
        }
        let opendata_data = Arc::clone(&self.opendata_data);
        let ctx = self.ctx.clone();
        spawn_task(async move {
            let service = OpendataService {
                service_name: name.clone(),
                service_descriptor: descriptor,
            };
            match service.get_data().await {
                Ok(data) => {
                    crate::app_settings::save_opendata_cache(&name, &data);
                    opendata_data.lock().unwrap().insert(name.clone(), data);
                }
                Err(e) => log::error!("Failed to fetch opendata {name}: {e}"),
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

    fn open_url(url: &str) {
        crate::open_url::open_url(url);
    }

    fn show_left_panel(&mut self, ui: &mut Ui) {
        let selected_id = self.selected_roadwork.clone();
        let sync_config = crate::app_settings::sync_config(&self.settings);

        if let Some(id) = selected_id {
            let mut guard = self.roadwork_data.lock().unwrap();
            if let Some(roadwork_data) = guard.as_mut()
                && let Some(roadwork) = roadwork_data.roadworks.get_mut(&id)
            {
                let mut status_changed = false;
                egui::Panel::left("left_panel").show_inside(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Id:").strong());
                        ui.add(Label::new(&roadwork.opendata.id).wrap_mode(TextWrapMode::Truncate));
                        ui.horizontal(|ui| {
                            egui::Grid::new("loc_grid")
                                .num_columns(2)
                                .spacing([4.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label(RichText::new("Latitude:").strong());
                                    ui.add(
                                        Label::new(roadwork.opendata.latitude.to_string())
                                            .wrap_mode(TextWrapMode::Truncate),
                                    );
                                    ui.end_row();
                                    ui.label(RichText::new("Longitude:").strong());
                                    ui.add(
                                        Label::new(roadwork.opendata.longitude.to_string())
                                            .wrap_mode(TextWrapMode::Truncate),
                                    );
                                    ui.end_row();
                                });
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

                        if ui
                            .add_enabled(!roadwork.url.is_empty(), Button::new("Open URL"))
                            .clicked()
                        {
                            Self::open_url(&roadwork.url);
                        }

                        status_changed = StatusPanel::new(roadwork).show(ui);
                    });
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
                                self.load_data();
                                self.toasts.success(format!("Switching to {service_name}"));
                            }
                        }
                    });
                if ui.button("Reload").clicked() {
                    self.load_data();
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
                if ui.button("Opendata service helper").clicked() {
                    self.show_opendata_service_helper_dialog = true;
                }
                if ui.button("Opendata").clicked() {
                    self.show_opendata_panel = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let size = egui::vec2(18.0, 18.0);
                    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
                    ui.painter()
                        .rect_filled(rect, 3.0, crate::build_color::build_color());
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
        self.service_helper.show(
            ui,
            &mut self.show_service_helper_dialog,
            &self.selected_service,
            &mut self.toasts,
            false,
        );
        self.opendata_service_helper.show(
            ui,
            &mut self.show_opendata_service_helper_dialog,
            &mut self.toasts,
            false,
        );
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(saved) = self.opendata_service_helper.take_saved() {
            if !saved.original_name.is_empty() && saved.original_name != saved.name {
                self.settings.opendata_services.remove(&saved.original_name);
                self.opendata_data
                    .lock()
                    .unwrap()
                    .remove(&saved.original_name);
                if self.settings.selected_opendata_service.as_deref()
                    == Some(saved.original_name.as_str())
                {
                    self.settings.selected_opendata_service = Some(saved.name.clone());
                }
            }
            self.settings
                .opendata_services
                .insert(saved.name.clone(), saved.descriptor);
            crate::app_settings::save_settings(&self.settings);
            if let Some(data) = saved.data {
                crate::app_settings::save_opendata_cache(&saved.name, &data);
                self.opendata_data
                    .lock()
                    .unwrap()
                    .insert(saved.name.clone(), data);
            }
            self.toasts
                .success(format!("Service \"{}\" saved", saved.name));
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
                let mut names: Vec<String> =
                    self.settings.opendata_services.keys().cloned().collect();
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
                        self.load_opendata(&name);
                    }
                }
                ui.horizontal(|ui| {
                    let has_selection = current.is_some();
                    if ui
                        .add_enabled(has_selection, Button::new("Edit"))
                        .on_hover_text("Open the opendata service helper to edit this source")
                        .clicked()
                        && let Some(name) = &current
                        && let Some(descriptor) = self.settings.opendata_services.get(name)
                        && let Ok(json) = crate::gui::pretty_json_tabs(descriptor)
                    {
                        self.opendata_service_helper =
                            OpendataServiceHelperDialog::new(Some(json), name.clone(), false);
                        self.show_opendata_service_helper_dialog = true;
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
                                self.load_opendata(name);
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
        self.settings.opendata_services.remove(name);
        crate::app_settings::remove_opendata_cache(name);
        self.opendata_data.lock().unwrap().remove(name);
        if self.settings.selected_opendata_service.as_deref() == Some(name) {
            self.settings.selected_opendata_service = None;
        }
        crate::app_settings::save_settings(&self.settings);
        #[cfg(target_arch = "wasm32")]
        Self::notify_extension_delete(name);
        self.toasts.success(format!("Source \"{name}\" deleted"));
    }

    /// Posts a `ROADWORK_DELETE_OPENDATA_SERVICE` message to the parent window so
    /// the WME extension removes the source from its own settings.
    #[cfg(target_arch = "wasm32")]
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
        let dropped_files: Vec<egui::DroppedFile> = ui.ctx().input(|i| i.raw.dropped_files.clone());
        if !dropped_files.is_empty() && !self.show_opendata_service_helper_dialog {
            self.toasts
                .info("Drop a data file to import it, after opening the opendata service helper");
        }

        if self.helper_only {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                if self.show_service_helper_dialog {
                    self.service_helper.show(
                        ui,
                        &mut self.show_service_helper_dialog,
                        &self.selected_service,
                        &mut self.toasts,
                        true,
                    );
                } else if self.show_opendata_service_helper_dialog {
                    self.opendata_service_helper.show(
                        ui,
                        &mut self.show_opendata_service_helper_dialog,
                        &mut self.toasts,
                        true,
                    );
                }
            });
        } else {
            self.show_top_panel(ui);
            self.show_left_panel(ui);

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
