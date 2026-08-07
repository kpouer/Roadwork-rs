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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_opendata_service_descriptor() -> Result<(), Box<dyn std::error::Error>> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = std::path::Path::new(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let path = workspace_root.join("opendata/json/France-Paris.json");
        let file = File::open(path)?;
        let descriptor = serde_json::from_reader::<File, OpendataServiceDescriptor>(file)?;

        assert_eq!(descriptor.data_array, "$.records[*]");
        assert_eq!(descriptor.id, "$.recordid");
        assert_eq!(
            descriptor.latitude,
            Some("$.geometry.coordinates[1]".to_string())
        );
        assert_eq!(
            descriptor.longitude,
            Some("$.geometry.coordinates[0]".to_string())
        );
        assert_eq!(
            descriptor.polygon,
            Some("$.fields.geo_shape.coordinates[0]".to_string())
        );
        assert!(descriptor.description.is_none());
        assert_eq!(
            descriptor.metadata.url,
            "https://opendata.paris.fr/api/records/1.0/search/?dataset=chantiers-perturbants&q=&rows=1000&facet=cp_arrondissement&facet=typologie&facet=maitre_ouvrage&facet=objet&facet=impact_circulation&facet=niveau_perturbation&facet=statut&exclude.statut=5"
        );

        Ok(())
    }
}
