use serde::{Deserialize, Serialize};
use strum_macros::{Display, IntoStaticStr};

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct SyncData {
    #[serde(rename = "localUpdateTime")]
    pub(crate) local_update_time: u64,
    /**
     * Timestamp of the last server change
     */
    #[serde(rename = "serverUpdateTime")]
    pub(crate) server_update_time: u64,
    pub status: Status,
    pub(crate) dirty: bool,
}

impl SyncData {
    pub fn new_from(src: &SyncData) -> Self {
        Self {
            dirty: false,
            ..src.clone()
        }
    }

    pub fn copy(&mut self, other: &SyncData) {
        self.local_update_time = other.local_update_time;
        self.server_update_time = other.server_update_time;
        self.status = other.status;
    }

    pub(crate) fn update_time(&mut self, server_update_time: u64) {
        self.local_update_time = server_update_time;
        self.server_update_time = server_update_time;
    }
}

#[derive(
    Debug,
    Default,
    Display,
    Clone,
    Copy,
    Deserialize,
    Serialize,
    Ord,
    PartialOrd,
    PartialEq,
    Eq,
    IntoStaticStr,
)]
pub enum Status {
    #[default]
    New,
    Later,
    Ignored,
    Finished,
    Treated,
}
