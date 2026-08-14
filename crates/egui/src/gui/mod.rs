pub mod about_dialog;
pub mod center_picker_dialog;
pub mod metada_dialog;
mod metadata_form;
pub mod roadwork_marker;
pub mod service_helper_dialog;
pub mod service_helper_form;
pub mod settings_dialog;
pub mod status_panel;

use egui::Ui;
use serde::Serialize;

/// Serializes a JSON value with a tab-based indentation.
pub(crate) fn pretty_json_tabs<T: Serialize + ?Sized>(value: &T) -> serde_json::Result<String> {
    let mut buf = Vec::new();
    let mut serializer = serde_json::Serializer::with_formatter(
        &mut buf,
        serde_json::ser::PrettyFormatter::with_indent(b"\t"),
    );
    value.serialize(&mut serializer)?;
    Ok(String::from_utf8(buf).expect("serde_json only emits valid UTF-8"))
}

/// Above this size, raw JSON editors skip syntax highlighting so that laying out
/// huge dropped/fetched documents does not stall the UI on every frame.
const MAX_HIGHLIGHT_BYTES: usize = 2 * 1024 * 1024;

pub(crate) fn layout_json_text(
    ui: &Ui,
    text: &dyn egui::TextBuffer,
    wrap_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let source = text.as_str();
    if source.len() > MAX_HIGHLIGHT_BYTES {
        let job = egui::text::LayoutJob::simple(
            source.to_string(),
            egui::FontId::monospace(12.0),
            ui.visuals().text_color(),
            wrap_width,
        );
        return ui.fonts_mut(|f| f.layout_job(job));
    }
    let theme = egui_extras::syntax_highlighting::CodeTheme::from_style(ui.style());
    let mut job =
        egui_extras::syntax_highlighting::highlight(ui.ctx(), ui.style(), &theme, source, "json");
    job.wrap.max_width = wrap_width;
    ui.fonts_mut(|f| f.layout_job(job))
}
