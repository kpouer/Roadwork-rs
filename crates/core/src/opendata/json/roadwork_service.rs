use crate::MyError;
use crate::http_service::HttpService;
use crate::json_tools::JsonTools;
use crate::model::date_range::DateRange;
use crate::model::roadwork::Roadwork;
use crate::model::roadwork_data::RoadworkData;
use crate::opendata::json::model::date_parser::DateParser;
use crate::opendata::json::model::date_result::DateResult;
use crate::opendata::json::model::opendata_service_descriptor::OpendataServiceDescriptor;
use crate::opendata::json::model::service_descriptor::ServiceDescriptor;
use crate::opendata::json::opendata_service::OpendataService;
use crate::opendata::json::path_validation::PathValidation;
use chrono::{DateTime, Datelike, Timelike};
use chrono_tz::Tz;
use jsonpath_rust::JsonPath;
use log::{error, info, warn};
use serde_json::Value;

#[derive(Debug)]
pub struct RoadworkService {
    pub service_name: String,
    pub service_descriptor: ServiceDescriptor,
}

impl RoadworkService {
    fn opendata_service(&self) -> OpendataService {
        OpendataService {
            service_name: self.service_name.clone(),
            service_descriptor: OpendataServiceDescriptor::from(&self.service_descriptor),
        }
    }

    pub async fn get_data(&self) -> Result<RoadworkData, MyError> {
        let url = self.build_url();
        info!("getData {url}");
        let json = HttpService.get_url(&url).await?;
        self.parse_json(&json)
    }

    pub fn roadwork_array_targets_array(&self, json: &str) -> bool {
        self.opendata_service().roadwork_array_targets_array(json)
    }

    pub fn roadwork_count(&self, json: &str) -> usize {
        self.opendata_service().element_count(json)
    }

    pub fn element_at(&self, json: &str, index: usize) -> Option<Value> {
        self.opendata_service().element_at(json, index)
    }

    /// Validates every descriptor path against every element of the roadwork array.
    pub fn validate(&self, json: &str) -> Vec<PathValidation> {
        let ods = self.opendata_service();
        let mut report = ods.validate(json);
        let elements = ods.roadwork_elements(json);
        if elements.is_empty() {
            return report;
        }

        let count = elements.len();
        let descriptor = &self.service_descriptor;

        report.extend(
            [
                ods.optional_scalar_report(&elements, "road", descriptor.road.as_ref()),
                ods.optional_scalar_report(&elements, "url", descriptor.url.as_ref()),
                ods.optional_scalar_report(
                    &elements,
                    "locationDetails",
                    descriptor.location_details.as_ref(),
                ),
                ods.optional_scalar_report(
                    &elements,
                    "impactCirculationDetail",
                    descriptor.impact_circulation_detail.as_ref(),
                ),
            ]
            .into_iter()
            .flatten(),
        );

        let locale = descriptor.metadata.get_locale();
        for (label, date) in [
            ("from", descriptor.from.as_ref()),
            ("to", descriptor.to.as_ref()),
        ] {
            let Some(date) = date else { continue };
            if date.path.trim().is_empty() {
                continue;
            }
            let failures = ods.optional_path_failures(&elements, |element| {
                element
                    .get_path(&date.path)
                    .map(|value| date.parse(&value, locale).is_ok())
                    .unwrap_or(false)
            });
            report.push(PathValidation::new(
                label,
                &date.path,
                false,
                "parseable date",
                failures,
                count,
                None,
            ));
        }

        report
    }

    pub fn path_points_to_scalar(&self, json: &str, path: &str) -> bool {
        self.opendata_service().path_points_to_scalar(json, path)
    }

    pub fn path_fetched_value(&self, json: &str, path: &str) -> Option<String> {
        self.opendata_service().path_fetched_value(json, path)
    }

    pub fn path_fetched_value_in(&self, element: &Value, path: &str) -> Option<String> {
        self.opendata_service().path_fetched_value_in(element, path)
    }

    pub fn path_points_to_scalar_or_array(&self, json: &str, path: &str) -> bool {
        self.opendata_service()
            .path_points_to_scalar_or_array(json, path)
    }

    pub fn path_points_to_scalar_in(&self, element: &Value, path: &str) -> bool {
        self.opendata_service()
            .path_points_to_scalar_in(element, path)
    }

    pub fn path_points_to_scalar_or_array_in(&self, element: &Value, path: &str) -> bool {
        self.opendata_service()
            .path_points_to_scalar_or_array_in(element, path)
    }

    pub fn parse_json(&self, json: &str) -> Result<RoadworkData, MyError> {
        let json: serde_json::Value = serde_json::from_str(json)?;
        let data_array = json.query(&self.service_descriptor.data_array)?;
        info!("Found {} items", data_array.len());
        let mut roadworks = Vec::with_capacity(data_array.len());
        for value in data_array {
            match self.build_roadwork(value) {
                Ok(roadwork) => {
                    if Self::is_valid(&roadwork) {
                        roadworks.push(roadwork);
                    } else {
                        warn!("{roadwork:?} is invalid");
                    }
                }
                Err(e) => warn!("Unable to build roadwork {}", e),
            }
        }
        Ok(RoadworkData::new(&self.service_name, roadworks))
    }

