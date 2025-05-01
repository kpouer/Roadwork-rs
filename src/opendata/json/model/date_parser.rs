use crate::opendata::json::model::date_result::DateResult;
use crate::opendata::json::model::parser::Parser;
use crate::MyError;
use chrono::DateTime;
use chrono_tz::Tz;
use serde::Deserialize;
use crate::MyError::ParsingError;

#[derive(Debug, Deserialize)]
pub(crate) struct DateParser {
    pub(crate) path: String,
    parsers: Vec<Parser>,
}

impl DateParser {
    pub(crate) fn parse(&self, value: &str, locale: Tz) -> Result<DateResult, MyError> {
        for parser in &self.parsers {
            if let Some(regex) = parser.get_pattern() {
                let captures = regex.captures(value);
                if let Some(groups) = captures {
                    let value = if groups.len() == 1 {
                        groups[1].to_string()
                    } else {
                        groups[0].to_string()
                    };
                    let mut timestamp;
                    match &parser.format {
                        Some(format) => {
                            timestamp = Self::parse_date(format, &value, locale)?
                                .timestamp_millis()
                        }
                        None => {
                            // format is null then it must be a timestamp in seconds or ms
                            timestamp = value.parse::<i64>()?;
                            if timestamp < 1000000000000 {
                                // the timestamp was in second
                                timestamp *= 1000;
                            }
                        }
                    }
                    return Ok(DateResult::new(timestamp, parser.clone()));
                }
            }
        }
        Err(ParsingError(format!(
            "Unable to parse date '{value}' with parsers :{}",
            self.to_string_parsers()
        )))
    }

    fn to_string_parsers(&self) -> String {
        let mut formats = Vec::new();
        for format in &self.parsers {
            if let Some(format) = &format.format {
                formats.push(format.as_str())
            }
        }
        formats.join("|")
    }

    fn parse_date(pattern: &str, value: &str, locale: Tz) -> Result<DateTime<Tz>, MyError> {
        DateTime::parse_from_str(value, pattern)
            .map(|d| d.with_timezone(&locale))
            .map_err(|e| MyError::ChronoParseError(e))
    }
}
