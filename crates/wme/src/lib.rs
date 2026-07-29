//! Roadwork WME extension — builds a `.user.js` userscript for the Waze Map Editor.
//!
//! The actual logic lives in the static JS/CSS files assembled at build time.

/// The version of the WME extension, sourced from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The build date of the WME extension, injected by `build.rs`.
pub const BUILD_DATE: &str = env!("WME_BUILD_DATE");