    pub fn parse_json_preview(&self, json: &str) -> Result<RoadworkData, MyError> {
        let json: serde_json::Value = serde_json::from_str(json)?;
        let data_array = json.query(&self.service_descriptor.data_array)?;
        let mut items = Vec::with_capacity(data_array.len());
        for (index, value) in data_array.into_iter().enumerate() {
            let mut item = self.build_roadwork_preview(value);
            if item.opendata.id.is_empty() {
                item.opendata.id = format!("element#{index}");
            }
            items.push(item);
        }
        Ok(RoadworkData::new(&self.service_name, items))
    }

    pub fn extract_roadwork_array(&self, json: &str) -> Result<Value, MyError> {
        self.opendata_service().extract_roadwork_array(json)
    }

    pub fn build_url(&self) -> String {
        self.opendata_service().build_url()
    }

    fn is_valid(roadwork: &Roadwork) -> bool {
        if roadwork.opendata.longitude == 0.0 && roadwork.opendata.latitude == 0.0 {
            warn!("{roadwork:?} is invalid because it has no location");
            return false;
        }
        true
    }

    fn build_roadwork(&self, node: &Value) -> Result<Roadwork, MyError> {
        let mut roadwork_builder = Roadwork {
            opendata: self.opendata_service().build_opendata(node)?,
            ..Roadwork::default()
        };
        self.fill_roadwork_fields(node, &mut roadwork_builder);
        let date_range = self.get_date_range(node)?;
        roadwork_builder.start = date_range.from.timestamp_millis();
        roadwork_builder.end = date_range
            .to
            .map(|date| date.timestamp_millis())
            .unwrap_or(0);
        if let Some(url) = &self.service_descriptor.url {
            roadwork_builder.url = node.get_path(url)?;
        }
        Ok(roadwork_builder)
    }

    fn build_roadwork_preview(&self, node: &Value) -> Roadwork {
        let mut roadwork_builder = Roadwork {
            opendata: self.opendata_service().build_opendata_preview(node),
            ..Roadwork::default()
        };
        self.fill_roadwork_fields(node, &mut roadwork_builder);
        if self.service_descriptor.from.is_some()
            && let Ok(date_range) = self.get_date_range(node)
        {
            roadwork_builder.start = date_range.from.timestamp_millis();
            roadwork_builder.end = date_range
                .to
                .map(|date| date.timestamp_millis())
                .unwrap_or(0);
        }
        if let Some(url) = &self.service_descriptor.url {
            roadwork_builder.url = node.get_path(url).unwrap_or_default();
        }
        roadwork_builder
    }

    fn fill_roadwork_fields(&self, node: &Value, roadwork_builder: &mut Roadwork) {
        if let Some(road) = &self.service_descriptor.road {
            roadwork_builder.road = node.get_path(road).ok();
        }
        if let Some(location_details) = &self.service_descriptor.location_details {
            roadwork_builder.location_details = node.get_path(location_details).ok();
        }
        if let Some(impact_circulation_detail) = &self.service_descriptor.impact_circulation_detail
        {
            roadwork_builder.impact_circulation_detail =
                node.get_path(impact_circulation_detail).ok();
        }
    }

    fn parse_date(
        &self,
        node: &Value,
        date_parser: &Option<DateParser>,
    ) -> Result<DateResult, MyError> {
        if date_parser.is_none() {
            return Err(MyError::ParsingError(
                "Cannot parse date as dateParse is null".to_string(),
            ));
        }
        let current_year = chrono::Utc::now().year();
        let date_parser = date_parser.as_ref().unwrap();
        let value = node.get_path(&date_parser.path)?;
        let mut result =
            date_parser.parse(&value, self.service_descriptor.metadata.get_locale())?;
        if result.reset_hour {
            match Self::drop_time(&result.date) {
                None => warn!("Unable to add year to date {}", result.date),
                Some(date) => result.date = date,
            }
        }
        if result.add_year {
            match Self::add_year(current_year, &result.date) {
                None => warn!("Unable to add year to date {}", result.date),
                Some(date) => result.date = date,
            }
        }
        Ok(result)
    }

    fn add_year(year: i32, date: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        date.with_year(year)
    }

    fn drop_time(date: &DateTime<Tz>) -> Option<DateTime<Tz>> {
        date.with_hour(0)
            .unwrap()
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
    }

