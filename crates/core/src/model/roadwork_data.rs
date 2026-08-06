use crate::model::roadwork::Roadwork;
use crate::now_millis;
use roadwork_sync::SyncData;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Deserialize, Serialize)]
pub struct RoadworkData {
    pub source: String,
    pub roadworks: HashMap<String, Roadwork>,
    pub created: u64,
}

impl<'a> IntoIterator for &'a mut RoadworkData {
    type Item = &'a mut Roadwork;
    type IntoIter = std::collections::hash_map::ValuesMut<'a, String, Roadwork>;

    fn into_iter(self) -> Self::IntoIter {
        self.roadworks.values_mut()
    }
}

impl RoadworkData {
    pub fn new(source: &str, roadworks: Vec<Roadwork>) -> Self {
        let mut roadworks_map = HashMap::new();
        roadworks.into_iter().for_each(|roadwork| {
            roadworks_map.insert(roadwork.opendata.id.clone(), roadwork);
        });
        Self {
            source: source.to_string(),
            roadworks: roadworks_map,
            created: now_millis(),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Roadwork> {
        self.roadworks.values()
    }

    pub fn get_mut_roadwork(&mut self, id: &str) -> Option<&mut Roadwork> {
        self.roadworks.get_mut(id)
    }

    pub fn merge(&mut self, cached_data: &RoadworkData) {
        for existing_roadwork in cached_data.roadworks.values() {
            if let Some(new_roadwork) = self.roadworks.get_mut(&existing_roadwork.opendata.id) {
                new_roadwork.sync_data = SyncData::new_from(&existing_roadwork.sync_data);
            }
        }
    }

    pub fn apply_finished_status(&mut self) {
        for roadwork in self.roadworks.values_mut() {
            if roadwork.is_expired() {
                roadwork.sync_data.status = roadwork_sync::Status::Finished;
            }
        }
    }
}
