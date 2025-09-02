use crate::model::roadwork::Roadwork;
use eframe::emath::Pos2;
use eframe::epaint::{Color32, Stroke};
use egui::{Response, Sense, Ui, Widget};
use roadwork_sync::Status;
use walkers::{Position, Projector};

pub struct RoadworkMarker<'a> {
    roadwork: &'a Roadwork,
    projector: &'a Projector,
    clicked: bool,
}

impl<'a> RoadworkMarker<'a> {
    pub(crate) fn new(roadwork: &'a Roadwork, projector: &'a Projector, clicked: bool) -> Self {
        Self {
            roadwork,
            projector,
            clicked,
        }
    }
}

impl RoadworkMarker<'_> {
    fn status_2_color(status: Status) -> Color32 {
        match status {
            Status::New => Color32::RED,
            Status::Later => Color32::BLUE,
            Status::Ignored => Color32::YELLOW,
            Status::Finished => Color32::DARK_GRAY,
            Status::Treated => Color32::GREEN,
        }
    }

    fn is_within_circle(center: Pos2, pos: Pos2, radius: f32) -> bool {
        let dx = center.x - pos.x;
        let dy = center.y - pos.y;
        let distance_squared = dx * dx + dy * dy;
        distance_squared <= radius * radius
    }
}

impl Widget for RoadworkMarker<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        // let response = draw_roadwork(
        //     &mut child_ui,
        //     self.roadwork,
        //     self.roadwork.sync_data.status == Status::New,
        // )
        let screen_position = self.projector.project(Position::new(
            self.roadwork.longitude,
            self.roadwork.latitude,
        ));
        let painter = ui.painter();
        let color32 = Self::status_2_color(self.roadwork.sync_data.status);
        if self.clicked
            && let Some(pos) = ui.ctx().pointer_interact_pos()
            && Self::is_within_circle(screen_position.to_pos2(), pos, 10.0)
        {
            response.mark_changed();
        }
        painter.circle(screen_position.to_pos2(), 10., color32, Stroke::default());
        response
    }
}
