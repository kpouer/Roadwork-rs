use crate::country::model::{Country, CountryElement, CountryLevel, PolygonEncoding, Query};
use crate::http_service::HttpService;
use crate::json_tools::JsonTools;
use crate::model::wkt::polygon::Polygon;
use crate::opendata::json::model::lat_lng::LatLng;
use jsonpath_rust::JsonPath;
use jsonpath_rust::parser::errors::JsonPathError;
use log::info;
use serde_json::Value;
use thiserror::Error;

include!(concat!(env!("OUT_DIR"), "/countries.rs"));

/// Manages the countries and their administrative subdivision levels.
#[derive(Debug, Default)]
pub struct CountryManager {
    countries: Vec<Country>,
}

impl CountryManager {
    pub fn new() -> Self {
        Self {
            countries: Vec::new(),
        }
    }

    /// Loads the countries embedded at compile time from `resources/country/`.
    pub fn load() -> Self {
        let mut manager = Self::new();
        for (path, json) in COUNTRIES {
            match serde_json::from_str::<Country>(json) {
                Ok(country) => manager.countries.push(country),
                Err(e) => log::error!("Failed to parse country {path}: {e}"),
            }
        }
        manager
    }

    /// Parses a single country descriptor.
    pub fn parse(json: &str) -> Result<Country, CountryError> {
        serde_json::from_str(json).map_err(CountryError::from)
    }

    pub fn add(&mut self, country: Country) {
        self.countries.push(country);
    }

    pub fn countries(&self) -> &[Country] {
        &self.countries
    }

    pub fn get(&self, name: &str) -> Option<&Country> {
        self.countries.iter().find(|country| country.name == name)
    }

    pub fn level<'a>(&self, country: &'a Country, name: &str) -> Option<&'a CountryLevel> {
        country.levels.iter().find(|level| level.name == name)
    }

    /// Runs a query against a level of a country and parses the elements.
    pub async fn query(
        &self,
        level: &CountryLevel,
        query: Query,
    ) -> Result<Vec<CountryElement>, CountryError> {
        let url = build_url(level, &query)?;
        info!("query {url}");
        let json = HttpService.get_url(&url).await?;
        self.parse_response(level, &json)
    }

    pub async fn query_by_name(
        &self,
        level: &CountryLevel,
        name: &str,
    ) -> Result<Vec<CountryElement>, CountryError> {
        self.query(level, Query::Name(name.to_string())).await
    }

    pub async fn query_by_polygon(
        &self,
        level: &CountryLevel,
        polygon: &Polygon,
    ) -> Result<Vec<CountryElement>, CountryError> {
        self.query(level, Query::Polygon(polygon.clone())).await
    }

    pub async fn query_children(
        &self,
        level: &CountryLevel,
        parent_level: &str,
        parent_code: &str,
    ) -> Result<Vec<CountryElement>, CountryError> {
        self.query(
            level,
            Query::Children {
                parent_level: parent_level.to_string(),
                parent_code: parent_code.to_string(),
            },
        )
        .await
    }

    fn parse_response(
        &self,
        level: &CountryLevel,
        json: &str,
    ) -> Result<Vec<CountryElement>, CountryError> {
        let value: Value = serde_json::from_str(json)?;
        let mut elements = Vec::new();
        for node in value.query(&level.data_array)? {
            let mut element = CountryElement {
                name: node.get_path(&level.name_path).unwrap_or_default(),
                ..CountryElement::default()
            };
            if let Some(code_path) = level.code_path.as_deref().filter(|p| !p.is_empty()) {
                element.code = node.get_path(code_path).ok();
            }
            if let Some(polygon_path) = level.polygon_path.as_deref().filter(|p| !p.is_empty())
                && let Ok(found) = node.query(polygon_path)
                && let Some(geometry) = found.first()
            {
                element.polygons = parse_geojson_geometry(geometry);
            }
            if let Some(center_path) = level.center_path.as_deref().filter(|p| !p.is_empty())
                && let Ok(found) = node.query(center_path)
                && let Some(value) = found.first()
            {
                element.center = parse_center(value);
            }
            if let Some(parent_codes) = level.parent_codes.as_ref() {
                for (parent_level, path) in parent_codes {
                    if let Some(code) = node.get_path(path).ok().filter(|c| !c.is_empty()) {
                        element.parent_codes.insert(parent_level.clone(), code);
                    }
                }
            }
            elements.push(element);
        }
        Ok(elements)
    }
}

