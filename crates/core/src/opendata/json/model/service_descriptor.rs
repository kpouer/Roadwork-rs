use crate::opendata::json::model::date_parser::DateParser;
use crate::opendata::json::model::metadata::Metadata;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    #[serde(rename = "dataArray")]
    pub data_array: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_service_descriptor() -> Result<(), Box<dyn std::error::Error>> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let workspace_root = std::path::Path::new(manifest_dir)
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let path = workspace_root.join("opendata/json/France-Paris.json");
        let file = File::open(path)?;
        let service_descriptor = serde_json::from_reader::<File, ServiceDescriptor>(file)?;

        assert_eq!(service_descriptor.data_array, "$.records[*]");
        assert_eq!(service_descriptor.id, "$.recordid");
        assert_eq!(
            service_descriptor.latitude,
            Some("$.geometry.coordinates[1]".to_string())
        );
        assert_eq!(
            service_descriptor.longitude,
            Some("$.geometry.coordinates[0]".to_string())
        );
        assert_eq!(
            service_descriptor.polygon,
            Some("$.fields.geo_shape.coordinates[0]".to_string())
        );
        assert_eq!(service_descriptor.road, Some("$.fields.voie".to_string()));
        assert_eq!(
            service_descriptor.location_details,
            Some("$.fields.precision_localisation".to_string())
        );
        assert_eq!(
            service_descriptor.impact_circulation_detail,
            Some("$.fields.impact_circulation_detail".to_string())
        );

        assert!(service_descriptor.from.is_some());
        assert!(service_descriptor.to.is_some());

        assert_eq!(
            service_descriptor.metadata.url,
            "https://opendata.paris.fr/api/records/1.0/search/?dataset=chantiers-perturbants&q=&rows=1000&facet=cp_arrondissement&facet=typologie&facet=maitre_ouvrage&facet=objet&facet=impact_circulation&facet=niveau_perturbation&facet=statut&exclude.statut=5"
        );

        Ok(())
    }
}
