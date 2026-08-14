use crate::opendata::json::model::date_result::DateResult;
use chrono::{DateTime, NaiveDate, TimeZone};
use chrono_tz::Tz;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Parser {
    pub matcher: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default)]
    #[serde(rename = "addYear")]
    pub add_year: bool,
    #[serde(default)]
    #[serde(rename = "resetHour")]
    pub reset_hour: bool,
}

impl Parser {
    pub fn parse(&self, value: &str, locale: Tz) -> Option<DateResult> {
        let pattern = Regex::new(&self.matcher).ok()?;
        if let Some(groups) = pattern.captures(value) {
            let date_string = if groups.len() == 1 {
                groups[0].to_string()
            } else {
                groups[1].to_string()
            };
            let timestamp = self.parse_date(&date_string, locale)?;
            return Some(DateResult {
                date: DateTime::from_timestamp_millis(timestamp)
                    .map(|d| d.with_timezone(&locale))?,
                add_year: self.add_year,
                reset_hour: self.reset_hour,
            });
        }
        None
    }

    fn parse_date(&self, date_string: &str, locale: Tz) -> Option<i64> {
        match &self.format {
            Some(format) => {
                let naive_date = NaiveDate::parse_from_str(date_string, format).ok()?;
                let naive_datetime = naive_date.and_hms_opt(0, 0, 0)?;
                let datetime = locale.from_local_datetime(&naive_datetime).single()?;
                Some(datetime.timestamp_millis())
            }
            None => {
                let mut timestamp = date_string.parse::<i64>().ok()?;
                if timestamp < 1000000000000 {
                    timestamp *= 1000;
                }
                Some(timestamp)
            }
        }
    }
}
