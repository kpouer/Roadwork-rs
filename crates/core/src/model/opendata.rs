use crate::model::wkt::polygon::Polygon;
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
pub struct Opendata {
    pub id: String,
    pub reference: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub polygons: Option<Vec<Polygon>>,
    pub description: Option<String>,
}
