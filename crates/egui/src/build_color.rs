include!(concat!(env!("OUT_DIR"), "/build_color.rs"));

pub(crate) fn build_color() -> egui::Color32 {
    egui::Color32::from_rgb(BUILD_COLOR_R, BUILD_COLOR_G, BUILD_COLOR_B)
}
