use crate::opendata::json::model::metadata::Metadata;
use crate::opendata::json::model::service_descriptor::ServiceDescriptor;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct OpendataServiceDescriptor {
    pub metadata: Metadata,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latitude: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub longitude: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polygon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "dataArray")]
    pub data_array: String,
}

impl From<&ServiceDescriptor> for OpendataServiceDescriptor {
    fn from(descriptor: &ServiceDescriptor) -> Self {
        Self {
            metadata: descriptor.metadata.clone(),
            id: descriptor.id.clone(),
            latitude: descriptor.latitude.clone(),
            longitude: descriptor.longitude.clone(),
            polygon: descriptor.polygon.clone(),
            description: descriptor.description.clone(),
            data_array: descriptor.data_array.clone(),
        }
    }
}
