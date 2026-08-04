use crate::convert::{latlng_to_position, position_to_latlng};
use crate::gui::about_dialog::AboutDialog;
use crate::gui::metada_dialog::MetadataDialog;
use crate::gui::roadwork_marker::RoadworkMarker;
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
use roadwork_core::opendata::json::model::lat_lng::LatLng;
use roadwork_core::opendata::json::model::metadata::Metadata;
use std::sync::{Arc, Mutex};
use walkers::{HttpTiles, Map, MapMemory, Projector};

const DEFAULT_WME_URL: &str =
    "https://waze.com/fr/editor?env=row&lat=${lat}&&lon=${lon}&zoomLevel=19";
const API_BASE: &str = "/api";

pub struct RoadworkApp {
    ctx: Context,
    tiles: HttpTiles,
    map_memory: MapMemory,
    position: LatLng,
    services: Vec<String>,
    selected_service: String,
    roadwork_data: Arc<Mutex<Option<RoadworkData>>>,
    current_metadata: Arc<Mutex<Option<Metadata>>>,
    selected_roadwork: Option<String>,
    hide_expired: bool,
    logs_panel_open: bool,
    toasts: Toasts,
    show_about_dialog: bool,
    show_info_dialog: bool,
}

impl RoadworkApp {
    pub fn new(egui_ctx: Context) -> Self {
        let position = LatLng::default();

        let app = Self {
            ctx: egui_ctx.clone(),
            tiles: HttpTiles::new(Waze, egui_ctx.clone()),
            map_memory: Default::default(),
            position,
            services: Vec::new(),
            selected_service: "France-Paris".to_string(),
            roadwork_data: Arc::new(Mutex::new(None)),
            current_metadata: Arc::new(Mutex::new(None)),
            selected_roadwork: None,
            hide_expired: false,
            logs_panel_open: false,
            toasts: Toasts::default(),
            show_about_dialog: false,
            show_info_dialog: false,
        };

        app.spawn_init();
        app
    }

    fn spawn_init(&self) {
        let roadwork_data = Arc::clone(&self.roadwork_data);
        let ctx = self.ctx.clone();
        let selected_service = self.selected_service.clone();

        wasm_bindgen_futures::spawn_local(async move {
            let fetched_services = fetch_services().await.unwrap_or_default();

            if !fetched_services.is_empty() {
                let service = if fetched_services.contains(&selected_service) {
                    selected_service
                } else {
                    fetched_services[0].clone()
                };

                match fetch_roadworks(&service).await {
                    Ok(data) => {
                        *roadwork_data.lock().unwrap() = Some(data);
                    }
                    Err(e) => log::error!("Failed to fetch roadworks: {e}"),
                }
            }

            ctx.request_repaint();
        });
    }

    fn reload_data(&self) {
        let roadwork_data = Arc::clone(&self.roadwork_data);
        let ctx = self.ctx.clone();
        let service = self.selected_service.clone();

        wasm_bindgen_futures::spawn_local(async move {
            match refresh_roadworks(&service).await {
                Ok(data) => {
                    *roadwork_data.lock().unwrap() = Some(data);
                }
                Err(e) => log::error!("Failed to refresh roadworks: {e}"),
            }
            ctx.request_repaint();
        });
    }

