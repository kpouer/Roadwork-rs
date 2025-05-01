use crate::message::Message;
use crate::model::roadwork::Roadwork;
use crate::model::roadwork_data::RoadworkData;
use crate::opendata::json::model::lat_lng::LatLng;
use crate::opendata::open_data_service_manager::OpenDataServiceManager;
use crate::settings::Settings;
use crossbeam_channel::{Receiver, Sender};
use eframe::{App, Frame};
use egui::Context;
use log::info;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use walkers::sources::OpenStreetMap;
use walkers::{HttpOptions, HttpTiles, Map, MapMemory};

pub struct RoadworkApp {
    hermes_sender: Sender<Message>,
    hermes_receiver: Receiver<Message>,
    tiles: HttpTiles,
    map_memory: MapMemory,
    settings: Arc<Mutex<Settings>>,
    open_data_service_manager: OpenDataServiceManager,
    roadwork: Roadwork,
    position: LatLng,
}

impl RoadworkApp {
    pub fn new(egui_ctx: Context) -> Self {
        let (hermes_sender, hermes_receiver) = crossbeam_channel::bounded::<Message>(5);
        let settings = Default::default();
        let mut http_options = HttpOptions::default();
        http_options.cache = Some(PathBuf::from("cache"));
        Self {
            hermes_sender,
            hermes_receiver,
            tiles: HttpTiles::with_options(OpenStreetMap, http_options, egui_ctx),
            map_memory: Default::default(),
            open_data_service_manager: OpenDataServiceManager::new(Arc::clone(&settings)),
            settings,
            roadwork: Default::default(),
            position: Default::default(),
        }
    }

    fn reload_data(&mut self) {
        info!("reload data");
        self.open_data_service_manager.deleteCache();
        self.load_data();
    }

    pub(crate) fn load_data(&mut self) {
        info!("Loading data");
        self.position = self.open_data_service_manager.get_center();
        // mapView.removeAllMarkers();
        if let Some(data) = self.open_data_service_manager.get_data() {
            self.set_roadwork_data(data);
        }
    }

    fn set_roadwork_data(&self, roadworkData: RoadworkData) {
        // softwareModel.setRoadworkData(roadworkData);
        roadworkData
            .roadworks
            .values()
            .for_each(|roadwork| self.addMarker(&roadwork));
        // mapView.fitToMarkers();
    }

    fn addMarker(&self, roadwork: &Roadwork) {
        //todo : implement
        // var mouseAdapter = new MouseAdapter() {
        //     @Override
        //     public void mouseClicked(MouseEvent e) {
        //         softwareModel.setRoadwork(roadwork);
        //         detailPanel.setRoadwork(roadwork);
        //     }
        // };
        // var markers = roadwork.getMarker();
        // Arrays.stream(markers).forEach(marker -> {
        //             marker.addMouseListener(mouseAdapter);
        //             mapView.addMarker(marker);
        //         }
        // );
    }
}

impl App for RoadworkApp {
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
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
                if ui.button("Logs Panel").clicked() {
                    todo!()
                }
            });
        });

        egui::SidePanel::left("left_panel").show(ctx, |ui| {
            egui::Grid::new("my_grid")
                .num_columns(2)
                .spacing([40.0, 4.0])
                .striped(true)
                .show(ui, |ui| {
                    ui.label("Id:");
                    ui.label(&self.roadwork.id);
                });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let map = Map::new(
                Some(&mut self.tiles),
                &mut self.map_memory,
                self.position.into(),
            );
            ui.add(map);
        });
    }
}
