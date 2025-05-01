use crate::model::roadwork::Roadwork;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct RoadworkData {
    /**
     * The name of the opendata source
     */
    pub(crate) source: String,
    pub(crate) roadworks: HashMap<String, Roadwork>,
    /**
     * The date of the last update
     */
    pub(crate) created: Duration,
}

impl<'a> IntoIterator for &'a mut RoadworkData {
    type Item = &'a mut Roadwork;
    type IntoIter = std::collections::hash_map::ValuesMut<'a, String, Roadwork>;

    fn into_iter(self) -> Self::IntoIter {
        self.roadworks.values_mut()
    }
}

impl RoadworkData {
    pub(crate) fn new(source: &str, roadworks: Vec<Roadwork>) -> Self {
        let mut roadworks_map = HashMap::new();
        roadworks.into_iter().for_each(|roadwork| {
            roadworks_map.insert(roadwork.id.clone(), roadwork);
        });
        Self {
            source: source.to_string(),
            roadworks: roadworks_map,
            created: SystemTime::now().duration_since(UNIX_EPOCH).unwrap(),
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Roadwork> {
        self.roadworks.values()
    }

    pub(crate) fn get_mut_roadwork(&mut self, id: &str) -> Option<&mut Roadwork> {
        self.roadworks.get_mut(id)
    }
}
