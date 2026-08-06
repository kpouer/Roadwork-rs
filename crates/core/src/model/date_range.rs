use chrono::DateTime;
use chrono_tz::Tz;

pub struct DateRange {
    pub from: DateTime<Tz>,
    pub to: Option<DateTime<Tz>>,
}
