use crate::MyError;
use crate::MyError::JsonParsingError;
use crate::model::wkt::polygon::Polygon;
use jsonpath_rust::JsonPath;
use log::{debug, error};
use serde_json::Value;

pub trait JsonTools {
    fn get_path(&self, path: &str) -> Result<String, MyError>;
    fn get_path_as_double(&self, path: &str) -> Result<f64, MyError>;
    fn get_path_as_polygons(&self, path: &str) -> Option<Vec<Polygon>>;
    fn collect_arrays(&self, path: &str) -> Vec<(String, usize)>;
}

impl JsonTools for &Value {
    fn get_path(&self, path: &str) -> Result<String, MyError> {
        debug!("get_path path:{path}");
        let result = self.query(path)?;
        if result.is_empty() {
            return Err(JsonParsingError(format!(
                "Unable to get path {path} from {self}"
            )));
        }
        result[0]
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| result[0].as_number().map(|n| n.to_string()))
            .ok_or_else(|| JsonParsingError(format!("Unable to get path {path} from {self}")))
    }

    fn get_path_as_double(&self, path: &str) -> Result<f64, MyError> {
        let result = self.query(path)?;
        if result.is_empty() {
            return Err(JsonParsingError(format!(
                "Unable to get path {path} from {self}"
            )));
        }
        let value = result[0];
        match value {
            Value::Number(number) => Ok(number.as_f64().unwrap()),
            Value::String(string) => string.parse::<f64>().or(Err(JsonParsingError(format!(
                "Unable to parse {} as a double",
                string
            )))),
            _ => Err(JsonParsingError(format!(
                "Unable to get path {path} from {self}"
            ))),
        }
    }

    fn get_path_as_polygons(&self, path: &str) -> Option<Vec<Polygon>> {
        match self.query(path) {
            Ok(value) => {
                if is_multi_polygon(&value) {
                    get_multipolygon(&value).ok()
                } else if let Some(polygon) = value.first() {
                    get_polygon(polygon).ok().map(|polygon| vec![polygon])
                } else {
                    None
                }
            }
            Err(e) => {
                error!("Error parsing polygon {e}");
                None
            }
        }
    }

    fn collect_arrays(&self, path: &str) -> Vec<(String, usize)> {
        let mut arrays = Vec::new();
        match self {
            Value::Array(elements) => {
                if !elements.is_empty() && elements.iter().all(Value::is_object) {
                    arrays.push((path.to_string(), elements.len()));
                }
                for (i, element) in elements.iter().enumerate() {
                    if element.is_array() || element.is_object() {
                        arrays.extend(element.collect_arrays(&format!("{path}[{i}]")));
                    }
                }
            }
            Value::Object(map) => {
                for (key, child) in map {
                    if child.is_array() || child.is_object() {
                        let child_path = if is_plain_key(key) {
                            format!("{path}.{key}")
                        } else {
                            format!("{path}[\"{key}\"]")
                        };
                        arrays.extend(child.collect_arrays(&child_path));
                    }
                }
            }
            _ => {}
        }
        arrays
    }
}

fn get_multipolygon(value: &Vec<&Value>) -> Result<Vec<Polygon>, MyError> {
    let mut polygons = Vec::new();
    for polygon_array in value {
        if let Value::Array(polygon_array) = polygon_array {
            for polygon in polygon_array {
                polygons.push(get_polygon(polygon)?);
            }
        }
    }
    Ok(polygons)
}

fn is_multi_polygon(value: &Vec<&Value>) -> bool {
    if let Some(Value::Array(first_level)) = value.first()
        && let Some(Value::Array(_)) = first_level.first()
    {
        return true;
    }
    false
}

fn get_polygon(polygon: &Value) -> Result<Polygon, MyError> {
    let Value::Array(rings) = polygon else {
        return Err(MyError::JsonParsingError("Invalid polygon".to_string()));
    };
    let ring = rings
        .first()
        .ok_or(MyError::JsonParsingError("Empty polygon".to_string()))?;
    let Value::Array(ring) = ring else {
        return Err(MyError::JsonParsingError(
            "Invalid polygon ring".to_string(),
        ));
    };
    let mut xpoints = Vec::with_capacity(ring.len());
    let mut ypoints = Vec::with_capacity(ring.len());
    for point in ring {
        xpoints.push(point[0].as_f64().ok_or(MyError::JsonParsingError(
            "Unable to get point from polygon".to_string(),
        ))?);
        ypoints.push(point[1].as_f64().ok_or(MyError::JsonParsingError(
            "Unable to get point from polygon".to_string(),
        ))?);
    }
    Ok(Polygon { xpoints, ypoints })
}

