use crate::opendata::json::model::lat_lng::LatLng;
use chrono_tz::Tz;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Metadata {
    country: String,
    pub(crate) center: LatLng,
    sourceUrl: String,
    pub(crate) url: String,
    name: String,
    producer: Option<String>,
    licenceName: Option<String>,
    licenceUrl: Option<String>,
    locale: Option<String>,
    pub(crate) urlParams: Option<HashMap<String, String>>,
    tileServer: Option<String>,
    editorPattern: Option<String>,
}

impl Metadata {
    pub(crate) fn getLocale(&self) -> Tz {
        self.locale
            .as_ref()
            .map(|locale| Tz::from_str(&locale).unwrap_or(Tz::Europe__Paris))
            .unwrap_or(Tz::Europe__Paris)
    }
}

impl Metadata {
    pub(crate) fn new() -> Self {
        Self {
            tileServer: Some("WazeINTL".to_string()),
            editorPattern: Some(
                "https://waze.com/fr/editor?env=row&lat=${lat}&lon=${lon}&zoomLevel=19".to_string(),
            ),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use crate::MyError;
    use super::*;

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
        // assert_eq!(metadata.tileServer, Some("WazeINTL".to_string()));
        // assert_eq!(
        //     metadata.editorPattern,
        //     Some(
        //         "https://waze.com/fr/editor?env=row&lat=${lat}&lon=${lon}&zoomLevel=19".to_string()
        //     )
        // );
        Ok(())
    }
}
