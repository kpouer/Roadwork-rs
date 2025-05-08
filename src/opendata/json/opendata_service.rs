use crate::json_tools::JsonTools;
use crate::model::date_range::DateRange;
use crate::model::roadwork::Roadwork;
use crate::model::roadwork_data::RoadworkData;
use crate::opendata::json::model::date_parser::DateParser;
use crate::opendata::json::model::date_result::DateResult;
use crate::opendata::json::model::service_descriptor::ServiceDescriptor;
use crate::service::http_service::HttpService;
use crate::MyError;
use crate::MyError::RoadworkParsingError;
use chrono::{DateTime, Datelike, Timelike};
use chrono_tz::Tz;
use jsonpath_rust::JsonPath;
use log::{error, info, warn};
use serde_json::Value;
use std::fs;

#[derive(Debug)]
pub(crate) struct OpendataService {
    service_name: String,
    http_service: HttpService,
    pub(crate) service_descriptor: ServiceDescriptor,
}

impl OpendataService {
    pub(crate) fn new(service_name: String, service_descriptor: ServiceDescriptor) -> Self {
        Self {
            service_name,
            service_descriptor,
            http_service: HttpService,
        }
    }
}

impl OpendataService {
    pub(crate) fn get_data(&self) -> Result<RoadworkData, MyError> {
        let url = self.build_url();
        info!("getData {url}");
        let json = if cfg!(debug_assertions) {
            fs::read_to_string("test/example.json").expect("Unable to read file")
        } else {
            self.http_service.get_url(&url)?
        };
        self.parse_json(&json)
    }

    fn parse_json(&self, json: &str) -> Result<RoadworkData, MyError> {
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

    fn build_url(&self) -> String {
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
                    format!("{}&{}", &metadata.url, query_string)
                } else {
                    format!("{}?{}", &metadata.url, query_string)
                }
            }
        }
    }

    fn is_valid(roadwork: &Roadwork) -> bool {
        if roadwork.longitude == 0.0 && roadwork.latitude == 00.0 {
            warn!("{roadwork:?} is invalid because it has no location");
            return false;
        }
        //        if (roadwork.getStart() == 0) {
        //            logger.warn("{} is invalid because it's start date is 0", roadwork);
        //            return false;
        //        }
        //        if (roadwork.getEnd() == 0) {
        //            logger.warn("{} is invalid because it's end date is 0", roadwork);
        //            return false;
        //        }
        true
    }

    fn build_roadwork(&self, node: &Value) -> Result<Roadwork, MyError> {
        let mut roadwork_builder = Roadwork::default();
        roadwork_builder.id = node.get_path(&self.service_descriptor.id)?;
        let latitude_path = match &self.service_descriptor.latitude {
            None => {
                return Err(RoadworkParsingError(format!(
                    "Unable to get latitude from {}",
                    node
                )));
            }
            Some(latitude_path) => {
                if latitude_path.is_empty() {
                    return Err(RoadworkParsingError(format!(
                        "Unable to get latitude from {}",
                        node
                    )));
                } else {
                    Some(node.get_path_as_double(latitude_path)?)
                }
            }
        };
        if let Some(latitude_path) = latitude_path {
            roadwork_builder.latitude = latitude_path;
        } else {
            warn!("Unable to get latitude as it's path is empty");
        }
        let longitude_path = match &self.service_descriptor.longitude {
            None => {
                return Err(RoadworkParsingError(format!(
                    "Unable to get latitude from {}",
                    node
                )));
            }
            Some(longitude_path) => {
                if longitude_path.is_empty() {
                    return Err(RoadworkParsingError(format!(
                        "Unable to get latitude from {}",
                        node
                    )));
                } else {
                    Some(node.get_path_as_double(longitude_path)?)
                }
            }
        };
        if let Some(longitude_path) = longitude_path {
            roadwork_builder.longitude = longitude_path;
        } else {
            warn!("Unable to get longitude as it's path is empty");
        }
        if let Some(polygon_path) = &self.service_descriptor.polygon {
            if !polygon_path.is_empty() {
                roadwork_builder.polygons = node.get_path_as_polygons(polygon_path);
            }
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
        if let Some(impact_circulation_detail) = &self.service_descriptor.impact_circulation_detail {
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
        let current_year = chrono::Local::now().year();
        let date_parser = date_parser.as_ref().unwrap();
        let value = node.get_path(&date_parser.path)?;
        let mut result = date_parser.parse(&value, self.service_descriptor.metadata.get_locale())?;
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
        let current_year = chrono::Local::now().year();
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
