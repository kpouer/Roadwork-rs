use eframe::wasm_bindgen::JsCast;
use log::LevelFilter;
use roadwork_egui::roadwork_app::RoadworkApp;

fn main() {
    eframe::WebLogger::init(LevelFilter::Info).ok();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");

        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(move |cc| {
                    egui_extras::install_image_loaders(&cc.egui_ctx);
                    Ok(Box::new(RoadworkApp::new(cc.egui_ctx.clone())))
                }),
            )
            .await
            .expect("failed to start eframe");
    });
}
