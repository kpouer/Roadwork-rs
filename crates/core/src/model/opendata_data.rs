use crate::model::opendata::Opendata;
use crate::now_millis;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpendataData {
    pub source: String,
    pub opendata: HashMap<String, Opendata>,
    pub created: u64,
}

impl OpendataData {
    pub fn new(source: &str, opendata: Vec<Opendata>) -> Self {
        let mut opendata_map = HashMap::new();
        let mut next_auto: u64 = 1;
        for mut item in opendata {
            let key = if !item.id.is_empty() && !opendata_map.contains_key(&item.id) {
                item.id.clone()
            } else {
                loop {
                    let candidate = next_auto.to_string();
                    next_auto += 1;
                    if !opendata_map.contains_key(&candidate) {
                        break candidate;
                    }
                }
            };
            item.id = key;
            opendata_map.insert(item.id.clone(), item);
        }
        Self {
            source: source.to_string(),
            opendata: opendata_map,
            created: now_millis(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Opendata> {
        self.opendata.values()
    }

    pub fn get_mut_opendata(&mut self, id: &str) -> Option<&mut Opendata> {
        self.opendata.get_mut(id)
    }
}
