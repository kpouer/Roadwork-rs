pub(crate) struct DateRange {
    pub(crate) from: i64,
    pub(crate) to: i64,
}

impl DateRange {
    pub(crate) fn new(from: i64, to: i64) -> Self {
        Self { from, to }
    }
}
