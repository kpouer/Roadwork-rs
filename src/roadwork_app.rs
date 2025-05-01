use crate::gui::roadwork_marker::RoadworkMarker;
use crate::model::roadwork_data::RoadworkData;
use crate::opendata::json::model::lat_lng::LatLng;
use crate::opendata::open_data_service_manager::OpenDataServiceManager;
use crate::settings::Settings;
use eframe::epaint::text::TextWrapMode;
use eframe::{App, Frame, Storage};
use egui::text::LayoutJob;
use egui::{Button, Context, Label, RichText};
use log::info;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use walkers::sources::OpenStreetMap;
use walkers::{HttpOptions, HttpTiles, Map, MapMemory, Projector};
use crate::gui::logs_panel::LogsPanel;
use crate::gui::status_panel::StatusPanel;

const DEFAULT_WME_URL: &str =
    "https://waze.com/fr/editor?env=row&lat=${lat}&&lon=${lon}&zoomLevel=19";

pub struct RoadworkApp {
    tiles: HttpTiles,
    map_memory: MapMemory,
    settings: Arc<Mutex<Settings>>,
    open_data_service_manager: OpenDataServiceManager,
    position: LatLng,
    roadwork_data: Option<RoadworkData>,
    selected_roadwork: Option<String>,
    logs_panel_open: bool,
}

impl RoadworkApp {
    pub fn new(egui_ctx: Context) -> Self {
        let settings = Default::default();
        let mut http_options = HttpOptions::default();
        http_options.cache = Settings::settings_folder().map(|mut settings_folder| {
            settings_folder.push("cache");
            settings_folder
        });
        Self {
            tiles: HttpTiles::with_options(OpenStreetMap, http_options, egui_ctx),
            map_memory: Default::default(),
            open_data_service_manager: OpenDataServiceManager::new(Arc::clone(&settings)),
            settings,
            position: Default::default(),
            roadwork_data: None,
            selected_roadwork: None,
            logs_panel_open: false,
        }
    }

    fn reload_data(&mut self) {
        info!("reload data");
        self.open_data_service_manager.delete_cache();
        self.load_data();
    }

    pub fn load_data(&mut self) {
        info!("Loading data");
        self.position = self.open_data_service_manager.get_center();
        self.roadwork_data = self.open_data_service_manager.get_data();
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

    fn show_left_panel(&mut self, ctx: &Context) {
        let url = self.get_wme_url_pattern();

        if let Some((id, roadwork_data)) = self
            .selected_roadwork
            .as_ref()
            .zip(self.roadwork_data.as_mut())
        {
            let roadwork = roadwork_data
                .roadworks
                .get_mut(id)
                .expect("roadwork not found");
            egui::SidePanel::left("left_panel").show(ctx, |ui| {
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
                            let url = url.replace("${lat}", &format!("{}", roadwork.latitude))
                                .replace("${lon}", &format!("{}", roadwork.longitude));
                            open::that(url).expect("failed to open url");

                        }
                    });

                    egui::Grid::new("time_grid")
                        .num_columns(2)
                        .spacing([4.0, 4.0])
                        .show(ui, |ui| {
                            ui.label(RichText::new("Start:").strong());
                            ui.add(
                                Label::new(roadwork.start.to_string())
                                    .wrap_mode(TextWrapMode::Truncate),
                            );
                            ui.end_row();
                            ui.label(RichText::new("End:").strong());
                            ui.add(
                                Label::new(roadwork.end.to_string())
                                    .wrap_mode(TextWrapMode::Truncate),
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
                        open::that(&roadwork.url).expect("failed to open url");
                    }
                    
                    StatusPanel::new(roadwork).show(ui);
                });
            });
        }
    }

    fn get_wme_url_pattern(&self) -> String {
        if let Some(opendataservice) = self.open_data_service_manager.get_opendata_service() {
            if let Some(editor_pattern) =
                &opendataservice.service_descriptor.metadata.editor_pattern
            {
                editor_pattern.to_string()
            } else {
                DEFAULT_WME_URL.to_string()
            }
        } else {
            DEFAULT_WME_URL.to_string()
        }
    }

    fn show_top_panel(&mut self, ctx: &Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_label("")
                    .selected_text(format!("{}", self.settings.lock().unwrap().opendataService))
                    .show_ui(ui, |ui| {
                        // todo : remove this ugly clone
                        let services = self.open_data_service_manager.services().to_owned();
                        for service in services {
                            if ui
                                .selectable_value(
                                    &mut self.settings.lock().unwrap().opendataService,
                                    service.to_string(),
                                    service,
                                )
                                .clicked()
                            {
                                self.load_data();
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
                LogsPanel::new(&mut self.logs_panel_open).show_button(ctx, ui);
            });
        });
    }
}

impl App for RoadworkApp {
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        self.show_top_panel(ctx);
        self.show_left_panel(ctx);

        egui::CentralPanel::default().show(ctx, |ui| {
            let map = Map::new(
                Some(&mut self.tiles),
                &mut self.map_memory,
                self.position.into(),
            )
            .zoom_gesture(true)
            .double_click_to_zoom(true)
            .zoom_with_ctrl(false);
            let response = ui.add(map);
            if let Some(roadwork_data) = &self.roadwork_data {
                let projector =
                    Projector::new(response.rect, &self.map_memory, self.position.into());
                if response.clicked() {
                    self.selected_roadwork = None;
                }
                for (id, marker) in roadwork_data.roadworks.iter() {
                    if ui
                        .add(RoadworkMarker::new(marker, &projector, response.clicked()))
                        .changed()
                    {
                        self.selected_roadwork = Some(id.to_owned());
                    }
                }
            }
        });
        if ctx.input(|i| i.viewport().close_requested()) {
            if let Some(roadwork_data) = &self.roadwork_data {
                self.open_data_service_manager.save(&roadwork_data);
            }
            let settings = self.settings.lock().unwrap();
            settings.save().expect("Unable to save settings");
        }
    }

    fn save(&mut self, _storage: &mut dyn Storage) {
        info!("Saving data");
        if let Some(roadwork_data) = &self.roadwork_data {
            self.open_data_service_manager.save(&roadwork_data);
        }
        let settings = self.settings.lock().unwrap();
        settings.save().expect("Unable to save settings");
    }
}
