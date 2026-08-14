pub mod about_dialog;
pub mod center_picker_dialog;
pub mod metada_dialog;
mod metadata_form;
pub mod roadwork_marker;
pub mod service_helper_dialog;
pub mod service_helper_form;
pub mod settings_dialog;
pub mod status_panel;

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
