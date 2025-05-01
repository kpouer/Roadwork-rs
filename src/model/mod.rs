pub(crate) mod date_range;
pub(crate) mod roadwork;
pub(crate) mod roadwork_data;
pub(crate) mod wkt;

use egui::Color32;
use roadwork_sync::Status;

pub fn status_to_color(status: Status) -> Color32 {
    match status {
        Status::New => Color32::RED,
        Status::Later => Color32::BLUE,
        Status::Ignored => Color32::YELLOW,
        Status::Finished => Color32::GRAY,
        Status::Treated => Color32::GREEN,
    }
}
