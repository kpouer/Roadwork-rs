use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Polygon {
    pub xpoints: Vec<f64>,
    pub ypoints: Vec<f64>,
}

impl Polygon {
    pub fn new(xpoints: Vec<f64>, ypoints: Vec<f64>) -> Polygon {
        Self { xpoints, ypoints }
    }
}
