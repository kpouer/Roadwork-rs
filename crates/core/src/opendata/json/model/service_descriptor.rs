use crate::opendata::json::model::date_parser::DateParser;
use crate::opendata::json::model::metadata::Metadata;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServiceDescriptor {
    pub metadata: Metadata,
    pub id: String,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub polygon: Option<String>,
    pub road: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "locationDetails")]
    pub location_details: Option<String>,
    #[serde(rename = "impactCirculationDetail")]
    pub impact_circulation_detail: Option<String>,
    pub from: Option<DateParser>,
    pub to: Option<DateParser>,
    #[serde(rename = "roadworkArray")]
    pub roadwork_array: String,
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

        assert_eq!(service_descriptor.roadwork_array, "$.records[*]");
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
