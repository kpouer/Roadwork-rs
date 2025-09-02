use crate::opendata::json::model::date_result::DateResult;
use chrono::{DateTime, NaiveDate, TimeZone};
use chrono_tz::Tz;
use regex::Regex;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Parser {
    /// the matcher is a regexp that will extract the date format from a text
    pub(crate) matcher: String,
    /// A date format to parse the timestamp. If missing, then it is a timestamp
    pub(crate) format: Option<String>,
    #[serde(default)]
    #[serde(rename = "addYear")]
    pub(crate) add_year: bool,
    #[serde(default)]
    #[serde(rename = "resetHour")]
    pub(crate) reset_hour: bool,
}

impl Parser {
    pub(crate) fn parse(&self, value: &str, locale: Tz) -> Option<DateResult> {
        let pattern = Regex::new(&self.matcher).ok()?;
        if let Some(groups) = pattern.captures(value) {
            let date_string = if groups.len() == 1 {
                groups[0].to_string()
            } else {
                groups[1].to_string()
            };
            let timestamp = self.parse_date(&date_string, locale)?;
            return Some(DateResult::new(
                DateTime::from_timestamp_millis(timestamp).map(|d| d.with_timezone(&locale))?,
                self.add_year,
                self.reset_hour,
            ));
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
                // no format then it must be a timestamp in seconds or ms
                let mut timestamp = date_string.parse::<i64>().ok()?;
                if timestamp < 1000000000000 {
                    // the timestamp was in second
                    timestamp *= 1000;
                }
                Some(timestamp)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn test_parse_date() {
        let parser = Parser {
            matcher: String::from(".*"),
            format: Some(String::from("%Y-%m-%d")),
            add_year: false,
            reset_hour: false,
        };
        let date = parser
            .parse("2025-02-10", chrono_tz::Europe::Paris)
            .unwrap();
        assert_eq!(date.date.year(), 2025);
        assert_eq!(date.date.month(), 2);
        assert_eq!(date.date.day(), 10);
        assert_eq!(date.date.hour(), 0);
        assert_eq!(date.date.minute(), 0);
        assert_eq!(date.date.second(), 0);
    }

    #[test]
    fn test_parse_timestamp() {
        let parser = Parser {
            matcher: String::from(".*"),
            format: None,
            add_year: false,
            reset_hour: false,
        };
        let date = parser
            .parse("1746113416000", chrono_tz::Europe::Paris)
            .unwrap();
        assert_eq!(date.date.year(), 2025);
        assert_eq!(date.date.month(), 5);
        assert_eq!(date.date.day(), 1);
        assert_eq!(date.date.hour(), 17);
        assert_eq!(date.date.minute(), 30);
        assert_eq!(date.date.second(), 16);
    }

    #[test]
    fn test_parse_timestamp_sec() {
        let parser = Parser {
            matcher: String::from(".*"),
            format: None,
            add_year: false,
            reset_hour: false,
        };
        let date = parser
            .parse("1746113416", chrono_tz::Europe::Paris)
            .unwrap();
        assert_eq!(date.date.year(), 2025);
        assert_eq!(date.date.month(), 5);
        assert_eq!(date.date.day(), 1);
        assert_eq!(date.date.hour(), 17);
        assert_eq!(date.date.minute(), 30);
        assert_eq!(date.date.second(), 16);
    }
}
