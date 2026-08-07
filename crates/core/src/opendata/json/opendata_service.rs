use crate::http_service::HttpService;
use crate::json_tools::JsonTools;
use crate::model::opendata::Opendata;
use crate::model::opendata_data::OpendataData;
use crate::opendata::json::model::metadata::Metadata;
use crate::opendata::json::model::opendata_service_descriptor::OpendataServiceDescriptor;
use crate::opendata::json::path_validation::PathValidation;
use crate::{MyError, json_tools};
use jsonpath_rust::JsonPath;
use log::{info, warn};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug)]
pub struct OpendataService {
    pub service_name: String,
    pub service_descriptor: OpendataServiceDescriptor,
}

impl From<&OpendataServiceDescriptor> for OpendataService {
    fn from(service_descriptor: &OpendataServiceDescriptor) -> Self {
        Self {
            service_name: service_descriptor.metadata.name.clone(),
            service_descriptor: service_descriptor.clone(),
        }
    }
}

impl OpendataService {
    pub async fn get_data(&self) -> Result<OpendataData, MyError> {
        let url = self.build_url();
        info!("getData {url}");
        let json = HttpService.get_url(&url).await?;
        self.parse_json(&json)
    }

    pub fn roadwork_array_targets_array(&self, json: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(json) else {
            return false;
        };
        let Ok(results) = value.query(&self.service_descriptor.data_array) else {
            return false;
        };
        if results.is_empty() {
            return false;
        }
        results.iter().any(|v| v.is_array())
            || results.len() > 1
            || self.service_descriptor.data_array.contains('[')
    }

    pub fn element_count(&self, json: &str) -> usize {
        self.roadwork_elements(json).len()
    }

    pub fn element_at(&self, json: &str, index: usize) -> Option<Value> {
        self.roadwork_elements(json).get(index).cloned()
    }

    /// Validates every descriptor path against every element of the data array.
    pub fn validate(&self, json: &str) -> Vec<PathValidation> {
        let elements = self.roadwork_elements(json);
        let descriptor = &self.service_descriptor;
        let mut report = Vec::with_capacity(8);

        let array_valid = self.roadwork_array_targets_array(json) && !elements.is_empty();
        report.push(PathValidation {
            label: "dataArray",
            path: descriptor.data_array.to_owned(),
            required: true,
            expected: "array of opendata",
            failures: if array_valid { Vec::new() } else { vec![0] },
            element_count: elements.len(),
            message: if array_valid {
                None
            } else {
                Some("must target a non-empty array of opendata")
            },
        });

        if elements.is_empty() {
            return report;
        }

        let count = elements.len();

        let id_failures = self.path_failures(&elements, |element| {
            self.path_matches_in(element, &descriptor.id, json_tools::is_scalar)
        });
        report.push(PathValidation::new(
            "id",
            &descriptor.id,
            true,
            "scalar",
            id_failures,
            count,
            None,
        ));

        let duplicate_failures = self.duplicate_id_failures(&elements);
        report.push(PathValidation::new(
            "id unique",
            &descriptor.id,
            true,
            "unique",
            duplicate_failures,
            count,
            None,
        ));

        report.extend(
            [
                self.optional_scalar_report(&elements, "latitude", descriptor.latitude.as_ref()),
                self.optional_scalar_report(&elements, "longitude", descriptor.longitude.as_ref()),
                self.optional_scalar_report(
                    &elements,
                    "description",
                    descriptor.description.as_ref(),
                ),
            ]
            .into_iter()
            .flatten(),
        );

        if let Some(polygon) = descriptor.polygon.as_ref().filter(|p| !p.trim().is_empty()) {
            let failures = self.optional_path_failures(&elements, |element| {
                self.path_matches_in(element, polygon, |value| {
                    json_tools::is_scalar(value) || value.is_array()
                })
            });
            report.push(PathValidation::new(
                "polygon",
                polygon,
                false,
                "scalar or array",
                failures,
                count,
                None,
            ));
        }

        report
    }

