use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, Deserialize, Serialize, Clone)]
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

impl Default for SyncData {
    fn default() -> Self {
        SyncData {
            local_update_time: 0,
            server_update_time: 0,
            status: Status::New,
            dirty: false,
        }
    }
}

impl SyncData {
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

#[derive(Debug, Clone, Copy, Deserialize, Serialize, Ord, PartialOrd, PartialEq, Eq)]
pub enum Status {
    New,
    Later,
    Ignored,
    Finished,
    Treated,
}

impl Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Status::New => "New",
            Status::Later => "Later",
            Status::Ignored => "Ignored",
            Status::Finished => "Finished",
            Status::Treated => "Treated",
        };
        write!(f, "{}", str)
    }
}
