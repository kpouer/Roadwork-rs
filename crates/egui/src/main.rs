use roadwork_egui::roadwork_app::{RoadworkApp, StartupParams};

#[cfg(target_arch = "wasm32")]
use eframe::wasm_bindgen::JsCast;
#[cfg(target_arch = "wasm32")]
use log::LevelFilter;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Roadwork",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(RoadworkApp::new(
                cc.egui_ctx.clone(),
                StartupParams::default(),
            )))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
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

        let params = read_startup_params();

        eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(move |cc| {
                    egui_extras::install_image_loaders(&cc.egui_ctx);
                    Ok(Box::new(RoadworkApp::new(cc.egui_ctx.clone(), params)))
                }),
            )
            .await
            .expect("failed to start eframe");
    });
}

#[cfg(target_arch = "wasm32")]
fn read_startup_params() -> StartupParams {
    let mut params = StartupParams::default();
    let search = web_sys::window()
        .and_then(|w| w.location().search().ok())
        .unwrap_or_default();
    for pair in search.trim_start_matches('?').split('&') {
        let mut it = pair.splitn(2, '=');
        let key = it.next().unwrap_or_default();
        let value = it.next().unwrap_or_default();
        match key {
            "service" if !value.is_empty() => {
                params.service = Some(decode_url(value));
            }
            "serviceHelper" if value == "1" => {
                params.open_service_helper = true;
            }
            "opendata" if value == "1" => {
                params.open_opendata_service_helper = true;
            }
            "create" if value == "1" => {
                params.create_opendata_service = true;
            }
            _ => {}
        }
    }
    params
}

#[cfg(target_arch = "wasm32")]
fn decode_url(value: &str) -> String {
    value.replace('+', " ")
}
