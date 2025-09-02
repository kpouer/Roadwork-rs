use crate::opendata::json::model::lat_lng::LatLng;
use chrono_tz::Tz;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Metadata {
    country: String,
    pub(crate) center: LatLng,
    #[serde(rename = "sourceUrl")]
    source_url: String,
    pub(crate) url: String,
    name: String,
    producer: Option<String>,
    #[serde(rename = "licenceName")]
    licence_name: Option<String>,
    #[serde(rename = "licenceUrl")]
    licence_url: Option<String>,
    locale: Option<String>,
    pub(crate) url_params: Option<HashMap<String, String>>,
    #[serde(rename = "tileServer")]
    tile_server: Option<String>,
    #[serde(rename = "editorPattern")]
    pub(crate) editor_pattern: Option<String>,
}

impl Metadata {
    pub(crate) fn get_locale(&self) -> Tz {
        self.locale
            .as_ref()
            .map(|locale| Tz::from_str(locale).unwrap_or(Tz::Europe__Paris))
            .unwrap_or(Tz::Europe__Paris)
    }
}

impl Metadata {
    pub(crate) fn new() -> Self {
        Self {
            tile_server: Some("WazeINTL".to_string()),
            editor_pattern: Some(
                "https://waze.com/fr/editor?env=row&lat=${lat}&lon=${lon}&zoomLevel=19".to_string(),
            ),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MyError;

    #[test]
    fn test_deserialize() -> Result<(), MyError> {
        let json = r#"
{
    "country": "France",
    "name": "Paris",
    "producer": "Direction de la Voirie et des Déplacements - Ville de Paris",
    "licenceName": "Open Database License (ODbL)",
    "licenceUrl": "https://opendatacommons.org/licenses/odbl/",
        "sourceUrl": "https://opendata.paris.fr/explore/dataset/chantiers-perturbants/information/?disjunctive.cp_arrondissement&disjunctive.maitre_ouvrage&disjunctive.objet&disjunctive.impact_circulation&disjunctive.niveau_perturbation&disjunctive.statut",
        "url": "https://opendata.paris.fr/api/records/1.0/search/?dataset=chantiers-perturbants&q=&rows=1000&facet=cp_arrondissement&facet=typologie&facet=maitre_ouvrage&facet=objet&facet=impact_circulation&facet=niveau_perturbation&facet=statut&exclude.statut=5",
        "center": {
            "lat":48.85337,
            "lon": 2.34847
        },
        "locale": "fr_FR"
    }
        "#;
        let metadata = serde_json::from_str::<Metadata>(json)?;
        assert_eq!(metadata.country, "France");
        assert_eq!(metadata.name, "Paris");
        assert_eq!(
            metadata.producer,
            Some("Direction de la Voirie et des Déplacements - Ville de Paris".to_string())
        );
        assert_eq!(
            metadata.licence_name,
            Some("Open Database License (ODbL)".to_string())
        );
        assert_eq!(
            metadata.licence_url,
            Some("https://opendatacommons.org/licenses/odbl/".to_string())
        );
        assert_eq!(
            metadata.source_url,
            "https://opendata.paris.fr/explore/dataset/chantiers-perturbants/information/?disjunctive.cp_arrondissement&disjunctive.maitre_ouvrage&disjunctive.objet&disjunctive.impact_circulation&disjunctive.niveau_perturbation&disjunctive.statut"
        );
        assert_eq!(
            metadata.url,
            "https://opendata.paris.fr/api/records/1.0/search/?dataset=chantiers-perturbants&q=&rows=1000&facet=cp_arrondissement&facet=typologie&facet=maitre_ouvrage&facet=objet&facet=impact_circulation&facet=niveau_perturbation&facet=statut&exclude.statut=5"
        );
        assert_eq!(metadata.center.lat, 48.85337);
        assert_eq!(metadata.center.lon, 2.34847);
        assert_eq!(metadata.locale, Some("fr_FR".to_string()));
        Ok(())
    }
}