    pub(crate) fn optional_scalar_report(
        &self,
        elements: &[Value],
        label: &'static str,
        path: Option<&String>,
    ) -> Option<PathValidation> {
        let path = path.filter(|p| !p.trim().is_empty())?;
        let failures = self.optional_path_failures(elements, |element| {
            self.path_matches_in(element, path, json_tools::is_scalar)
        });
        Some(PathValidation::new(
            label,
            path,
            false,
            "scalar",
            failures,
            elements.len(),
            None,
        ))
    }

    fn path_failures<F>(&self, elements: &[Value], element_ok: F) -> Vec<usize>
    where
        F: Fn(&Value) -> bool,
    {
        let mut failures = Vec::new();
        for (index, element) in elements.iter().enumerate() {
            if !element_ok(element) {
                failures.push(index);
            }
        }
        failures
    }

    pub(crate) fn optional_path_failures<F>(&self, elements: &[Value], element_ok: F) -> Vec<usize>
    where
        F: Fn(&Value) -> bool,
    {
        if elements.iter().any(&element_ok) {
            Vec::new()
        } else {
            (0..elements.len()).collect()
        }
    }

    fn duplicate_id_failures(&self, elements: &[Value]) -> Vec<usize> {
        let mut occurrences: HashMap<String, Vec<usize>> = HashMap::new();
        for (index, element) in elements.iter().enumerate() {
            if let Ok(id) = element.get_path(&self.service_descriptor.id) {
                occurrences.entry(id).or_default().push(index);
            }
        }
        let mut failures: Vec<usize> = occurrences
            .values()
            .filter(|indices| indices.len() > 1)
            .flatten()
            .copied()
            .collect();
        failures.sort_unstable();
        failures
    }

    pub(crate) fn roadwork_elements(&self, json: &str) -> Vec<Value> {
        let Ok(value) = serde_json::from_str::<Value>(json) else {
            return Vec::new();
        };
        let Ok(results) = value.query(&self.service_descriptor.data_array) else {
            return Vec::new();
        };
        results.into_iter().cloned().collect()
    }

    pub fn path_points_to_scalar(&self, json: &str, path: &str) -> bool {
        self.path_matches(json, path, json_tools::is_scalar)
    }

    pub fn path_fetched_value(&self, json: &str, path: &str) -> Option<String> {
        if path.trim().is_empty() {
            return None;
        }
        self.roadwork_elements(json)
            .iter()
            .find_map(|element| self.path_fetched_value_in(element, path))
    }

    pub fn path_fetched_value_in(&self, element: &Value, path: &str) -> Option<String> {
        if path.trim().is_empty() {
            return None;
        }
        let found = element.query(path).ok()?;
        found
            .first()
            .map(|value| json_tools::format_fetched_value(value))
    }

    pub fn path_points_to_scalar_or_array(&self, json: &str, path: &str) -> bool {
        self.path_matches(json, path, |value| {
            json_tools::is_scalar(value) || value.is_array()
        })
    }

    pub fn path_points_to_scalar_in(&self, element: &Value, path: &str) -> bool {
        self.path_matches_in(element, path, json_tools::is_scalar)
    }

    pub fn path_points_to_scalar_or_array_in(&self, element: &Value, path: &str) -> bool {
        self.path_matches_in(element, path, |value| {
            json_tools::is_scalar(value) || value.is_array()
        })
    }

    fn path_matches<F>(&self, json: &str, path: &str, matches: F) -> bool
    where
        F: Fn(&Value) -> bool,
    {
        if path.trim().is_empty() {
            return true;
        }
        let elements = self.roadwork_elements(json);
        if elements.is_empty() {
            return true;
        }
        elements
            .iter()
            .any(|element| self.path_matches_in(element, path, &matches))
    }

    pub(crate) fn path_matches_in<F>(&self, element: &Value, path: &str, matches: F) -> bool
    where
        F: Fn(&Value) -> bool,
    {
        if path.trim().is_empty() {
            return true;
        }
        element
            .query(path)
            .map(|found| found.iter().any(|v| matches(v)))
            .unwrap_or(false)
    }

