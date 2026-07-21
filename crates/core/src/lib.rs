pub mod http_service;
pub mod json_tools;
pub mod model;
pub mod opendata;
pub mod settings;

use jsonpath_rust::parser::errors::JsonPathError;
use thiserror::Error;

pub fn now_millis() -> u64 {
    chrono::Utc::now().timestamp_millis() as u64
}

#[derive(Error, Debug)]
pub enum MyError {
    #[error("Date Parse Error {0:?}")]
    ChronoParseError(#[from] chrono::ParseError),
    #[error("Http Error {0:?}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("Parse Int Error {0:?}")]
    ParseIntError(#[from] std::num::ParseIntError),
    #[error("{0}")]
    RoadworkParsingError(String),
    #[error("{0}")]
    ParsingError(String),
    #[error("{0}")]
    JsonParsingError(String),
    #[error("{0}")]
    JsonPathError(#[from] JsonPathError),
    #[error("{0}")]
    SerdeError(#[from] serde_json::Error),
}
