use crate::database::RoadworkDb;
use crate::gui::about_dialog::AboutDialog;
use crate::gui::logs_panel::LogsPanel;
use crate::gui::metada_dialog::MetadataDialog;
use crate::gui::roadwork_marker::RoadworkMarker;
use crate::gui::status_panel::StatusPanel;
use crate::model::roadwork_data::RoadworkData;
use crate::opendata::bootstrap;
use crate::opendata::json::model::lat_lng::LatLng;
use crate::opendata::open_data_service_manager::OpenDataServiceManager;
use crate::settings::Settings;
use chrono::DateTime;
use eframe::epaint::text::TextWrapMode;
use eframe::{App, Frame, Storage};
use egui::text::LayoutJob;
use egui::{Button, Context, Label, Response, RichText, Ui};
use egui_notify::Toasts;
use log::info;
use std::sync::{Arc, Mutex};
use walkers::sources::OpenStreetMap;
use walkers::{HttpTiles, Map, MapMemory, Projector};

const DEFAULT_WME_URL: &str =
    "https://waze.com/fr/editor?env=row&lat=${lat}&&lon=${lon}&zoomLevel=19";

pub struct RoadworkApp {
    ctx: Context,
    db: Arc<RoadworkDb>,
    tiles: HttpTiles,
    map_memory: MapMemory,
    settings: Arc<Mutex<Settings>>,
    open_data_service_manager: Arc<Mutex<Option<OpenDataServiceManager>>>,
    position: LatLng,
    roadwork_data: Arc<Mutex<Option<RoadworkData>>>,
    selected_roadwork: Option<String>,
    logs_panel_open: bool,
    toasts: Toasts,
    show_about_dialog: bool,
    show_info_dialog: bool,
}

impl RoadworkApp {
    pub fn new(egui_ctx: Context, db: Arc<RoadworkDb>) -> Self {
        let settings: Arc<Mutex<Settings>> = Arc::new(Mutex::new(Settings::default()));
        let position = settings.lock().unwrap().map_center.unwrap_or_default();
        let roadwork_data: Arc<Mutex<Option<RoadworkData>>> = Arc::new(Mutex::new(None));
        let open_data_service_manager: Arc<Mutex<Option<OpenDataServiceManager>>> =
            Arc::new(Mutex::new(None));

        let app = Self {
            ctx: egui_ctx.clone(),
            db,
            tiles: HttpTiles::new(OpenStreetMap, egui_ctx.clone()),
            map_memory: Default::default(),
            open_data_service_manager,
            settings: Arc::clone(&settings),
            position,
            roadwork_data,
            selected_roadwork: None,
            logs_panel_open: false,
            toasts: Toasts::default(),
            show_about_dialog: false,
            show_info_dialog: false,
        };

        app.spawn_init();
        app
    }

    fn spawn_init(&self) {
        let db = Arc::clone(&self.db);
        let settings = Arc::clone(&self.settings);
        let roadwork_data = Arc::clone(&self.roadwork_data);
        let open_data_service_manager = Arc::clone(&self.open_data_service_manager);
        let ctx = self.ctx.clone();

        wasm_bindgen_futures::spawn_local(async move {
            let loaded_settings = db.load_settings().await;
            *settings.lock().unwrap() = loaded_settings;

            bootstrap::ensure_opendata_available(&db).await;

            let manager = OpenDataServiceManager::new(Arc::clone(&db), Arc::clone(&settings)).await;

            let saved_center = settings.lock().unwrap().map_center;
            let _pos = saved_center.unwrap_or_else(|| manager.get_center());
            let data = manager.get_data().await;

            *open_data_service_manager.lock().unwrap() = Some(manager);
            *roadwork_data.lock().unwrap() = data;

            ctx.request_repaint();
        });
    }