    pub fn parse_json(&self, json: &str) -> Result<OpendataData, MyError> {
        let json: serde_json::Value = serde_json::from_str(json)?;
        let data_array = json.query(&self.service_descriptor.data_array)?;
        info!("Found {} items", data_array.len());
        let mut opendata = Vec::with_capacity(data_array.len());
        for value in data_array {
            match self.build_opendata(value) {
                Ok(item) => {
                    if Self::is_valid(&item) {
                        opendata.push(item);
                    } else {
                        warn!("{item:?} is invalid");
                    }
                }
                Err(e) => warn!("Unable to build opendata {}", e),
            }
        }
        Ok(OpendataData::new(&self.service_name, opendata))
    }

    pub fn parse_json_preview(&self, json: &str) -> Result<OpendataData, MyError> {
        let json: serde_json::Value = serde_json::from_str(json)?;
        let data_array = json.query(&self.service_descriptor.data_array)?;
        let mut items = Vec::with_capacity(data_array.len());
        for (index, value) in data_array.into_iter().enumerate() {
            let mut item = self.build_opendata_preview(value);
            if item.id.is_empty() {
                item.id = format!("element#{index}");
            }
            items.push(item);
        }
        Ok(OpendataData::new(&self.service_name, items))
    }

    pub fn extract_roadwork_array(&self, json: &str) -> Result<Value, MyError> {
        let value: Value = serde_json::from_str(json)?;
        let results = value.query(&self.service_descriptor.data_array)?;
        Ok(Value::Array(results.into_iter().cloned().collect()))
    }

    pub fn build_url(&self) -> String {
        Self::build_url_with_params(&self.service_descriptor.metadata)
    }

    fn build_url_with_params(metadata: &Metadata) -> String {
        let (base, existing_query) = match metadata.url.split_once('?') {
            Some((base, query)) => (base, Some(query)),
            None => (metadata.url.as_str(), None),
        };

        let mut segments: Vec<String> = Vec::new();
        if let Some(query) = existing_query {
            for segment in query.split('&').filter(|segment| !segment.is_empty()) {
                segments.push(segment.to_string());
            }
        }
        let params: Vec<(&str, String)> = metadata
            .url_params
            .iter()
            .flatten()
            .map(|(key, value)| (key.as_str(), value.clone()))
            .collect();
        if params.is_empty() && segments.is_empty() {
            return metadata.url.clone();
        }
        let query_string = params
            .iter()
            .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
            .reduce(|acc, s| format!("{acc}&{s}"));
        if let Some(query_string) = query_string {
            segments.push(query_string);
        }
        if segments.is_empty() {
            return metadata.url.clone();
        }
        format!("{}?{}", base, segments.join("&"))
    }

    pub fn build_opendata(&self, node: &Value) -> Result<Opendata, MyError> {
        let mut opendata = Opendata {
            id: node.get_path(&self.service_descriptor.id)?,
            ..Opendata::default()
        };
        if let Some(latitude_path) = &self.service_descriptor.latitude
            && !latitude_path.is_empty()
        {
            opendata.latitude = node.get_path_as_double(latitude_path).unwrap_or(0.0);
        }
        if let Some(longitude_path) = &self.service_descriptor.longitude
            && !longitude_path.is_empty()
        {
            opendata.longitude = node.get_path_as_double(longitude_path).unwrap_or(0.0);
        }
        if let Some(polygon_path) = &self.service_descriptor.polygon
            && !polygon_path.is_empty()
        {
            opendata.polygons = node.get_path_as_polygons(polygon_path);
        }
        if let Some(description) = &self.service_descriptor.description {
            opendata.description = node.get_path(description).ok();
        }
        Ok(opendata)
    }

    pub fn build_opendata_preview(&self, node: &Value) -> Opendata {
        let mut opendata = Opendata {
            id: node
                .get_path(&self.service_descriptor.id)
                .unwrap_or_default(),
            ..Opendata::default()
        };
        if let Some(latitude_path) = &self.service_descriptor.latitude
            && !latitude_path.is_empty()
        {
            opendata.latitude = node.get_path_as_double(latitude_path).unwrap_or(0.0);
        }
        if let Some(longitude_path) = &self.service_descriptor.longitude
            && !longitude_path.is_empty()
        {
            opendata.longitude = node.get_path_as_double(longitude_path).unwrap_or(0.0);
        }
        if let Some(polygon_path) = &self.service_descriptor.polygon
            && !polygon_path.is_empty()
        {
            opendata.polygons = node.get_path_as_polygons(polygon_path);
        }
        if let Some(description) = &self.service_descriptor.description {
            opendata.description = node.get_path(description).ok();
        }
        opendata
    }