pub fn is_plain_key(key: &str) -> bool {
    !key.is_empty()
        && !key.chars().next().is_some_and(|c| c.is_ascii_digit())
        && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn find_json_arrays(json: &str) -> Vec<(String, usize)> {
    let Ok(value) = serde_json::from_str::<Value>(json) else {
        return Vec::new();
    };
    let mut arrays = (&value).collect_arrays("$");
    arrays.sort_by_key(|b| std::cmp::Reverse(b.1));
    arrays
}

/// Replaces the array targeted by a `dataArray`-style path (e.g. `$.records[*]`,
/// `$.features[*]`, `$.results`, `$`) with `array`.
pub fn replace_array_at_path(
    document: &mut Value,
    path: &str,
    array: Value,
) -> Result<(), MyError> {
    let path = path.trim();
    if path == "$" {
        *document = array;
        return Ok(());
    }
    let container = path.strip_suffix("[*]").unwrap_or(path);
    let segments = parse_path_segments(container)?;
    if segments.is_empty() {
        *document = array;
        return Ok(());
    }
    let mut current = document;
    let last = segments.len() - 1;
    for (index, segment) in segments.into_iter().enumerate() {
        let is_last = index == last;
        match segment {
            PathSegment::Key(key) => {
                if is_last {
                    let map = current.as_object_mut().ok_or_else(|| {
                        JsonParsingError(format!(
                            "Unable to set array at path {path}: intermediate value is not an object"
                        ))
                    })?;
                    map.insert(key, array);
                    return Ok(());
                }
                current = current.get_mut(&key).ok_or_else(|| {
                    JsonParsingError(format!(
                        "Unable to set array at path {path}: missing key {key}"
                    ))
                })?;
            }
            PathSegment::Index(index) => {
                if is_last {
                    let elements = current.as_array_mut().ok_or_else(|| {
                        JsonParsingError(format!(
                            "Unable to set array at path {path}: intermediate value is not an array"
                        ))
                    })?;
                    let element = elements.get_mut(index).ok_or_else(|| {
                        JsonParsingError(format!(
                            "Unable to set array at path {path}: missing index {index}"
                        ))
                    })?;
                    *element = array;
                    return Ok(());
                }
                current = current.get_mut(index).ok_or_else(|| {
                    JsonParsingError(format!(
                        "Unable to set array at path {path}: missing index {index}"
                    ))
                })?;
            }
        }
    }
    Ok(())
}

enum PathSegment {
    Key(String),
    Index(usize),
}

fn parse_path_segments(path: &str) -> Result<Vec<PathSegment>, MyError> {
    let body = path.strip_prefix('$').unwrap_or(path);
    let bytes = body.as_bytes();
    let mut segments = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'.' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                    i += 1;
                }
                segments.push(PathSegment::Key(body[start..i].to_string()));
            }
            b'[' => {
                i += 1;
                if i < bytes.len() && bytes[i] == b'"' {
                    i += 1;
                    let start = i;
                    while i < bytes.len() && bytes[i] != b'"' {
                        i += 1;
                    }
                    segments.push(PathSegment::Key(body[start..i].to_string()));
                    i += 1;
                    if i < bytes.len() && bytes[i] == b']' {
                        i += 1;
                    }
                } else {
                    let start = i;
                    while i < bytes.len() && bytes[i] != b']' {
                        i += 1;
                    }
                    let index = body[start..i]
                        .parse::<usize>()
                        .map_err(|_| JsonParsingError(format!("Unable to parse path {path}")))?;
                    segments.push(PathSegment::Index(index));
                    i += 1;
                }
            }
            _ => {
                return Err(JsonParsingError(format!("Unable to parse path {path}")));
            }
        }
    }
    Ok(segments)
}

