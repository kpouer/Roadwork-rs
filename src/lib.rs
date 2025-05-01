use jsonpath_rust::parser::errors::JsonPathError;
use thiserror::Error;
use crate::model::roadwork::RoadworkBuilderError;

mod config;
mod mapview;
mod message;
mod model;
mod opendata;
pub mod roadwork;
mod service;
mod settings;

#[derive(Error, Debug)]
pub(crate) enum MyError {
    #[error("Date Parse Error {0:?}")]
    ChronoParseError(#[from] chrono::ParseError),
    #[error("Http Error {0:?}")]
    ReqwestERror(#[from] reqwest::Error),
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
    RoadworkBuilderError(#[from] RoadworkBuilderError),
    #[error("{0}")]
    SerdeError(serde_json::Error),
}