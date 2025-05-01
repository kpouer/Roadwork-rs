use crate::model::date_range::DateRange;
use crate::model::roadwork::{Roadwork, RoadworkBuilder};
use crate::model::roadwork_data::RoadworkData;
use crate::model::wkt::polygon::Polygon;
use crate::opendata::json::model::date_parser::DateParser;
use crate::opendata::json::model::date_result::DateResult;
use crate::opendata::json::model::service_descriptor::ServiceDescriptor;
use crate::service::http_service::HttpService;
use chrono::{Datelike, Timelike};
use jsonpath_rust::JsonPath;
use log::{debug, error, info, warn};
use roadwork_sync::SyncData;
use serde::Deserialize;
use serde_json::Value;
use crate::MyError;
use crate::MyError::{JsonParsingError, RoadworkBuilderError, RoadworkParsingError};

#[derive(Debug, Deserialize)]
pub(crate) struct OpendataService {
    serviceName: String,
    httpService: HttpService,
    pub(crate) serviceDescriptor: ServiceDescriptor,
}

impl OpendataService {
    pub(crate) fn new(service_name: String, service_descriptor: ServiceDescriptor) -> Self {
        Self {
            serviceName: service_name,
            serviceDescriptor: service_descriptor,
            httpService: HttpService::default(),
        }
    }
}

impl OpendataService {
    pub(crate) fn get_data(&self) -> Result<RoadworkData, MyError> {
        let url = self.buildUrl();
        info!("getData {url}");
        let json = self.httpService.getUrl(&url)?;
        self.parse_json(json)
    }

    fn parse_json(&self, json: String) -> Result<RoadworkData, MyError> {
        let json: serde_json::Value = serde_json::from_str(&json).unwrap();
        let roadwork_array = json
            .query(&self.serviceDescriptor.roadworkArray)?;
        let roadworks = roadwork_array
            .into_iter()
            .flat_map(|node| self.buildRoadwork(node))
            .filter(|roadwork| Self::isValid(roadwork))
            .collect();
        Ok(RoadworkData::new(&self.serviceName, roadworks))
    }