pub fn element_scalar_paths(element: &Value) -> Vec<(String, String)> {
    let mut scalars = Vec::new();
    collect_scalar_leaves(element, "$", &mut scalars);
    scalars.sort();
    scalars
}

pub fn element_array_paths(element: &Value) -> Vec<(String, usize)> {
    let mut arrays = Vec::new();
    collect_element_arrays(element, "$", &mut arrays, 0);
    arrays.sort();
    arrays
}

fn collect_scalar_leaves(value: &Value, path: &str, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if out.len() >= MAX_SCALAR_PATHS {
                    return;
                }
                let child_path = if is_plain_key(key) {
                    format!("{path}.{key}")
                } else {
                    format!("{path}[\"{key}\"]")
                };
                collect_scalar_leaves(child, &child_path, out);
            }
        }
        Value::Array(elements) => {
            for (i, element) in elements.iter().enumerate().take(MAX_ARRAY_INDEX) {
                if out.len() >= MAX_SCALAR_PATHS {
                    return;
                }
                collect_scalar_leaves(element, &format!("{path}[{i}]"), out);
            }
        }
        _ => out.push((path.to_string(), format_fetched_value(value))),
    }
}

fn collect_element_arrays(
    value: &Value,
    path: &str,
    arrays: &mut Vec<(String, usize)>,
    depth: usize,
) {
    if depth > MAX_ARRAY_DEPTH {
        return;
    }
    match value {
        Value::Array(elements) => {
            arrays.push((path.to_string(), elements.len()));
            for (i, element) in elements.iter().enumerate().take(MAX_ARRAY_INDEX) {
                if element.is_array() || element.is_object() {
                    collect_element_arrays(element, &format!("{path}[{i}]"), arrays, depth + 1);
                }
            }
        }
        Value::Object(map) => {
            for (key, child) in map {
                if child.is_array() || child.is_object() {
                    let child_path = if is_plain_key(key) {
                        format!("{path}.{key}")
                    } else {
                        format!("{path}[\"{key}\"]")
                    };
                    collect_element_arrays(child, &child_path, arrays, depth + 1);
                }
            }
        }
        _ => {}
    }
}

pub fn is_scalar(value: &Value) -> bool {
    matches!(
        value,
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
    )
}

pub fn format_fetched_value(value: &Value) -> String {
    let text = match value {
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    };
    if text.len() > 200 {
        format!("{}…", &text[..200])
    } else {
        text
    }
}

const MAX_ARRAY_INDEX: usize = 8;
const MAX_ARRAY_DEPTH: usize = 4;
const MAX_SCALAR_PATHS: usize = 200;

#[cfg(test)]
mod replace_array_tests {
    use super::*;

    #[test]
    fn replaces_records_array() {
        let mut document = serde_json::json!({"records": [{"a": 1}], "other": true});
        replace_array_at_path(&mut document, "$.records[*]", serde_json::json!([{"a": 2}]))
            .unwrap();
        assert_eq!(
            document,
            serde_json::json!({"records": [{"a": 2}], "other": true})
        );
    }

    #[test]
    fn replaces_nested_array() {
        let mut document = serde_json::json!({"data": {"items": [1], "x": 1}});
        replace_array_at_path(&mut document, "$.data.items", serde_json::json!([1, 2])).unwrap();
        assert_eq!(
            document,
            serde_json::json!({"data": {"items": [1, 2], "x": 1}})
        );
    }

    #[test]
    fn replaces_whole_document() {
        let mut document = serde_json::json!({"a": 1});
        replace_array_at_path(&mut document, "$", serde_json::json!([1, 2, 3])).unwrap();
        assert_eq!(document, serde_json::json!([1, 2, 3]));
    }

    #[test]
    fn replaces_bracketed_key() {
        let mut document = serde_json::json!({"features with space": [1]});
        replace_array_at_path(
            &mut document,
            "$[\"features with space\"][*]",
            serde_json::json!([1, 2]),
        )
        .unwrap();
        assert_eq!(document, serde_json::json!({"features with space": [1, 2]}));
    }

    #[test]
    fn missing_intermediate_key_is_an_error() {
        let mut document = serde_json::json!({"records": []});
        assert!(
            replace_array_at_path(&mut document, "$.nope.items", serde_json::json!([])).is_err()
        );
    }
}
