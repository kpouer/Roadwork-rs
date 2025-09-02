use egui::TextBuffer;
use std::any::TypeId;
use std::ops::Range;

pub(crate) struct DummyTextBuffer<'a> {
    text: &'a str,
}

impl<'a> Default for DummyTextBuffer<'a> {
    fn default() -> Self {
        Self { text: "" }
    }
}

impl<'a> From<&'a str> for DummyTextBuffer<'a> {
    fn from(text: &'a str) -> Self {
        Self { text }
    }
}

impl<'a> From<&'a Option<String>> for DummyTextBuffer<'a> {
    fn from(text: &'a Option<String>) -> Self {
        match text.as_ref() {
            None => Self::default(),
            Some(text) => Self::new(text),
        }
    }
}

impl<'a> DummyTextBuffer<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        Self { text }
    }
}

impl TextBuffer for DummyTextBuffer<'_> {
    fn is_mutable(&self) -> bool {
        false
    }

    fn as_str(&self) -> &str {
        self.text
    }

    fn insert_text(&mut self, _text: &str, _char_index: usize) -> usize {
        0
    }

    fn delete_char_range(&mut self, _char_range: Range<usize>) {}

    fn type_id(&self) -> TypeId {
        std::any::TypeId::of::<&str>()
    }
}
