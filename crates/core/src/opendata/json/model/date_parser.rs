use crate::opendata::json::model::date_result::DateResult;
use crate::opendata::json::model::parser::Parser;
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DateParser {
    pub path: String,
    pub parsers: Vec<Parser>,
}

impl DateParser {
    pub fn parse(&self, value: &str, locale: Tz) -> Result<DateResult, DateError> {
        self.parsers
            .iter()
            .find_map(|parser| parser.parse(value, locale))
            .ok_or(DateError::ParsingError(
                value.to_string(),
                self.to_string_parsers(),
            ))
    }

    fn to_string_parsers(&self) -> String {
        let mut formats = Vec::with_capacity(self.parsers.len());
        for format in &self.parsers {
            if let Some(format) = &format.format {
                formats.push(format.as_str())
            }
        }
        formats.join("|")
    }
}

#[derive(Debug, Error)]
pub enum DateError {
    #[error("Unable to parse date '{0}' with parsers '{1}'")]
    ParsingError(String, String),
}
