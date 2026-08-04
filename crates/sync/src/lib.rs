use serde::{Deserialize, Serialize};
use strum_macros::{Display, IntoStaticStr};

#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct SyncData {
    #[serde(rename = "localUpdateTime")]
    pub local_update_time: u64,
    /**
     * Timestamp of the last server change
     */
    #[serde(rename = "serverUpdateTime")]
    pub server_update_time: u64,
    pub status: Status,
    pub dirty: bool,
}

impl SyncData {
    pub fn new_from(src: &SyncData) -> Self {
        Self {
            dirty: false,
            ..src.clone()
        }
    }

    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }

    pub fn copy(&mut self, other: &SyncData) {
        self.local_update_time = other.local_update_time;
        self.server_update_time = other.server_update_time;
        self.status = other.status;
    }

    pub fn local_update_time(&self) -> u64 {
        self.local_update_time
    }

    pub fn server_update_time(&self) -> u64 {
        self.server_update_time
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
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

impl<T: AsRef<str>> From<T> for Status {
    fn from(s: T) -> Self {
        match s.as_ref() {
            "New" => Status::New,
            "Later" => Status::Later,
            "Ignored" => Status::Ignored,
            "Finished" => Status::Finished,
            "Treated" => Status::Treated,
            _ => Status::New,
        }
    }
}
