use crate::opendata::json::model::parser::Parser;

pub(crate) struct DateResult {
    pub(crate) date: i64,
    pub(crate) parser: Parser,
}

impl DateResult {
    pub(crate) fn new(date: i64, parser: Parser) -> Self {
        Self { date, parser }
    }
}
