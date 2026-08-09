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
    install_panic_hook();

    let web_options = eframe::WebOptions::default();

    roadwork_egui::roadwork_app::setup_helper_data_listener();

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

/// Installs a panic hook that always surfaces the panic message: directly to the
/// browser console and as a fixed overlay on the page, so a crash is never a
/// silent freeze and the root cause is visible without digging through the
/// console. Only `eframe`'s console logger is not enough: panics in the app
/// would otherwise just print an opaque wasm `RuntimeError: unreachable`.
#[cfg(target_arch = "wasm32")]
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            format!("{:?}", info.payload())
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown location".to_string());
        let message = format!("Roadwork panic: {payload} ({location})");
        web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&message));
        if let Some(window) = web_sys::window()
            && let Some(document) = window.document()
            && let Ok(div) = document.create_element("div")
        {
            let _ = div.set_attribute(
                "style",
                "position:fixed;top:0;left:0;right:0;z-index:99999;\
                 background:#a00;color:#fff;font:12px monospace;padding:8px;\
                 white-space:pre-wrap;",
            );
            div.set_text_content(Some(&message));
            if let Some(body) = document.body() {
                let _ = body.insert_adjacent_element("afterbegin", &div);
            }
        }
    }));
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
            "descriptor" if !value.is_empty() => {
                params.opendata_descriptor = Some(decode_url(value));
            }
            _ => {}
        }
    }
    params
}

#[cfg(target_arch = "wasm32")]
fn decode_url(value: &str) -> String {
    let value = value.replace('+', "%20");
    js_sys::decode_uri_component(&value)
        .map(|v| v.as_string().unwrap_or_default())
        .unwrap_or_default()
}