    fn buildUrl(&self) -> String {
        let metadata = &self.serviceDescriptor.metadata;

        match &metadata.urlParams {
            None => metadata.url.clone(),
            Some(urlParams) => {
                let query_string = urlParams
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

    fn isValid(roadwork: &Roadwork) -> bool {
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

    fn buildRoadwork(&self, node: &Value) -> Result<Roadwork, MyError> {
        let mut roadworkBuilder = RoadworkBuilder::default();
        roadworkBuilder.syncData(Some(SyncData::default()));
        let id = Self::get_path(node, &self.serviceDescriptor.id)?;
        roadworkBuilder.id(id);
        let latitude_path = match &self.serviceDescriptor.latitude {
            None => return Err(RoadworkParsingError(format!("Unable to get latitude from {}", node))),
            Some(latitudePath) => {
                if latitudePath.is_empty() {
                    return Err(RoadworkParsingError(format!("Unable to get latitude from {}", node)));
                } else {
                    Some(Self::getPathAsDouble(node, latitudePath)?)
                }
            }
        };
        if let Some(latitude_path) = latitude_path {
            roadworkBuilder.latitude(latitude_path);
        } else {
            warn!("Unable to get latitude as it's path is empty");
        }
        let longitude_path = match &self.serviceDescriptor.longitude {
            None => return Err(RoadworkParsingError(format!("Unable to get latitude from {}", node))),
            Some(longitude_path) => {
                if longitude_path.is_empty() {
                    return Err(RoadworkParsingError(format!("Unable to get latitude from {}", node)));
                } else {
                    Some(Self::getPathAsDouble(node, longitude_path)?)
                }
            }
        };
        if let Some(longitude_path) = longitude_path {
            roadworkBuilder.longitude(longitude_path);
        } else {
            warn!("Unable to get longitude as it's path is empty");
        }
        if let Some(polygon_path) = &self.serviceDescriptor.polygon {
            if !polygon_path.is_empty() {
                roadworkBuilder.polygons(self.getPathAsPolygons(node, polygon_path));
            }
        }
        if let Some(road) = &self.serviceDescriptor.road {
            roadworkBuilder.road(Self::get_path(node, road)?);
        }
        if let Some(description) = &self.serviceDescriptor.description {
            roadworkBuilder.description(Self::get_path(node, description)?);
        }

        if let Some(locationDetails) = &self.serviceDescriptor.locationDetails {
            roadworkBuilder.locationDetails(Self::get_path(node, locationDetails)?);
        }
        let dateRange = self.getDateRange(node);
        roadworkBuilder.start(dateRange.from as u64);
        roadworkBuilder.end(dateRange.to as u64);
        if let Some(impactCirculationDetail) = &self.serviceDescriptor.impactCirculationDetail {
            roadworkBuilder.impactCirculationDetail(Self::get_path(node, impactCirculationDetail)?);
        }
        if let Some(locationDetails) = &self.serviceDescriptor.locationDetails {
            roadworkBuilder.locationDetails(Self::get_path(node, locationDetails)?);
        }
        if let Some(url) = &self.serviceDescriptor.url {
            roadworkBuilder.url(Self::get_path(node, url)?);
        }
        roadworkBuilder.build().map_err(RoadworkBuilderError)
    }

    fn parseDate(
        &self,
        node: &Value,
        dateParser: &Option<DateParser>,
    ) -> Result<DateResult, MyError> {
        if dateParser.is_none() {
            return Err(MyError::ParsingError("Cannot parse date as dateParse is null".to_string()));
        }
        let currentYear = chrono::Local::now().year();
        let dateParser = dateParser.as_ref().unwrap();
        let value = Self::get_path(node, &dateParser.path)?;
        let mut result = dateParser.parse(&value, self.serviceDescriptor.metadata.getLocale())?;
        if result.parser.resetHour {
            result.date = Self::fixTime(result.date);
        }
        if result.parser.addYear {
            result.date = Self::addYear(currentYear, result.date);
        }
        Ok(result)
    }

    fn addYear(year: i32, date: i64) -> i64 {
        chrono::DateTime::from_timestamp_millis(date)
            .unwrap()
            .with_year(year)
            .unwrap()
            .timestamp_millis()
    }

    fn fixTime(date: i64) -> i64 {
        chrono::DateTime::from_timestamp_millis(date)
            .unwrap()
            .with_hour(0)
            .unwrap()
            .with_minute(0)
            .unwrap()
            .with_second(0)
            .unwrap()
            .with_nanosecond(0)
            .unwrap()
            .timestamp_millis()
    }

    fn getDateRange(&self, node: &Value) -> DateRange {
        let currentYear = chrono::Local::now().year();
        let startTime = self
            .parseDate(node, &self.serviceDescriptor.from)
            .map(|date_result| date_result.date)
            .inspect_err(|e| error!("Error parsing start date {}", e))
            .unwrap_or_default();
        let mut endDate;
        match self.parseDate(node, &self.serviceDescriptor.to) {
            Ok(end) => {
                endDate = end.date;
                if end.parser.addYear {
                    endDate = Self::addYear(currentYear, endDate);
                    if startTime > endDate {
                        endDate = Self::addYear(currentYear + 1, endDate);
                    }
                }
            }
            Err(e) => {
                error!("Error parsing end date {}", e);
                endDate = 0
            }
        }

        DateRange::new(startTime, endDate)
    }

    fn getPathAsPolygons(&self, node: &Value, path: &str) -> Option<Vec<Polygon>> {
        match node.query(path) {
            Ok(value) => {
                if Self::isMultiPolygon(&value) {
                    let polygons = value
                        .into_iter()
                        .flat_map(|polygon_array| {
                            if let Value::Array(polygonArray) = polygon_array {
                                Some(Self::getPolygon(polygonArray))
                            } else {
                                None
                            }
                        })
                        .collect();
                    return Some(polygons);
                } else {
                    if let Some(Value::Array(polygonArray)) = value.get(0) {
                        return Some(vec![Self::getPolygon(polygonArray)]);
                    }
                }
            }
            Err(e) => error!("Error parsing polygon {e}"),
        }
        None
    }

    fn isMultiPolygon(value: &Vec<&Value>) -> bool {
        if let Some(Value::Array(firstLevel)) = value.get(0) {
            if let Some(Value::Array(_)) = firstLevel.get(0) {
                return true;
            }
        }
        false
    }

    fn getPolygon(polygonArray: &Vec<Value>) -> Polygon {
        let mut xpoints = Vec::with_capacity(polygonArray.len());
        let mut ypoints = Vec::with_capacity(polygonArray.len());
        for point in polygonArray {
            xpoints.push(point[0].as_f64().unwrap()); // todo clean this unwrap
            ypoints.push(point[1].as_f64().unwrap());
        }
        Polygon::new(xpoints, ypoints)
    }

    fn get_path(node: &Value, path: &str) -> Result<String, MyError> {
        debug!("get_path path:{path}");
        let result = node.query(path)?;
        if result.is_empty() {
            return Err(JsonParsingError(format!("Unable to get path {} from {}", path, node)));
        }
        result[0]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| JsonParsingError(format!("Unable to get path {} from {}", path, node)))
    }

    fn getPathAsDouble(node: &Value, path: &str) -> Result<f64, MyError> {
        let result = node.query(path)?;
        if result.is_empty() {
            return Err(JsonParsingError(format!("Unable to get path {} from {}", path, node)));
        }
        let value = result[0];
        match value {
            Value::Number(number) => Ok(number.as_f64().unwrap()),
            Value::String(string) => string
                .parse::<f64>()
                .or(Err(JsonParsingError(format!("Unable to parse {} as a double", string)))),
            _ => Err(JsonParsingError(format!("Unable to get path {} from {}", path, node))),
        }
    }
}
