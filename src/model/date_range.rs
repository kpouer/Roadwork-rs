use chrono::DateTime;
use chrono_tz::Tz;

pub(crate) struct DateRange {
    pub(crate) from: DateTime<Tz>,
    pub(crate) to: Option<DateTime<Tz>>,
}

impl DateRange {
    pub(crate) fn new(from: DateTime<Tz>, to: DateTime<Tz>) -> Self {
        Self { from, to: Some(to) }
    }
    
    pub(crate) fn without_end(from: DateTime<Tz>) -> Self {
        Self { from, to: None }   
    }
}