    fn is_valid(opendata: &Opendata) -> bool {
        if opendata.longitude == 0.0 && opendata.latitude == 0.0 {
            warn!("{opendata:?} is invalid because it has no location");
            return false;
        }
        true
    }
}

#[cfg(test)]
mod validate_tests {
    use super::*;
    use crate::opendata::json::model::opendata_service_descriptor::OpendataServiceDescriptor;

    fn descriptor() -> OpendataServiceDescriptor {
        serde_json::from_str::<OpendataServiceDescriptor>(
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
                "description": "$.fields.description"
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
                        "description": "Work",
                        "geo_shape": {"coordinates": [[[2.34,48.84],[2.36,48.86]]]}
                    }
                },
                {
                    "recordid": "abc",
                    "fields": {
                        "geo_shape": {"coordinates": null}
                    }
                }
            ]
        }"#
    }

    fn service() -> OpendataService {
        OpendataService {
            service_name: "test".to_string(),
            service_descriptor: descriptor(),
        }
    }

    #[test]
    fn validates_all_elements() {
        let report = service().validate(json());
        let get = |label: &str| report.iter().find(|p| p.label == label).unwrap();
        assert!(get("dataArray").is_valid());
        assert!(get("id").is_valid());
        assert_eq!(get("id unique").failures, vec![0, 1]);
        assert!(get("latitude").is_valid());
        assert!(get("longitude").is_valid());
        assert!(get("polygon").is_valid());
        assert!(get("description").is_valid());
    }

    #[test]
    fn optional_path_valid_when_matches_at_least_one() {
        let mut service_descriptor = descriptor();
        service_descriptor.description = Some("$.fields.description".to_string());
        let json = r#"{
            "records": [
                {"recordid": "1", "fields": {"description": "Work"}},
                {"recordid": "2", "fields": {}}
            ]
        }"#;
        let ods = OpendataService {
            service_name: "test".to_string(),
            service_descriptor,
        };
        let report = ods.validate(json);
        let get = |label: &str| report.iter().find(|p| p.label == label).unwrap();
        assert!(get("description").is_valid());
        assert_eq!(get("description").failures, Vec::<usize>::new());
    }

    #[test]
    fn optional_path_invalid_when_matches_none() {
        let mut service_descriptor = descriptor();
        service_descriptor.description = Some("$.fields.nonexistent".to_string());
        let json = r#"{
            "records": [
                {"recordid": "1", "fields": {"description": "Work"}},
                {"recordid": "2", "fields": {"description": "Other"}}
            ]
        }"#;
        let ods = OpendataService {
            service_name: "test".to_string(),
            service_descriptor,
        };
        let report = ods.validate(json);
        let get = |label: &str| report.iter().find(|p| p.label == label).unwrap();
        assert!(!get("description").is_valid());
        assert_eq!(get("description").failures, vec![0, 1]);
    }

    #[test]
    fn reports_bad_array_path() {
        let mut service_descriptor = descriptor();
        service_descriptor.data_array = "$.nope".to_string();
        let ods = OpendataService {
            service_name: "test".to_string(),
            service_descriptor,
        };
        let report = ods.validate(json());
        assert!(!report[0].is_valid());
        assert_eq!(report[0].label, "dataArray");
        assert_eq!(report.len(), 1);
    }

    #[test]
    fn build_url_appends_url_params() {
        let mut service_descriptor = descriptor();
        service_descriptor.metadata.url_params = Some(HashMap::from([(
            "app_token".to_string(),
            "abc".to_string(),
        )]));
        let ods = OpendataService {
            service_name: "test".to_string(),
            service_descriptor,
        };
        assert_eq!(ods.build_url(), "u?app_token=abc");
    }
}
