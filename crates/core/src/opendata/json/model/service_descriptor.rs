use crate::opendata::json::model::date_parser::DateParser;
use crate::opendata::json::model::metadata::Metadata;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ServiceDescriptor {
    pub metadata: Metadata,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polygon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub road: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "locationDetails")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_details: Option<String>,
    #[serde(rename = "impactCirculationDetail")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact_circulation_detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<DateParser>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<DateParser>,
    pub data_array: String,
}
