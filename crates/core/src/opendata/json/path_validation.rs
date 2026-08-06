#[derive(Debug, Clone)]
pub struct PathValidation {
    pub label: &'static str,
    pub path: String,
    pub required: bool,
    pub expected: &'static str,
    pub failures: Vec<usize>,
    pub element_count: usize,
    pub message: Option<&'static str>,
}

impl PathValidation {
    pub fn new(
        label: &'static str,
        path: &str,
        required: bool,
        expected: &'static str,
        failures: Vec<usize>,
        element_count: usize,
        message: Option<&'static str>,
    ) -> Self {
        Self {
            label,
            path: path.to_string(),
            required,
            expected,
            failures,
            element_count,
            message,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.failures.is_empty()
    }
}