fn build_url(level: &CountryLevel, query: &Query) -> Result<String, CountryError> {
    let (param, value) = match query {
        Query::All => return Ok(level.url.clone()),
        Query::Name(name) => (
            level
                .name_param
                .as_deref()
                .ok_or_else(|| CountryError::MissingNameParam(level.name.clone()))?,
            name.clone(),
        ),
        Query::Code(code) => (
            level
                .code_param
                .as_deref()
                .ok_or_else(|| CountryError::MissingCodeParam(level.name.clone()))?,
            code.clone(),
        ),
        Query::Polygon(polygon) => (
            level
                .polygon_param
                .as_deref()
                .ok_or_else(|| CountryError::PolygonSearchUnsupported(level.name.clone()))?,
            encode_polygon(polygon, level.polygon_encoding),
        ),
        Query::Point { lat, lon } => {
            let lat_param = level
                .lat_param
                .as_deref()
                .ok_or_else(|| CountryError::PointSearchUnsupported(level.name.clone()))?;
            let lon_param = level
                .lon_param
                .as_deref()
                .ok_or_else(|| CountryError::PointSearchUnsupported(level.name.clone()))?;
            let separator = if level.url.contains('?') { '&' } else { '?' };
            return Ok(format!(
                "{level_url}{separator}{lat_param}={lat}{separator}{lon_param}={lon}",
                level_url = level.url,
                lat = urlencoding::encode(&format!("{lat}")),
                lon = urlencoding::encode(&format!("{lon}")),
            ));
        }
        Query::BBox {
            lat_min,
            lon_min,
            lat_max,
            lon_max,
        } => {
            let param = level
                .bbox_param
                .as_deref()
                .ok_or_else(|| CountryError::BBoxSearchUnsupported(level.name.clone()))?;
            let separator = if level.url.contains('?') { '&' } else { '?' };
            return Ok(format!(
                "{level_url}{separator}{param}={lat_min},{lon_min},{lat_max},{lon_max}",
                level_url = level.url,
            ));
        }
        Query::Children {
            parent_level,
            parent_code,
        } => {
            let param = level
                .parent_params
                .as_ref()
                .and_then(|params| params.get(parent_level))
                .ok_or_else(|| {
                    CountryError::UnknownParent(level.name.clone(), parent_level.clone())
                })?;
            (param.as_str(), parent_code.clone())
        }
    };
    let separator = if level.url.contains('?') { '&' } else { '?' };
    Ok(format!(
        "{}{separator}{param}={}",
        level.url,
        urlencoding::encode(&value)
    ))
}

fn encode_polygon(polygon: &Polygon, encoding: PolygonEncoding) -> String {
    match encoding {
        PolygonEncoding::GeoJson => encode_polygon_geojson(polygon),
    }
}

fn encode_polygon_geojson(polygon: &Polygon) -> String {
    let points: Vec<String> = polygon
        .xpoints
        .iter()
        .zip(polygon.ypoints.iter())
        .map(|(x, y)| format!("[{x},{y}]"))
        .collect();
    format!(
        "{{\"type\":\"Polygon\",\"coordinates\":[[{}]]}}",
        points.join(",")
    )
}

/// Extracts the polygons of a GeoJSON geometry object (`Polygon` or
/// `MultiPolygon`). Each polygon keeps its outer ring.
fn parse_geojson_geometry(geometry: &Value) -> Vec<Polygon> {
    let Some(geo_type) = geometry.get("type").and_then(Value::as_str) else {
        return Vec::new();
    };
    match geo_type {
        "Polygon" => coords_array(geometry)
            .and_then(|rings| rings.first())
            .and_then(|ring| ring_to_polygon(ring).ok())
            .into_iter()
            .collect(),
        "MultiPolygon" => coords_array(geometry)
            .into_iter()
            .flat_map(|polygons| {
                polygons
                    .iter()
                    .filter_map(|polygon| polygon.as_array().and_then(|rings| rings.first()))
                    .filter_map(|ring| ring_to_polygon(ring).ok())
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn coords_array(geometry: &Value) -> Option<&Vec<Value>> {
    geometry.get("coordinates").and_then(Value::as_array)
}

/// Extracts a representative point from a JsonPath result: either a
/// `[lon, lat]` array or a GeoJSON `Point` object.
fn parse_center(value: &Value) -> Option<LatLng> {
    match value {
        Value::Array(coords) if coords.len() >= 2 => Some(LatLng {
            lon: coords[0].as_f64()?,
            lat: coords[1].as_f64()?,
        }),
        Value::Object(_) if value.get("type").and_then(Value::as_str) == Some("Point") => {
            let coords = value.get("coordinates").and_then(Value::as_array)?;
            if coords.len() >= 2 {
                Some(LatLng {
                    lon: coords[0].as_f64()?,
                    lat: coords[1].as_f64()?,
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

fn ring_to_polygon(ring: &Value) -> Result<Polygon, ()> {
    let Value::Array(points) = ring else {
        return Err(());
    };
    let mut xpoints = Vec::with_capacity(points.len());
    let mut ypoints = Vec::with_capacity(points.len());
    for point in points {
        let Value::Array(coords) = point else {
            return Err(());
        };
        xpoints.push(coords.first().and_then(Value::as_f64).ok_or(())?);
        ypoints.push(coords.get(1).and_then(Value::as_f64).ok_or(())?);
    }
    Ok(Polygon { xpoints, ypoints })
}

#[derive(Error, Debug)]
pub enum CountryError {
    #[error(transparent)]
    JsonParseError(#[from] serde_json::Error),
    #[error(transparent)]
    JsonPathError(#[from] JsonPathError),
    #[error(transparent)]
    HttpError(#[from] reqwest::Error),
    #[error("Level '{0}' has no name query parameter")]
    MissingNameParam(String),
    #[error("Level '{0}' has no code query parameter")]
    MissingCodeParam(String),
    #[error("Level '{0}' has no polygon query parameter")]
    PolygonSearchUnsupported(String),
    #[error("Level '{0}' has no point (lat/lon) query parameters")]
    PointSearchUnsupported(String),
    #[error("Level '{0}' has no bbox query parameter")]
    BBoxSearchUnsupported(String),
    #[error("Level '{0}' has no '{1}' parent level")]
    UnknownParent(String, String),
}
