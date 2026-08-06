use chrono::DateTime;
use chrono_tz::Tz;

pub struct DateResult {
    pub date: DateTime<Tz>,
    pub add_year: bool,
    pub reset_hour: bool,
}
