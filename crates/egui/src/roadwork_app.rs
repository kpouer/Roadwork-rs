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
use roadwork_core::model::roadwork_data::RoadworkData;
use roadwork_core::model::service_info::ServiceInfo;
use roadwork_core::opendata::json::model::lat_lng::LatLng;
use roadwork_core::opendata::json::model::metadata::Metadata;
use roadwork_core::settings::Settings;
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
        };

        app.load_data();
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
            ui.ctx(),
            &mut self.show_service_helper_dialog,
            &self.selected_service,
            &mut self.toasts,
        );
        self.opendata_service_helper.show(
            ui.ctx(),
            &mut self.show_opendata_service_helper_dialog,
            &mut self.toasts,
        );
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
        for file in dropped_files {
            if self.show_opendata_service_helper_dialog {
                match self.opendata_service_helper.handle_dropped_file(file) {
                    Ok(name) => {
                        self.toasts.success(format!("Data imported from {name}"));
                    }
                    Err(e) => {
                        self.toasts.error(format!("Import failed: {e}"));
                    }
                }
            } else {
                self.toasts.info(
                    "Drop a data file to import it, after opening the opendata service helper",
                );
            }
        }

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
        self.toasts.show(ui.ctx());
    }

    fn save(&mut self, _storage: &mut dyn Storage) {
        info!("Saving data (frontend state is managed by backend)");
    }

    fn persist_egui_memory(&self) -> bool {
        true
    }
}
