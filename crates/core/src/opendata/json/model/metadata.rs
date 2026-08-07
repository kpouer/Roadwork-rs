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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    #[serde(rename = "licenceName")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub licence_name: Option<String>,
    #[serde(rename = "licenceUrl")]
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
    pub fn source_url(&self) -> Option<&str> {
        self.source_url.as_deref()
    }
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }
    pub fn locale_str(&self) -> Option<&str> {
        self.locale.as_deref()
    }
}