    fn load_data(&self) {
        let roadwork_data = Arc::clone(&self.roadwork_data);
        let ctx = self.ctx.clone();
        let service = self.selected_service.clone();

        wasm_bindgen_futures::spawn_local(async move {
            match fetch_roadworks(&service).await {
                Ok(data) => {
                    *roadwork_data.lock().unwrap() = Some(data);
                }
                Err(e) => log::error!("Failed to fetch roadworks: {e}"),
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
        web_sys::window()
            .unwrap()
            .open_with_url_and_target(url, "_blank")
            .ok();
    }

    fn show_left_panel(&mut self, ui: &mut Ui) {
        let url = self.get_wme_url_pattern();
        let selected_id = self.selected_roadwork.clone();

        if let Some(id) = selected_id {
            let mut guard = self.roadwork_data.lock().unwrap();
            if let Some(roadwork_data) = guard.as_mut()
                && let Some(roadwork) = roadwork_data.roadworks.get_mut(&id)
            {
                egui::Panel::left("left_panel").show_inside(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Id:").strong());
                        ui.add(Label::new(&roadwork.id).wrap_mode(TextWrapMode::Truncate));
                        ui.horizontal(|ui| {
                            egui::Grid::new("loc_grid")
                                .num_columns(2)
                                .spacing([4.0, 4.0])
                                .show(ui, |ui| {
                                    ui.label(RichText::new("Latitude:").strong());
                                    ui.add(
                                        Label::new(roadwork.latitude.to_string())
                                            .wrap_mode(TextWrapMode::Truncate),
                                    );
                                    ui.end_row();
                                    ui.label(RichText::new("Longitude:").strong());
                                    ui.add(
                                        Label::new(roadwork.longitude.to_string())
                                            .wrap_mode(TextWrapMode::Truncate),
                                    );
                                    ui.end_row();
                                });
                            if ui.button("WME").clicked() {
                                let url = url
                                    .replace("${lat}", &format!("{}", roadwork.latitude))
                                    .replace("${lon}", &format!("{}", roadwork.longitude));
                                Self::open_url(&url);
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
                        if let Some(text) = &roadwork.description {
                            ui.label(RichText::new("Description:").strong());
                            ui.label(Self::get_multiline_text(text));
                        }

                        if ui
                            .add_enabled(!roadwork.url.is_empty(), Button::new("Open URL"))
                            .clicked()
                        {
                            Self::open_url(&roadwork.url);
                        }

                        StatusPanel::new(roadwork).show(ui);
                    });
                });
            }
        }
    }

    fn get_wme_url_pattern(&self) -> String {
        if let Some(metadata) = self.current_metadata.lock().unwrap().as_ref()
            && let Some(editor_pattern) = &metadata.editor_pattern
        {
            return editor_pattern.to_string();
        }
        DEFAULT_WME_URL.to_string()
    }

    fn show_top_panel(&mut self, ui: &mut Ui) {
        egui::Panel::top("menu_bar").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("Help", |ui| {
                    if ui.button("About").clicked() {
                        self.show_about_dialog = true;
                        ui.close();
                    }
                });
            });
            if self.show_about_dialog {
                AboutDialog::new(&mut self.show_about_dialog).show(ui);
            }
        });
        egui::Panel::top("top_panel").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                let current_service = self.selected_service.clone();
                egui::ComboBox::from_label("")
                    .selected_text(&current_service)
                    .show_ui(ui, |ui| {
                        for service in &self.services {
                            let service_name = service.clone();
                            if ui
                                .selectable_value(
                                    &mut self.selected_service,
                                    service.clone(),
                                    service.clone(),
                                )
                                .changed()
                            {
                                self.load_data();
                                self.toasts.success(format!("Switching to {service_name}"));
                            }
                        }
                    });
                if ui.button("Reload").clicked() {
                    self.reload_data();
                }
                ui.checkbox(&mut self.hide_expired, "Hide expired");

                if ui.button("Info").clicked() {
                    self.show_info_dialog = true;
                }
            });
        });

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

// --- API Client functions ---

async fn fetch_services() -> Result<Vec<String>, reqwest::Error> {
    let url = format!("{API_BASE}/services");
    let resp = reqwest::get(&url).await?.json::<Vec<String>>().await?;
    Ok(resp)
}

async fn fetch_roadworks(service: &str) -> Result<RoadworkData, reqwest::Error> {
    let url = format!("{API_BASE}/roadworks?service={service}");
    let resp = reqwest::get(&url).await?.json::<RoadworkData>().await?;
    Ok(resp)
}

async fn refresh_roadworks(service: &str) -> Result<RoadworkData, reqwest::Error> {
    let url = format!("{API_BASE}/roadworks/refresh?service={service}");
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .send()
        .await?
        .json::<RoadworkData>()
        .await?;
    Ok(resp)
}
