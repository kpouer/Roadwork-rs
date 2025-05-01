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
    #[serde(rename = "locationDetails")]
    pub(crate) location_details: Option<String>,
    #[serde(rename = "impactCirculationDetail")]
    pub(crate) impact_circulation_detail: Option<String>,
    pub(crate) from: Option<DateParser>,
    pub(crate) to: Option<DateParser>,
    #[serde(rename = "roadworkArray")]
    pub(crate) roadwork_array: String,
    pub(crate) url: Option<String>,
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use super::*;

    #[test]
    fn test_service_descriptor() -> Result<(), Box<dyn std::error::Error>> {
        let file = File::open("opendata/json/France-Paris.json")?;
        let service_descriptor = serde_json::from_reader::<File, ServiceDescriptor>(file)?;

        // Vérifier les champs de navigation JSON
        assert_eq!(service_descriptor.roadwork_array, "$.records");
        assert_eq!(service_descriptor.id, "@.recordid");
        assert_eq!(service_descriptor.latitude, Some("@.geometry.coordinates[1]".to_string()));
        assert_eq!(service_descriptor.longitude, Some("@.geometry.coordinates[0]".to_string()));
        assert_eq!(service_descriptor.polygon, Some("@.fields.geo_shape.coordinates".to_string()));
        assert_eq!(service_descriptor.road, Some("@.fields.voie".to_string()));
        assert_eq!(service_descriptor.location_details, Some("@.fields.precision_localisation".to_string()));
        assert_eq!(service_descriptor.impact_circulation_detail, Some("@.fields.impact_circulation_detail".to_string()));

        // Vérifier que les champs optionnels sont correctement définis
        assert!(service_descriptor.from.is_some());
        assert!(service_descriptor.to.is_some());

        // Vérifier que l'URL des métadonnées est correcte
        assert_eq!(service_descriptor.metadata.url, "https://opendata.paris.fr/api/records/1.0/search/?dataset=chantiers-perturbants&q=&rows=1000&facet=cp_arrondissement&facet=typologie&facet=maitre_ouvrage&facet=objet&facet=impact_circulation&facet=niveau_perturbation&facet=statut&exclude.statut=5");

        Ok(())

    }
}
