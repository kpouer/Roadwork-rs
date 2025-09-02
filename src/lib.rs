use jsonpath_rust::parser::errors::JsonPathError;
use thiserror::Error;

mod gui;
mod json_tools;
mod model;
mod opendata;
pub mod roadwork_app;
mod service;
pub mod settings;

/// the path where the opendata definitions are stored
pub(crate) const OPENDATA_FOLDER: &str = "data/opendata";

pub(crate) fn opendata_folder_path() -> std::path::PathBuf {
    // Prefer user's home directory: ~/.roadwork/data/opendata if available
    if let Some(mut home) = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .or_else(home::home_dir)
    {
        home.push(".roadwork");
        home.push("data");
        home.push("opendata");
        return home;
    }
    std::path::PathBuf::from(OPENDATA_FOLDER)
}

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
