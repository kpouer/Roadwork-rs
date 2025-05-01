use crate::opendata::json::model::date_parser::DateParser;
use crate::opendata::json::model::metadata::Metadata;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct ServiceDescriptor {
    pub(crate) metadata: Metadata,
    pub(crate) id: String,
    pub(crate) latitude: Option<String>,
    pub(crate) longitude: Option<String>,
    pub(crate) polygon: Option<String>,
    pub(crate) road: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) locationDetails: Option<String>,
    pub(crate) impactCirculationDetail: Option<String>,
    pub(crate) from: Option<DateParser>,
    pub(crate) to: Option<DateParser>,
    pub(crate) roadworkArray: String,
    pub(crate) url: Option<String>,
}