    fn reload_data(&self) {
        info!("reload data");
        let open_data_service_manager = Arc::clone(&self.open_data_service_manager);
        let roadwork_data = Arc::clone(&self.roadwork_data);
        let ctx = self.ctx.clone();

        wasm_bindgen_futures::spawn_local(async move {
            if let Some(manager) = open_data_service_manager.lock().unwrap().as_ref() {
                manager.delete_cache().await;
                let data = manager.get_data().await;
                *roadwork_data.lock().unwrap() = data;
            }
            ctx.request_repaint();
        });
    }

    fn load_data(&self) {
        info!("Loading data");
        let _settings = Arc::clone(&self.settings);
        let roadwork_data = Arc::clone(&self.roadwork_data);
        let open_data_service_manager = Arc::clone(&self.open_data_service_manager);
        let ctx = self.ctx.clone();

        wasm_bindgen_futures::spawn_local(async move {
            if let Some(manager) = open_data_service_manager.lock().unwrap().as_ref() {
                let data = manager.get_data().await;
                *roadwork_data.lock().unwrap() = data;
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
            if let Some(roadwork_data) = guard.as_mut() {
                if let Some(roadwork) = roadwork_data.roadworks.get_mut(&id) {
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
    }

    fn get_wme_url_pattern(&self) -> String {
        if let Some(manager) = self.open_data_service_manager.lock().unwrap().as_ref() {
            if let Some(opendataservice) = manager.get_opendata_service() {
                if let Some(editor_pattern) =
                    &opendataservice.service_descriptor.metadata.editor_pattern
                {
                    return editor_pattern.to_string();
                }
            }
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
                let opendata_service_name =
                    self.settings.lock().unwrap().opendata_service.to_string();
                egui::ComboBox::from_label("")
                    .selected_text(opendata_service_name)
                    .show_ui(ui, |ui| {
                        let services = self
                            .open_data_service_manager
                            .lock()
                            .unwrap()
                            .as_ref()
                            .map(|m| m.services().to_owned())
                            .unwrap_or_default();
                        for service in services {
                            let service_name = service.to_string();
                            if ui
                                .selectable_value(
                                    &mut self.settings.lock().unwrap().opendata_service,
                                    service.to_string(),
                                    service,
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
                ui.checkbox(
                    &mut self.settings.lock().unwrap().hide_expired,
                    "Hide expired",
                );
                LogsPanel::new(&mut self.logs_panel_open).show_button(ui);

                if ui.button("Info").clicked() {
                    self.show_info_dialog = true;
                }
            });
        });

        if self.show_info_dialog {
            if let Some(ods) = self
                .open_data_service_manager
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|m| m.get_opendata_service())
            {
                let md = &ods.service_descriptor.metadata;
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
                self.position.into(),
            )
            .zoom_gesture(true)
            .double_click_to_zoom(true)
            .zoom_with_ctrl(false);
            let response = ui.add(map);

            if let Some(center) = self.map_memory.detached() {
                self.position = center.into();
            }

            if let Some(roadwork_data) = self.roadwork_data.lock().unwrap().as_ref() {
                let projector =
                    Projector::new(response.rect, &self.map_memory, self.position.into());
                if response.clicked() {
                    self.selected_roadwork = None;
                }
                let hide_expired = self.settings.lock().unwrap().hide_expired;
                for (id, marker) in roadwork_data.roadworks.iter() {
                    if hide_expired && marker.is_expired() {
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
        info!("Saving data");

        {
            let mut settings = self.settings.lock().unwrap();
            settings.map_center = Some(self.position);
            let z = self.map_memory.zoom();
            settings.map_zoom = Some(z);
        }

        let db = Arc::clone(&self.db);
        let settings = Arc::clone(&self.settings);
        let roadwork_data = Arc::clone(&self.roadwork_data);

        wasm_bindgen_futures::spawn_local(async move {
            let s = settings.lock().unwrap().clone();
            db.save_settings(&s).await;

            if let Some(data) = roadwork_data.lock().unwrap().as_ref() {
                db.save_cache(&data.source, data).await;
            }
        });
    }

    fn persist_egui_memory(&self) -> bool {
        true
    }
}
