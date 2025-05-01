use regex::Regex;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Parser {
    pub(crate) format: Option<String>,
    matcher: String,
    #[serde(default)]
    pub(crate) addYear: bool,
    #[serde(default)]
    pub(crate) resetHour: bool,
}

impl Default for Parser {
    fn default() -> Self {
        Self {
            format: None,
            matcher: String::new(),
            addYear: false,
            resetHour: true,
        }
    }
}

impl Parser {
    pub(crate) fn get_pattern(&self) -> Option<Regex> {
        //todo : can we cache the regex ?
        self.format
            .as_ref()
            .and_then(|format| Regex::new(format).ok())
    }
}
