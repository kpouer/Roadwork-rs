use jsonpath_rust::JsonPath;
use log::{debug, error};
use serde_json::Value;
use crate::model::wkt::polygon::Polygon;
use crate::MyError;
use crate::MyError::JsonParsingError;

pub(crate) trait JsonTools {
    fn get_path(&self, path: &str) -> Result<String, MyError>;
    fn get_path_as_double(&self, path: &str) -> Result<f64, MyError>;
    fn get_path_as_polygons(&self, path: &str) -> Option<Vec<Polygon>>;
}

impl JsonTools for &Value {
    fn get_path(&self, path: &str) -> Result<String, MyError> {
        debug!("get_path path:{path}");
        let result = self.query(path)?;
        if result.is_empty() {
            return Err(JsonParsingError(format!("Unable to get path {path} from {self}")));
        }
        result[0]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| JsonParsingError(format!("Unable to get path {path} from {self}")))
    }

    fn get_path_as_double(&self, path: &str) -> Result<f64, MyError> {
        let result = self.query(path)?;
        if result.is_empty() {
            return Err(JsonParsingError(format!("Unable to get path {path} from {self}")));
        }
        let value = result[0];
        match value {
            Value::Number(number) => Ok(number.as_f64().unwrap()),
            Value::String(string) => string
                .parse::<f64>()
                .or(Err(JsonParsingError(format!("Unable to parse {} as a double", string)))),
            _ => Err(JsonParsingError(format!("Unable to get path {path} from {self}"))),
        }
    }

    fn get_path_as_polygons(&self, path: &str) -> Option<Vec<Polygon>> {
        match self.query(path) {
            Ok(value) => {
                if is_multi_polygon(&value) {
                    return Some(get_multipolygon(&value).ok()?);
                } else if let Some(Value::Array(polygon_array)) = value.first() {
                    return Some(vec![get_polygon(polygon_array).ok()?]);
                }
            }
            Err(e) => error!("Error parsing polygon {e}"),
        }
        None
    }
}

fn get_multipolygon(value: &Vec<&Value>) -> Result<Vec<Polygon>, MyError> {
    let mut polygons = Vec::new();
    for polygon_array in value {
        if let Value::Array(polygon_array) = polygon_array {
            let polygon = get_polygon(polygon_array)?;
            polygons.push(polygon)
        }
    }
    
    Ok(polygons)
}

fn is_multi_polygon(value: &Vec<&Value>) -> bool {
    if let Some(Value::Array(first_level)) = value.first() {
        if let Some(Value::Array(_)) = first_level.first() {
            return true;
        }
    }
    false
}

fn get_polygon(polygon_array: &Vec<Value>) -> Result<Polygon, MyError> {
    let mut xpoints = Vec::with_capacity(polygon_array.len());
    let mut ypoints = Vec::with_capacity(polygon_array.len());
    for point in polygon_array {
        xpoints.push(point[0].as_f64().ok_or(MyError::JsonParsingError("Unable to get point from polygon".to_string()))?);
        ypoints.push(point[1].as_f64().ok_or(MyError::JsonParsingError("Unable to get point from polygon".to_string()))?);
    }
    Ok(Polygon::new(xpoints, ypoints))
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;
    
    #[test]
    fn is_multi_polygon() {
        let json = json!("[
            [
              [
                2.352152402234131,
                48.87953656405322
              ],
              [
                2.3524563598371353,
                48.87932070314852
              ]
            ]
          ]");
        
    }
}