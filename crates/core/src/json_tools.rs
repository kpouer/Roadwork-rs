use crate::model::wkt::polygon::Polygon;
use jsonpath_rust::JsonPath;
use jsonpath_rust::parser::errors::JsonPathError;
use log::{debug, error};
use serde_json::Value;
use thiserror::Error;

pub trait JsonTools {
    fn get_path(&self, path: &str) -> Result<String, JsonError>;
    fn get_path_as_double(&self, path: &str) -> Result<f64, JsonError>;
    fn get_path_as_polygons(&self, path: &str) -> Option<Vec<Polygon>>;
    fn collect_arrays(&self, path: &str) -> Vec<(String, usize)>;
}

impl JsonTools for &Value {
    fn get_path(&self, path: &str) -> Result<String, JsonError> {
        debug!("get_path path:{path}");
        let result = self.query(path)?;
        if result.is_empty() {
            return Err(JsonError::InvalidJsonPath(
                path.to_string(),
                self.to_string(),
            ));
        }
        result[0]
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| result[0].as_number().map(|n| n.to_string()))
            .ok_or_else(|| JsonError::InvalidJsonPath(path.to_string(), self.to_string()))
    }

    fn get_path_as_double(&self, path: &str) -> Result<f64, JsonError> {
        let result = self.query(path)?;
        if result.is_empty() {
            return Err(JsonError::InvalidJsonPath(
                path.to_string(),
                self.to_string(),
            ));
        }
        let value = result[0];
        match value {
            Value::Number(number) => Ok(number.as_f64().unwrap()),
            Value::String(string) => string
                .parse::<f64>()
                .or(Err(JsonError::NotADouble(string.into()))),
            _ => Err(JsonError::InvalidJsonPath(
                path.to_string(),
                self.to_string(),
            )),
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

fn get_multipolygon(value: &Vec<&Value>) -> Result<Vec<Polygon>, JsonError> {
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

fn get_polygon(polygon: &Value) -> Result<Polygon, JsonError> {
    let Value::Array(rings) = polygon else {
        return Err(JsonError::InvalidPolygon);
    };
    let ring = rings.first().ok_or(JsonError::EmptyPolygon)?;
    let Value::Array(ring) = ring else {
        return Err(JsonError::InvalidPolygonRing);
    };
    let mut xpoints = Vec::with_capacity(ring.len());
    let mut ypoints = Vec::with_capacity(ring.len());
    for point in ring {
        xpoints.push(point[0].as_f64().ok_or(JsonError::MissingPointInPolygon)?);
        ypoints.push(point[1].as_f64().ok_or(JsonError::MissingPointInPolygon)?);
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
    find_json_arrays_value(&value)
}

pub fn find_json_arrays_value(value: &Value) -> Vec<(String, usize)> {
    let mut arrays = value.collect_arrays("$");
    arrays.sort_by_key(|b| std::cmp::Reverse(b.1));
    arrays
}

pub fn element_scalar_paths(element: &Value) -> Vec<(String, String)> {
    let mut scalars = Vec::new();
    collect_scalar_leaves(element, "$", &mut scalars, &|_| true);
    scalars.sort();
    scalars
}

pub fn element_number_paths_between(element: &Value, min: f64, max: f64) -> Vec<(String, String)> {
    let mut numbers = Vec::new();
    collect_scalar_leaves(element, "$", &mut numbers, &|value| {
        is_number_between(value, min, max)
    });
    numbers.sort();
    numbers
}

pub fn element_string_paths(element: &Value) -> Vec<(String, String)> {
    let mut strings = Vec::new();
    collect_scalar_leaves(element, "$", &mut strings, &|value| {
        matches!(value, Value::String(_))
    });
    strings.sort();
    strings
}

pub fn is_number_between(value: &Value, min: f64, max: f64) -> bool {
    value
        .as_f64()
        .is_some_and(|number| (min..=max).contains(&number))
}

pub fn element_array_paths(element: &Value) -> Vec<(String, usize)> {
    let mut arrays = Vec::new();
    collect_element_arrays(element, "$", &mut arrays, 0);
    arrays.sort();
    arrays
}

fn collect_scalar_leaves(
    value: &Value,
    path: &str,
    out: &mut Vec<(String, String)>,
    filter: &dyn Fn(&Value) -> bool,
) {
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
                collect_scalar_leaves(child, &child_path, out, filter);
            }
        }
        Value::Array(elements) => {
            for (i, element) in elements.iter().enumerate().take(MAX_ARRAY_INDEX) {
                if out.len() >= MAX_SCALAR_PATHS {
                    return;
                }
                collect_scalar_leaves(element, &format!("{path}[{i}]"), out, filter);
            }
        }
        _ => {
            if filter(value) {
                out.push((path.to_string(), format_fetched_value(value)));
            }
        }
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

pub const LATITUDE_RANGE: (f64, f64) = (-90.0, 90.0);
pub const LONGITUDE_RANGE: (f64, f64) = (-180.0, 180.0);

/// An incremental, cooperative JSON scanner.
///
/// Parses a JSON document a bounded number of bytes at a time, reporting the
/// fraction of the document consumed so far. This keeps the UI responsive and
/// lets a progress bar reflect real byte-level progress on large files. When
/// the scan completes, the whole document is available as a `serde_json::Value`.
///
/// The scanner owns its source so it can live across frames (or threads) while
/// the document is being processed incrementally.
pub struct JsonScan {
    src: Box<[u8]>,
    pos: usize,
    end: usize,
    stack: Vec<ScanFrame>,
    root: Option<Value>,
    error: Option<String>,
    mode: ScanMode,
    token_buf: String,
    string_escaped: bool,
    unicode_hex: u32,
    unicode_left: u8,
}

enum ScanMode {
    Expect,
    InString,
    InScalar,
}

enum ScanFrame {
    Array {
        values: Vec<Value>,
    },
    Object {
        fields: Vec<(String, Value)>,
        pending_key: Option<String>,
        expect_value: bool,
    },
}

impl JsonScan {
    pub fn new(source: &str) -> Self {
        let src = source.as_bytes().to_vec().into_boxed_slice();
        let end = src.len();
        Self {
            src,
            pos: 0,
            end,
            stack: Vec::new(),
            root: None,
            error: None,
            mode: ScanMode::Expect,
            token_buf: String::new(),
            string_escaped: false,
            unicode_hex: 0,
            unicode_left: 0,
        }
    }

    /// The source document as a string.
    pub fn source(&self) -> &str {
        std::str::from_utf8(&self.src).unwrap_or_default()
    }

    /// True once the document has been fully scanned (or failed).
    pub fn is_done(&self) -> bool {
        self.root.is_some() || self.error.is_some()
    }

    /// Fraction (0..=1) of the document consumed so far.
    pub fn progress(&self) -> f32 {
        if self.end == 0 {
            return 1.0;
        }
        (self.pos as f32 / self.end as f32).clamp(0.0, 1.0)
    }

    /// Advances the scan by up to `budget` bytes and returns the progress.
    pub fn step(&mut self, budget: usize) -> f32 {
        if self.is_done() {
            return self.progress();
        }
        let stop = self.end.min(self.pos.saturating_add(budget.max(1)));
        while self.pos < stop {
            let b = self.src[self.pos];
            match self.mode {
                ScanMode::InString => {
                    self.pos += 1;
                    if self.unicode_left > 0 {
                        let d = hex_digit(b);
                        self.unicode_hex = (self.unicode_hex << 4) | u32::from(d);
                        self.unicode_left -= 1;
                        if self.unicode_left == 0 {
                            if let Some(c) = char::from_u32(self.unicode_hex) {
                                self.token_buf.push(c);
                            } else {
                                self.token_buf.push('\u{FFFD}');
                            }
                        }
                        continue;
                    }
                    if self.string_escaped {
                        self.string_escaped = false;
                        match b {
                            b'"' => self.token_buf.push('"'),
                            b'\\' => self.token_buf.push('\\'),
                            b'/' => self.token_buf.push('/'),
                            b'b' => self.token_buf.push('\u{0008}'),
                            b'f' => self.token_buf.push('\u{000C}'),
                            b'n' => self.token_buf.push('\n'),
                            b'r' => self.token_buf.push('\r'),
                            b't' => self.token_buf.push('\t'),
                            b'u' => {
                                self.unicode_hex = 0;
                                self.unicode_left = 4;
                            }
                            other => {
                                self.error = Some(format!("Invalid escape \\{}", other as char));
                            }
                        }
                    } else if b == b'\\' {
                        self.string_escaped = true;
                    } else if b == b'"' {
                        self.finish_string_token();
                        self.mode = ScanMode::Expect;
                    } else {
                        self.token_buf.push(b as char);
                    }
                }
                ScanMode::InScalar => {
                    if is_scalar_end(b) {
                        self.finish_scalar_token();
                        self.mode = ScanMode::Expect;
                        continue;
                    }
                    self.token_buf.push(b as char);
                    self.pos += 1;
                }
                ScanMode::Expect => match b {
                    b' ' | b'\t' | b'\n' | b'\r' => {
                        self.pos += 1;
                    }
                    b'{' => {
                        self.stack.push(ScanFrame::Object {
                            fields: Vec::new(),
                            pending_key: None,
                            expect_value: false,
                        });
                        self.pos += 1;
                    }
                    b'[' => {
                        self.stack.push(ScanFrame::Array { values: Vec::new() });
                        self.pos += 1;
                    }
                    b'}' => {
                        self.finish_container(false);
                        self.pos += 1;
                    }
                    b']' => {
                        self.finish_container(true);
                        self.pos += 1;
                    }
                    b',' => {
                        self.pos += 1;
                    }
                    b':' => {
                        if let Some(ScanFrame::Object { expect_value, .. }) = self.stack.last_mut()
                        {
                            *expect_value = true;
                        } else {
                            self.error = Some("Unexpected ':'".to_string());
                        }
                        self.pos += 1;
                    }
                    b'"' => {
                        self.mode = ScanMode::InString;
                        self.token_buf.clear();
                        self.string_escaped = false;
                        self.unicode_left = 0;
                        self.pos += 1;
                    }
                    b'-' | b'0'..=b'9' | b't' | b'f' | b'n' => {
                        self.mode = ScanMode::InScalar;
                        self.token_buf.clear();
                        self.token_buf.push(b as char);
                        self.pos += 1;
                    }
                    other => {
                        self.error = Some(format!("Unexpected byte 0x{other:02x}"));
                    }
                },
            }
            if self.is_done() {
                break;
            }
        }
        if !self.is_done() && self.pos >= self.end {
            self.error = Some("Unexpected end of input".to_string());
        }
        self.progress()
    }

    /// Consumes the scanner, returning the source document and the parsed value.
    pub fn into_parts(self) -> (String, Result<serde_json::Value, String>) {
        let Self {
            src,
            pos,
            end,
            root,
            error,
            ..
        } = self;
        let source = String::from_utf8_lossy(&src).into_owned();
        let result = (|| {
            if let Some(err) = error {
                return Err(err);
            }
            let root = root.ok_or_else(|| "JSON scan not finished".to_string())?;
            let tail = &src[pos.min(end)..];
            if !tail.iter().all(u8::is_ascii_whitespace) {
                return Err("Trailing content after JSON value".to_string());
            }
            Ok(root)
        })();
        (source, result)
    }

    fn finish_scalar_token(&mut self) {
        let token = std::mem::take(&mut self.token_buf);
        let value = match token.as_str() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            "null" => Some(Value::Null),
            _ => serde_json::from_str::<Value>(&token).ok(),
        };
        match value {
            Some(v) => self.attach_value(v),
            None => self.error = Some(format!("Invalid token {token}")),
        }
    }

    fn finish_string_token(&mut self) {
        let s = std::mem::take(&mut self.token_buf);
        self.attach_value(Value::String(s));
    }

    fn attach_value(&mut self, value: Value) {
        if self.error.is_some() {
            return;
        }
        match self.stack.last_mut() {
            None => {
                self.root = Some(value);
            }
            Some(ScanFrame::Array { values }) => {
                values.push(value);
            }
            Some(ScanFrame::Object {
                fields,
                pending_key,
                expect_value,
            }) => {
                if !*expect_value && pending_key.is_none() {
                    match value {
                        Value::String(s) => {
                            *pending_key = Some(s);
                        }
                        _ => {
                            self.error = Some("Object key must be a string".to_string());
                        }
                    }
                } else {
                    let key = pending_key.take().unwrap_or_default();
                    fields.push((key, value));
                    *expect_value = false;
                }
            }
        }
    }

    fn finish_container(&mut self, is_array: bool) {
        if self.error.is_some() {
            return;
        }
        let value = match self.stack.pop() {
            Some(ScanFrame::Array { values }) if is_array => Some(Value::Array(values)),
            Some(ScanFrame::Object { fields, .. }) if !is_array => {
                Some(Value::Object(fields.into_iter().collect()))
            }
            _ => {
                self.error = Some(if is_array {
                    "Unexpected ']'".to_string()
                } else {
                    "Unexpected '}'".to_string()
                });
                return;
            }
        };
        if let Some(v) = value {
            self.attach_value(v);
        }
    }
}

fn hex_digit(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

fn is_scalar_end(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | b',' | b'}' | b']')
}

#[derive(Error, Debug)]
pub enum JsonError {
    #[error("Unable to get path {0} from {1}")]
    InvalidJsonPath(String, String),
    #[error("Unable to parse {0} as a double")]
    NotADouble(String),
    #[error(transparent)]
    JsonPathError(#[from] JsonPathError),
    #[error("Invalid polygon")]
    InvalidPolygon,
    #[error("Unable to get point from polygon")]
    MissingPointInPolygon,
    #[error("Invalid polygon ring")]
    InvalidPolygonRing,
    #[error("Empty polygon")]
    EmptyPolygon,
}
