use crate::opendata::json::model::lat_lng::LatLng;
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Metadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    /// Stable identity of the source, decoupled from the file path. If absent,
    /// the descriptor key falls back to the file path without the `.json`
    /// extension. A non-empty id is typically a UUID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub center: LatLng,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub licence_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub licence_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url_params: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl Metadata {
    /// Returns the stable key of this source: `category.id` when present and
    /// non-empty, otherwise the given fallback (the file path without the
    /// `.json` extension).
    pub fn effective_key(&self, fallback: &str) -> String {
        self.id
            .as_deref()
            .filter(|id| !id.trim().is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| fallback.to_owned())
    }

    /// Human-readable display label: `<country> - <name>` (just `name` when
    /// the country is missing).
    pub fn label(&self) -> String {
        match (&self.country, self.name.as_str()) {
            (Some(country), name) if !country.trim().is_empty() => format!("{country} - {name}"),
            (_, name) => name.to_string(),
        }
    }

    pub fn get_locale(&self) -> Tz {
        self.locale
            .as_ref()
            .map(|locale| Tz::from_str(locale).unwrap_or(Tz::Europe__Paris))
            .unwrap_or(Tz::Europe__Paris)
    }
}
