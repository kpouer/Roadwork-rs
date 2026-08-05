use crate::MyError;
use crate::http_service::HttpService;
use crate::json_tools::JsonTools;
use crate::model::date_range::DateRange;
use crate::model::roadwork::Roadwork;
use crate::model::roadwork_data::RoadworkData;
use crate::opendata::json::model::date_parser::DateParser;
use crate::opendata::json::model::date_result::DateResult;
use crate::opendata::json::model::service_descriptor::ServiceDescriptor;
use chrono::{DateTime, Datelike, Timelike};
use chrono_tz::Tz;
use jsonpath_rust::JsonPath;
use log::{error, info, warn};
use serde_json::Value;

#[derive(Debug)]
pub struct OpendataService {
    pub service_name: String,
    http_service: HttpService,
    pub service_descriptor: ServiceDescriptor,
}

impl OpendataService {
    pub fn new(service_name: String, service_descriptor: ServiceDescriptor) -> Self {
        Self {
            service_name,
            service_descriptor,
            http_service: HttpService,
        }
    }
}

pub fn find_json_arrays(json: &str) -> Vec<(String, usize)> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let mut arrays = Vec::new();
    collect_arrays(&value, "$", &mut arrays);
    arrays
}

fn collect_arrays(value: &Value, path: &str, arrays: &mut Vec<(String, usize)>) {
    match value {
        Value::Array(elements) => {
            arrays.push((path.to_string(), elements.len()));
            for (i, element) in elements.iter().enumerate() {
                if element.is_array() || element.is_object() {
                    collect_arrays(element, &format!("{path}[{i}]"), arrays);
                }
            }
        }
        Value::Object(map) => {
            for (key, child) in map {
                if child.is_array() || child.is_object() {
                    let child_path = if is_plain_key(key) {
                        format!("{path}.{key}")
                    } else {
                        format!("{path}[\"{key}\"]")
                    };
                    collect_arrays(child, &child_path, arrays);
                }
            }
        }
        _ => {}
    }
}

fn is_plain_key(key: &str) -> bool {
    !key.is_empty()
        && !key.chars().next().is_some_and(|c| c.is_ascii_digit())
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

const MAX_ARRAY_INDEX: usize = 8;
const MAX_ARRAY_DEPTH: usize = 4;
const MAX_SCALAR_PATHS: usize = 200;

pub fn find_element_scalar_paths(json: &str, array_path: &str) -> Vec<(String, String)> {
    let Some(element) = first_element(json, array_path) else {
        return Vec::new();
    };
    let mut scalars = Vec::new();
    collect_scalar_leaves(&element, "$", &mut scalars);
    scalars.sort();
    scalars
}

pub fn find_element_array_paths(json: &str, array_path: &str) -> Vec<(String, usize)> {
    let Some(element) = first_element(json, array_path) else {
        return Vec::new();
    };
    let mut arrays = Vec::new();
    collect_element_arrays(&element, "$", &mut arrays, 0);
    arrays.sort();
    arrays
}

fn first_element(json: &str, array_path: &str) -> Option<Value> {
    if array_path.trim().is_empty() {
        return None;
    }
    let value = serde_json::from_str::<Value>(json).ok()?;
    let results = value.query(array_path).ok()?;
    results.into_iter().next().cloned()
}

fn collect_scalar_leaves(value: &Value, path: &str, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if out.len() >= MAX_SCALAR_PATHS {
                    return;
                }
                let child_path = if is_plain_key(key) {
                    format!("{path}.{key}")
                } else {
                    format!("{path}[\"{key}\"]")
                };
                collect_scalar_leaves(child, &child_path, out);
            }
        }
        Value::Array(elements) => {
            for (i, element) in elements.iter().enumerate().take(MAX_ARRAY_INDEX) {
                if out.len() >= MAX_SCALAR_PATHS {
                    return;
                }
                collect_scalar_leaves(element, &format!("{path}[{i}]"), out);
            }
        }
        _ => out.push((path.to_string(), format_fetched_value(value))),
    }
}

fn collect_element_arrays(
    value: &Value,
    path: &str,
    arrays: &mut Vec<(String, usize)>,
    depth: usize,
) {
    if depth > MAX_ARRAY_DEPTH {
        return;
    }
    match value {
        Value::Array(elements) => {
            arrays.push((path.to_string(), elements.len()));
            for (i, element) in elements.iter().enumerate().take(MAX_ARRAY_INDEX) {
                if element.is_array() || element.is_object() {
                    collect_element_arrays(element, &format!("{path}[{i}]"), arrays, depth + 1);
                }
            }
        }
        Value::Object(map) => {
            for (key, child) in map {
                if child.is_array() || child.is_object() {
                    let child_path = if is_plain_key(key) {
                        format!("{path}.{key}")
                    } else {
                        format!("{path}[\"{key}\"]")
                    };
                    collect_element_arrays(child, &child_path, arrays, depth + 1);
                }
            }
        }
        _ => {}
    }
}

impl OpendataService {
    pub async fn get_data(&self) -> Result<RoadworkData, MyError> {
        let url = self.build_url();
        info!("getData {url}");
        let json = self.http_service.get_url(&url).await?;
        self.parse_json(&json)
    }

