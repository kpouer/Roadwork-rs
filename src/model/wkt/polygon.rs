use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct Polygon {
    pub(crate) xpoints: Vec<f64>,
    pub(crate) ypoints: Vec<f64>,
}

impl Polygon {
    pub(crate) fn new(xpoints: Vec<f64>, ypoints: Vec<f64>) -> Polygon {
        Self { xpoints, ypoints }
    }
}
