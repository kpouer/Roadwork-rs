use crate::model::wkt::polygon::Polygon;
use crate::opendata::json::model::lat_lng::LatLng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A country and its administrative subdivision levels.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Country {
    pub name: String,
    #[serde(rename = "isoCode", skip_serializing_if = "Option::is_none")]
    pub iso_code: Option<String>,
    #[serde(rename = "sourceUrl", skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer: Option<String>,
    #[serde(rename = "licenceName", skip_serializing_if = "Option::is_none")]
    pub licence_name: Option<String>,
    #[serde(rename = "licenceUrl", skip_serializing_if = "Option::is_none")]
    pub licence_url: Option<String>,
    pub levels: Vec<CountryLevel>,
}

/// One administrative level of a country (région, département, commune, ...).
///
/// The list of levels is free: each country defines its own hierarchy. Every
/// level can be queried through a REST API by name, by polygon or as the
/// children of a parent level.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CountryLevel {
    pub name: String,
    pub url: String,
    /// Query parameter used to search by name (e.g. `nom` on geo.api.gouv.fr).
    #[serde(rename = "nameParam", skip_serializing_if = "Option::is_none")]
    pub name_param: Option<String>,
    /// Query parameters used to search by point (e.g. `lat`/`lon` on
    /// geo.api.gouv.fr). Optional: only levels supporting a spatial point
    /// search define them.
    #[serde(rename = "latParam", skip_serializing_if = "Option::is_none")]
    pub lat_param: Option<String>,
    #[serde(rename = "lonParam", skip_serializing_if = "Option::is_none")]
    pub lon_param: Option<String>,
    /// Query parameter receiving a bounding box (`bbox`). Optional: only APIs
    /// supporting a rectangle search define it.
    #[serde(rename = "bboxParam", skip_serializing_if = "Option::is_none")]
    pub bbox_param: Option<String>,
    /// Query parameter receiving a polygon (e.g. `geom`). Optional: some APIs
    /// (geo.api.gouv.fr) do not support polygon search.
    #[serde(rename = "polygonParam", skip_serializing_if = "Option::is_none")]
    pub polygon_param: Option<String>,
    /// How the polygon is encoded when sent to the API.
    #[serde(rename = "polygonEncoding", default)]
    pub polygon_encoding: PolygonEncoding,
    /// JsonPath targeting the result array in the API response.
    #[serde(rename = "dataArray")]
    pub data_array: String,
    /// JsonPath to the element name.
    #[serde(rename = "namePath")]
    pub name_path: String,
    /// JsonPath to the element code.
    #[serde(rename = "codePath", skip_serializing_if = "Option::is_none")]
    pub code_path: Option<String>,
    /// JsonPath to the element polygon (its `coordinates` array).
    #[serde(rename = "polygonPath", skip_serializing_if = "Option::is_none")]
    pub polygon_path: Option<String>,
    /// JsonPath to a representative point of the element (`[lon, lat]` array
    /// or GeoJSON `Point` object). Optional: lets the UI center the map on an
    /// element that carries no polygon (e.g. a département centroid).
    #[serde(rename = "centerPath", skip_serializing_if = "Option::is_none")]
    pub center_path: Option<String>,
    /// Query parameter used to search by exact code (e.g. `code` on
    /// geo.api.gouv.fr). Optional: levels that can be resolved from a parent's
    /// code define it.
    #[serde(rename = "codeParam", skip_serializing_if = "Option::is_none")]
    pub code_param: Option<String>,
    /// Parent level name → JsonPath (within this level's elements) to the code
    /// of that parent, e.g. `{"région": "$.properties.codeRegion"}`. Lets the
    /// resolver climb the hierarchy by code without needing geometries.
    #[serde(
        rename = "parentCodes",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub parent_codes: Option<HashMap<String, String>>,
    /// Parent level name → query parameter used to filter the children
    /// (e.g. `département` → `codeDepartement`).
    #[serde(
        rename = "parentParams",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub parent_params: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PolygonEncoding {
    #[default]
    GeoJson,
}

/// A result element returned by a level query: a name and an optional polygon.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CountryElement {
    pub name: String,
    pub code: Option<String>,
    pub polygons: Vec<Polygon>,
    /// Representative point for centering the map on the element.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center: Option<LatLng>,
    /// Parent level name → the parent's code, extracted from the element
    /// JSON via the level's `parentCodes` paths. Not serialized (client reads
    /// the resolved chain, not raw elements).
    #[serde(default, skip_serializing)]
    pub parent_codes: HashMap<String, String>,
}

/// The different ways to query a [`CountryLevel`].
#[derive(Debug, Clone)]
pub enum Query {
    All,
    Name(String),
    Code(String),
    Polygon(Polygon),
    Point {
        lat: f64,
        lon: f64,
    },
    BBox {
        lat_min: f64,
        lon_min: f64,
        lat_max: f64,
        lon_max: f64,
    },
    Children {
        parent_level: String,
        parent_code: String,
    },
}
