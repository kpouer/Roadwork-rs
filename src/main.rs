#![windows_subsystem = "windows"]

use egui_extras::install_image_loaders;
use roadworkapp_lib::roadwork::RoadworkApp;

fn main() -> eframe::Result {
    env_logger::init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("Roadwork")
            // .with_icon(icon_data())
            .with_min_inner_size([320.0, 200.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Disk Mosaic",
        options,
        Box::new(|ctx| {
            install_image_loaders(&ctx.egui_ctx);
            Ok(Box::new(RoadworkApp::new(ctx.egui_ctx.clone())))
        }),
    )
}

// fn icon_data() -> egui::IconData {
//     let app_icon_png_bytes = include_bytes!("../media/icon.png");
//
//     match eframe::icon_data::from_png_bytes(app_icon_png_bytes) {
//         Ok(icon_data) => icon_data,
//         Err(err) => panic!("Failed to load app icon: {err}"),
//     }
// }