    pub fn roadwork_array_targets_array(&self, json: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(json) else {
            return false;
        };
        let Ok(results) = value.query(&self.service_descriptor.roadwork_array) else {
            return false;
        };
        if results.is_empty() {
            return false;
        }
        results.iter().any(|v| v.is_array())
            || results.len() > 1
            || self.service_descriptor.roadwork_array.contains('[')
    }

    pub fn path_points_to_scalar(&self, json: &str, path: &str) -> bool {
        self.path_matches(json, path, is_scalar)
    }

    pub fn path_fetched_value(&self, json: &str, path: &str) -> Option<String> {
        if path.trim().is_empty() {
            return None;
        }
        let Ok(value) = serde_json::from_str::<Value>(json) else {
            return None;
        };
        let Ok(results) = value.query(&self.service_descriptor.roadwork_array) else {
            return None;
        };
        for node in results.iter() {
            if let Ok(found) = node.query(path)
                && let Some(first) = found.first()
            {
                return Some(format_fetched_value(first));
            }
        }
        None
    }

    pub fn path_points_to_scalar_or_array(&self, json: &str, path: &str) -> bool {
        self.path_matches(json, path, |value| is_scalar(value) || value.is_array())
    }

    fn path_matches<F>(&self, json: &str, path: &str, matches: F) -> bool
    where
        F: Fn(&Value) -> bool,
    {
        if path.trim().is_empty() {
            return true;
        }
        let Ok(value) = serde_json::from_str::<Value>(json) else {
            return false;
        };
        let Ok(results) = value.query(&self.service_descriptor.roadwork_array) else {
            return false;
        };
        if results.is_empty() {
            return true;
        }
        results.iter().any(|node| {
            node.query(path)
                .map(|found| found.iter().any(|v| matches(v)))
                .unwrap_or(false)
        })
    }

    pub fn parse_json(&self, json: &str) -> Result<RoadworkData, MyError> {
        let json: serde_json::Value = serde_json::from_str(json)?;
        let roadwork_array = json.query(&self.service_descriptor.roadwork_array)?;
        info!("Found {} roadworks", roadwork_array.len());
        let mut roadworks = Vec::with_capacity(roadwork_array.len());
        for value in roadwork_array {
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

    pub fn build_url(&self) -> String {
        let metadata = &self.service_descriptor.metadata;

        match &metadata.url_params {
            None => metadata.url.clone(),
            Some(url_params) => {
                let query_string = url_params
                    .iter()
                    .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
                    .reduce(|acc, s| format!("{acc}&{s}"))
                    .unwrap_or_default();
                if metadata.url.contains("?") {
                    format!("{}&{}", metadata.url, query_string)
                } else {
                    format!("{}?{}", metadata.url, query_string)
                }
            }
        }
    }

    fn is_valid(roadwork: &Roadwork) -> bool {
        if roadwork.longitude == 0.0 && roadwork.latitude == 0.0 {
            warn!("{roadwork:?} is invalid because it has no location");
            return false;
        }
        true
    }

    fn build_roadwork(&self, node: &Value) -> Result<Roadwork, MyError> {
        let mut roadwork_builder = Roadwork {
            id: node.get_path(&self.service_descriptor.id)?,
            ..Roadwork::default()
        };
        if let Some(latitude_path) = &self.service_descriptor.latitude
            && !latitude_path.is_empty()
        {
            roadwork_builder.latitude = node.get_path_as_double(latitude_path).unwrap_or(0.0);
        }
        if let Some(longitude_path) = &self.service_descriptor.longitude
            && !longitude_path.is_empty()
        {
            roadwork_builder.longitude = node.get_path_as_double(longitude_path).unwrap_or(0.0);
        }
        if let Some(polygon_path) = &self.service_descriptor.polygon
            && !polygon_path.is_empty()
        {
            roadwork_builder.polygons = node.get_path_as_polygons(polygon_path);
        }
        if let Some(road) = &self.service_descriptor.road {
            roadwork_builder.road = node.get_path(road).ok();
        }
        if let Some(description) = &self.service_descriptor.description {
            roadwork_builder.description = node.get_path(description).ok();
        }

        if let Some(location_details) = &self.service_descriptor.location_details {
            roadwork_builder.location_details = node.get_path(location_details).ok();
        }
        let date_range = self.get_date_range(node)?;
        roadwork_builder.start = date_range.from.timestamp_millis();
        roadwork_builder.end = date_range
            .to
            .map(|date| date.timestamp_millis())
            .unwrap_or(0);
        if let Some(impact_circulation_detail) = &self.service_descriptor.impact_circulation_detail
        {
            roadwork_builder.impact_circulation_detail =
                node.get_path(impact_circulation_detail).ok();
        }
        if let Some(url) = &self.service_descriptor.url {
            roadwork_builder.url = node.get_path(url)?;
        }
        Ok(roadwork_builder)
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
                Ok(DateRange::new(start_time, end_date))
            }
            Err(e) => {
                error!("Error parsing end date {}", e);
                Ok(DateRange::without_end(start_time))
            }
        }
    }
}

fn is_scalar(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

fn format_fetched_value(value: &Value) -> String {
    let text = match value {
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    };
    if text.len() > 200 {
        format!("{}…", &text[..200])
    } else {
        text
    }
}
