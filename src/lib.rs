use jsonpath_rust::parser::errors::JsonPathError;
use thiserror::Error;

mod model;
mod opendata;
pub mod roadwork_app;
mod service;
mod settings;
mod json_tools;
mod gui;

#[derive(Error, Debug)]
pub(crate) enum MyError {
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