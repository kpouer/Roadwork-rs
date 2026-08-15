use crate::http_service::HttpService;
use crate::json_tools::JsonTools;
use crate::model::date_range::DateRange;
use crate::model::opendata::Opendata;
use crate::model::opendata_data::OpendataData;
use crate::model::roadwork::Roadwork;
use crate::model::roadwork_data::RoadworkData;
use crate::opendata::json::model::date_parser::DateParser;
use crate::opendata::json::model::date_result::DateResult;
use crate::opendata::json::model::metadata::Metadata;
use crate::opendata::json::model::service_descriptor::ServiceDescriptor;
use crate::opendata::json::path_validation::PathValidation;
use crate::{MyError, json_tools};
use chrono::{DateTime, Datelike, Timelike};
use chrono_tz::Tz;
use jsonpath_rust::JsonPath;
use log::{error, info, warn};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug)]
pub struct OpendataService {
    pub service_name: String,
    pub service_descriptor: ServiceDescriptor,
}

impl From<&ServiceDescriptor> for OpendataService {
    fn from(service_descriptor: &ServiceDescriptor) -> Self {
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

    pub async fn get_roadworks_data(&self) -> Result<RoadworkData, MyError> {
        let url = self.build_url();
        info!("getData {url}");
        let json = HttpService.get_url(&url).await?;
        self.parse_roadworks(&json)
    }

    pub fn roadwork_array_targets_array(&self, json: &str) -> bool {
        let Ok(value) = serde_json::from_str::<Value>(json) else {
            return false;
        };
        self.roadwork_array_targets_array_value(&value)
    }

    pub fn roadwork_array_targets_array_value(&self, value: &Value) -> bool {
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
        let Ok(value) = serde_json::from_str::<Value>(json) else {
            return 0;
        };
        self.element_count_value(&value)
    }

    pub fn element_count_value(&self, value: &Value) -> usize {
        self.roadwork_array(value)
            .map(|array| array.len())
            .unwrap_or(0)
    }

    pub fn element_at(&self, json: &str, index: usize) -> Option<Value> {
        self.roadwork_elements(json).get(index).cloned()
    }

    pub fn element_at_value(&self, value: &Value, index: usize) -> Option<Value> {
        self.roadwork_array(value)
            .ok()
            .and_then(|array| array.get(index).map(|v| (*v).clone()))
    }

    /// Validates every descriptor path against every element of the data array.
    pub fn validate(&self, json: &str) -> Vec<PathValidation> {
        let elements = self.roadwork_elements(json);
        let descriptor = &self.service_descriptor;
        let mut report = Vec::with_capacity(8);

        let array_valid = self.roadwork_array_targets_array(json) && !elements.is_empty();
        report.push(PathValidation {
            label: "data_array",
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

    /// Validates every descriptor path — including the roadwork-specific fields
    /// (road, url, locationDetails, impactCirculationDetail, dates) — against
    /// every element of the roadwork array.
    pub fn validate_roadworks(&self, json: &str) -> Vec<PathValidation> {
        let mut report = self.validate(json);
        let elements = self.roadwork_elements(json);
        if elements.is_empty() {
            return report;
        }

        let count = elements.len();
        let descriptor = &self.service_descriptor;

        report.extend(
            [
                self.optional_scalar_report(&elements, "road", descriptor.road.as_ref()),
                self.optional_scalar_report(
                    &elements,
                    "locationDetails",
                    descriptor.location_details.as_ref(),
                ),
                self.optional_scalar_report(
                    &elements,
                    "impactCirculationDetail",
                    descriptor.impact_circulation_detail.as_ref(),
                ),
            ]
            .into_iter()
            .flatten(),
        );

        let locale = descriptor.metadata.get_locale();

        if let Some(date) = &descriptor.from {
            self.validate_date(&mut report, &elements, count, locale, "from", date);
        }
        if let Some(date) = &descriptor.to {
            self.validate_date(&mut report, &elements, count, locale, "from", date);
        }

        report
    }

    fn validate_date(
        &self,
        report: &mut Vec<PathValidation>,
        elements: &Vec<Value>,
        count: usize,
        locale: Tz,
        label: &str,
        date: &DateParser,
    ) {
        if date.path.trim().is_empty() {}
        let failures = self.optional_path_failures(&elements, |element| {
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
        self.roadwork_elements_value(&value)
    }

    pub(crate) fn roadwork_elements_value(&self, value: &Value) -> Vec<Value> {
        self.roadwork_array(value)
            .map(|array| array.into_iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Returns the elements matched by `data_array`, borrowing from the document.
    ///
    /// This performs a single `jsonpath` query, avoiding the full-element cloning
    /// done by [`Self::roadwork_elements_value`]. It is safe to reuse the returned
    /// slice while the document stays alive.
    pub fn roadwork_array<'a>(&self, value: &'a Value) -> Result<Vec<&'a Value>, MyError> {
        Ok(value.query(&self.service_descriptor.data_array)?)
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
        let value: serde_json::Value = serde_json::from_str(json)?;
        self.parse_value(&value)
    }

    pub fn parse_value(&self, value: &Value) -> Result<OpendataData, MyError> {
        self.parse_value_preview(value, usize::MAX)
    }

    /// Parses at most `limit` elements of the data array, used for previews.
    pub fn parse_value_preview(
        &self,
        value: &Value,
        limit: usize,
    ) -> Result<OpendataData, MyError> {
        let data_array = value.query(&self.service_descriptor.data_array)?;
        info!("Found {} items", data_array.len());
        let mut opendata = Vec::with_capacity(limit.min(data_array.len()));
        for value in data_array.into_iter().take(limit) {
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
        let value: serde_json::Value = serde_json::from_str(json)?;
        let data_array = value.query(&self.service_descriptor.data_array)?;
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
        self.extract_value(&value)
    }

    pub fn extract_value(&self, value: &Value) -> Result<Value, MyError> {
        let results = value.query(&self.service_descriptor.data_array)?;
        Ok(Value::Array(results.into_iter().cloned().collect()))
    }

    pub fn build_url(&self) -> String {
        Self::build_url_with_params(&self.service_descriptor.metadata)
    }

    fn build_url_with_params(metadata: &Metadata) -> String {
        let Some(url) = metadata.url.as_deref() else {
            return String::new();
        };
        let (base, existing_query) = match url.split_once('?') {
            Some((base, query)) => (base, Some(query)),
            None => (url, None),
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
            return url.to_string();
        }
        let query_string = params
            .iter()
            .map(|(key, value)| format!("{key}={}", urlencoding::encode(value)))
            .reduce(|acc, s| format!("{acc}&{s}"));
        if let Some(query_string) = query_string {
            segments.push(query_string);
        }
        if segments.is_empty() {
            return url.to_string();
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

    pub fn is_valid(opendata: &Opendata) -> bool {
        if opendata.longitude == 0.0 && opendata.latitude == 0.0 {
            warn!("{opendata:?} is invalid because it has no location");
            return false;
        }
        true
    }

    pub fn parse_roadworks(&self, json: &str) -> Result<RoadworkData, MyError> {
        let json: serde_json::Value = serde_json::from_str(json)?;
        let data_array = json.query(&self.service_descriptor.data_array)?;
        info!("Found {} items", data_array.len());
        let mut roadworks = Vec::with_capacity(data_array.len());
        for value in data_array {
            match self.build_roadwork(value) {
                Ok(roadwork) => {
                    if Self::is_valid_roadwork(&roadwork) {
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

    pub fn parse_roadworks_preview(&self, json: &str) -> Result<RoadworkData, MyError> {
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

    fn is_valid_roadwork(roadwork: &Roadwork) -> bool {
        if roadwork.opendata.longitude == 0.0 && roadwork.opendata.latitude == 0.0 {
            warn!("{roadwork:?} is invalid because it has no location");
            return false;
        }
        true
    }

    fn build_roadwork(&self, node: &Value) -> Result<Roadwork, MyError> {
        let mut roadwork_builder = Roadwork {
            opendata: self.build_opendata(node)?,
            ..Roadwork::default()
        };
        self.fill_roadwork_fields(node, &mut roadwork_builder);
        let date_range = self.get_date_range(node)?;
        roadwork_builder.start = date_range.from.timestamp_millis();
        roadwork_builder.end = date_range
            .to
            .map(|date| date.timestamp_millis())
            .unwrap_or(0);
        Ok(roadwork_builder)
    }

    fn build_roadwork_preview(&self, node: &Value) -> Roadwork {
        let mut roadwork_builder = Roadwork {
            opendata: self.build_opendata_preview(node),
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
