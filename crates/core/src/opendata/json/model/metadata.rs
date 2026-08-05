use crate::opendata::json::model::lat_lng::LatLng;
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Metadata {
    pub country: String,
    pub center: LatLng,
    #[serde(rename = "sourceUrl")]
    pub source_url: String,
    pub url: String,
    pub name: String,
    pub producer: Option<String>,
    #[serde(rename = "licenceName")]
    pub licence_name: Option<String>,
    #[serde(rename = "licenceUrl")]
    pub licence_url: Option<String>,
    pub locale: Option<String>,
    pub url_params: Option<HashMap<String, String>>,
    #[serde(rename = "tileServer")]
    pub tile_server: Option<String>,
    #[serde(rename = "editorPattern")]
    pub editor_pattern: Option<String>,
}

impl Metadata {
    pub fn get_locale(&self) -> Tz {
        self.locale
            .as_ref()
            .map(|locale| Tz::from_str(locale).unwrap_or(Tz::Europe__Paris))
            .unwrap_or(Tz::Europe__Paris)
    }

    pub fn country(&self) -> &str {
        &self.country
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn producer(&self) -> Option<&str> {
        self.producer.as_deref()
    }
    pub fn licence_name(&self) -> Option<&str> {
        self.licence_name.as_deref()
    }
    pub fn licence_url(&self) -> Option<&str> {
        self.licence_url.as_deref()
    }
    pub fn source_url(&self) -> &str {
        &self.source_url
    }
    pub fn locale_str(&self) -> Option<&str> {
        self.locale.as_deref()
    }
    pub fn tile_server(&self) -> Option<&str> {
        self.tile_server.as_deref()
    }
}
