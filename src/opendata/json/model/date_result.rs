use chrono::DateTime;
use chrono_tz::Tz;

pub(crate) struct DateResult {
    pub(crate) date: DateTime<Tz>,
    pub(crate) add_year: bool,
    pub(crate) reset_hour: bool,
}

impl DateResult {
    pub(crate) fn new(date: DateTime<Tz>, add_year: bool, reset_hour: bool) -> Self {
        Self {
            date,
            add_year,
            reset_hour,
        }
    }
}
