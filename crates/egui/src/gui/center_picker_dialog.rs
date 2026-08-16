use crate::tools::{latlng_to_position, position_to_latlng};
use crate::waze_livemap::Waze;
use egui::{Color32, Context, DragValue, RichText, Stroke};
use roadwork_core::opendata::json::model::lat_lng::LatLng;
use walkers::{HttpTiles, Map, MapMemory, Tiles};

#[derive(Default)]
pub(crate) struct CenterPickerDialog {
    map_memory: MapMemory,
    tiles: Option<HttpTiles>,
    selected: LatLng,
}

impl CenterPickerDialog {
    pub(crate) fn open(&mut self, center: LatLng) {
        self.selected = center;
        self.map_memory.center_at(latlng_to_position(center));
        self.map_memory.set_zoom(12.0).ok();
    }

    /// Shows the picker window. Returns the selected center when the user clicks OK.
    pub(crate) fn show(&mut self, ctx: &Context, open: &mut bool) -> Option<LatLng> {
        let mut result = None;
        if !*open {
            return result;
        }
        if self.tiles.is_none() {
            self.tiles = Some(HttpTiles::new(Waze, ctx.clone()));
        }
        let Self {
            map_memory,
            tiles,
            selected,
        } = self;

        let screen = ctx.content_rect().size();
        let max = egui::vec2(screen.x * 0.9, screen.y * 0.9);
        let mut should_close = false;
        egui::Window::new("Select center")
            .open(open)
            .resizable(true)
            .default_size([600.0, 500.0])
            .max_size(max)
            .collapsible(false)
            .show(ctx, |ui| {
                let map = Map::new(
                    tiles.as_mut().map(|t| t as &mut dyn Tiles),
                    map_memory,
                    latlng_to_position(*selected),
                )
                .zoom_gesture(true)
                .double_click_to_zoom(true)
                .zoom_with_ctrl(false);
                map.show(ui, |ui, response, projector, _memory| {
                    if response.clicked()
                        && let Some(pos) = response.interact_pointer_pos()
                    {
                        *selected = position_to_latlng(projector.unproject(pos.to_vec2()));
                        ui.ctx().request_repaint();
                    }
                    let pos = projector.project(latlng_to_position(*selected)).to_pos2();
                    let painter = ui.painter();
                    painter.circle(
                        pos,
                        8.0,
                        Color32::TRANSPARENT,
                        Stroke::new(2.0_f32, Color32::RED),
                    );
                    painter.circle_filled(pos, 2.0, Color32::RED);
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Center").strong());
                    ui.label("lat");
                    ui.add(DragValue::new(&mut selected.lat).speed(0.0001));
                    ui.label("lon");
                    ui.add(DragValue::new(&mut selected.lon).speed(0.0001));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("OK").clicked() {
                            result = Some(*selected);
                            should_close = true;
                        }
                        if ui.button("Cancel").clicked() {
                            should_close = true;
                        }
                    });
                });
            });
        if should_close {
            *open = false;
        }
        result
    }
}
