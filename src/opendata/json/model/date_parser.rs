use crate::MyError;
use crate::MyError::ParsingError;
use crate::opendata::json::model::date_result::DateResult;
use crate::opendata::json::model::parser::Parser;
use chrono_tz::Tz;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct DateParser {
    pub(crate) path: String,
    parsers: Vec<Parser>,
}

impl DateParser {
    pub(crate) fn parse(&self, value: &str, locale: Tz) -> Result<DateResult, MyError> {
        self.parsers
            .iter()
            .find_map(|parser| parser.parse(value, locale))
            .ok_or(ParsingError(format!(
                "Unable to parse date '{value}' with parsers :{}",
                self.to_string_parsers()
            )))
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
