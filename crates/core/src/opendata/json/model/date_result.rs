use chrono::DateTime;
use chrono_tz::Tz;

pub struct DateResult {
    pub date: DateTime<Tz>,
    pub add_year: bool,
    pub reset_hour: bool,
}

impl DateResult {
    pub fn new(date: DateTime<Tz>, add_year: bool, reset_hour: bool) -> Self {
        Self {
            date,
            add_year,
            reset_hour,
        }
    }
}
