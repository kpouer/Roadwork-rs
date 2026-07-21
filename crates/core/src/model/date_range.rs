use chrono::DateTime;
use chrono_tz::Tz;

pub struct DateRange {
    pub from: DateTime<Tz>,
    pub to: Option<DateTime<Tz>>,
}

impl DateRange {
    pub fn new(from: DateTime<Tz>, to: DateTime<Tz>) -> Self {
        Self { from, to: Some(to) }
    }

    pub fn without_end(from: DateTime<Tz>) -> Self {
        Self { from, to: None }
    }
}