    fn get_date_range(&self, node: &Value) -> Result<DateRange, MyError> {
        let current_year = chrono::Utc::now().year();
        let start_time = self
            .parse_date(node, &self.service_descriptor.from)
            .map(|date_result| date_result.date)
            .inspect_err(|e| error!("Error parsing start date {}", e))?;
        match self.parse_date(node, &self.service_descriptor.to) {
            Ok(end) => {
                let mut end_date = end.date;
                if end.add_year {
                    match Self::add_year(current_year, &end_date) {
                        None => {}
                        Some(date_time) => end_date = date_time,
                    }
                    if start_time > end_date {
                        match Self::add_year(current_year, &end_date) {
                            None => {}
                            Some(date_time) => end_date = date_time,
                        }
                    }
                }
                Ok(DateRange {
                    from: start_time,
                    to: Some(end_date),
                })
            }
            Err(e) => {
                error!("Error parsing end date {}", e);
                Ok(DateRange {
                    from: start_time,
                    to: None,
                })
            }
        }
    }
}

#[cfg(test)]
mod validate_tests {
    use super::*;
    use crate::opendata::json::model::service_descriptor::ServiceDescriptor;

    fn descriptor() -> ServiceDescriptor {
        serde_json::from_str::<ServiceDescriptor>(
            r#"{
                "metadata": {
                    "country": "France", "name": "Paris", "sourceUrl": "s", "url": "u",
                    "center": {"lat": 48.0, "lon": 2.0}, "locale": "fr_FR"
                },
                "dataArray": "$.records[*]",
                "id": "$.recordid",
                "latitude": "$.geometry.coordinates[1]",
                "longitude": "$.geometry.coordinates[0]",
                "polygon": "$.fields.geo_shape.coordinates[0]",
                "road": "$.fields.voie",
                "locationDetails": "$.fields.precision_localisation",
                "impactCirculationDetail": "$.fields.impact_circulation_detail",
                "from": {"path": "$.fields.date_debut", "parsers": [{"matcher": ".*", "format": "%Y-%m-%d"}]},
                "to": {"path": "$.fields.date_fin", "parsers": [{"matcher": ".*", "format": "%Y-%m-%d"}]}
            }"#,
        )
        .unwrap()
    }

    fn json() -> &'static str {
        r#"{
            "records": [
                {
                    "recordid": "abc",
                    "geometry": {"coordinates": [2.35, 48.85]},
                    "fields": {
                        "voie": "Rue X",
                        "precision_localisation": "Precise",
                        "impact_circulation_detail": "Some",
                        "geo_shape": {"coordinates": [[[2.34,48.84],[2.36,48.86]]]},
                        "date_debut": "2023-01-01",
                        "date_fin": "2023-01-31"
                    }
                },
                {
                    "recordid": "abc",
                    "fields": {
                        "geo_shape": {"coordinates": null},
                        "date_debut": "not-a-date"
                    }
                }
            ]
        }"#
    }

    #[test]
    fn validates_all_elements() {
        let rws = RoadworkService {
            service_name: "test".to_string(),
            service_descriptor: descriptor(),
        };
        let report = rws.validate(json());
        let get = |label: &str| report.iter().find(|p| p.label == label).unwrap();
        assert!(get("dataArray").is_valid());
        assert!(get("id").is_valid());
        assert_eq!(get("id unique").failures, vec![0, 1]);
        assert!(get("latitude").is_valid());
        assert!(get("longitude").is_valid());
        assert!(get("road").is_valid());
        assert!(get("locationDetails").is_valid());
        assert!(get("impactCirculationDetail").is_valid());
        assert!(get("polygon").is_valid());
        assert!(get("from").is_valid());
        assert!(get("to").is_valid());
    }

    #[test]
    fn optional_path_valid_when_matches_at_least_one() {
        let mut service_descriptor = descriptor();
        service_descriptor.road = Some("$.fields.voie".to_string());
        let json = r#"{
            "records": [
                {"recordid": "1", "fields": {"voie": "Rue X"}},
                {"recordid": "2", "fields": {}}
            ]
        }"#;
        let rws = RoadworkService {
            service_name: "test".to_string(),
            service_descriptor,
        };
        let report = rws.validate(json);
        let get = |label: &str| report.iter().find(|p| p.label == label).unwrap();
        assert!(get("road").is_valid());
        assert_eq!(get("road").failures, Vec::<usize>::new());
    }

    #[test]
    fn optional_path_invalid_when_matches_none() {
        let mut service_descriptor = descriptor();
        service_descriptor.road = Some("$.fields.nonexistent".to_string());
        let json = r#"{
            "records": [
                {"recordid": "1", "fields": {"voie": "Rue X"}},
                {"recordid": "2", "fields": {"voie": "Rue Y"}}
            ]
        }"#;
        let rws = RoadworkService {
            service_name: "test".to_string(),
            service_descriptor,
        };
        let report = rws.validate(json);
        let get = |label: &str| report.iter().find(|p| p.label == label).unwrap();
        assert!(!get("road").is_valid());
        assert_eq!(get("road").failures, vec![0, 1]);
    }

    #[test]
    fn reports_bad_array_path() {
        let mut service_descriptor = descriptor();
        service_descriptor.data_array = "$.nope".to_string();
        let rws = RoadworkService {
            service_name: "test".to_string(),
            service_descriptor,
        };
        let report = rws.validate(json());
        assert!(!report[0].is_valid());
        assert_eq!(report[0].label, "dataArray");
        assert_eq!(report.len(), 1);
    }
}
