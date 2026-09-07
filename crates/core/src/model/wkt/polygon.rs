use serde::{Deserialize, Serialize};

/// A polygon ring as two parallel arrays. `xpoints` is the longitude (x)
/// axis, `ypoints` the latitude (y) axis.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Polygon {
    pub xpoints: Vec<f64>,
    pub ypoints: Vec<f64>,
}

impl Polygon {
    /// Whether the point (`lon`, `lat`) lies inside this polygon (ray casting).
    pub fn contains_point(&self, lon: f64, lat: f64) -> bool {
        let n = self.xpoints.len().min(self.ypoints.len());
        if n < 3 {
            return false;
        }
        let mut inside = false;
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = (self.xpoints[i], self.ypoints[i]);
            let (xj, yj) = (self.xpoints[j], self.ypoints[j]);
            let intersects =
                (yi > lat) != (yj > lat) && lon < (xj - xi) * (lat - yi) / (yj - yi) + xi;
            if intersects {
                inside = !inside;
            }
            j = i;
        }
        inside
    }

    /// Whether the axis-aligned rectangle `(lat_min, lon_min, lat_max, lon_max)`
    /// is entirely inside this polygon (all four corners inside).
    pub fn contains_rect(&self, lon_min: f64, lat_min: f64, lon_max: f64, lat_max: f64) -> bool {
        self.contains_point(lon_min, lat_min)
            && self.contains_point(lon_min, lat_max)
            && self.contains_point(lon_max, lat_min)
            && self.contains_point(lon_max, lat_max)
    }
}
