use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Polygon {
    pub xpoints: Vec<f64>,
    pub ypoints: Vec<f64>,
}
