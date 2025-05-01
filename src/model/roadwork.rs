use crate::mapview::marker::{BaseMarkerTrait, Marker};
use crate::model::status_to_color;
use crate::model::wkt::polygon::Polygon;
use derive_builder::Builder;
use egui::Color32;
use roadwork_sync::SyncData;
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Default, Debug, Deserialize, Serialize, Builder)]
pub(crate) struct Roadwork {
    pub(crate) id: String,
    pub(crate) latitude: f64,
    pub(crate) longitude: f64,
    pub(crate) polygons: Option<Vec<Polygon>>,
    start: u64,
    end: u64,
    road: String,
    locationDetails: String,
    impactCirculationDetail: String,
    description: String,
    #[serde(skip)]
    markers: Vec<Marker>,
    pub(crate) syncData: Option<SyncData>,
    url: String,
}

impl Roadwork {
    pub(crate) fn isExpired(&self) -> bool {
        Duration::from_millis(self.end) < SystemTime::now().duration_since(UNIX_EPOCH).unwrap()
    }

    pub(crate) fn updateMarker(&mut self) {
        let color = self.getColor();
        self.markers.iter_mut().for_each(|marker| match marker {
            Marker::Polygon(polygonMarker) => {
                polygonMarker.setBorderColor(color);
                let background_color = Color32::RED.gamma_multiply(0.5);
                polygonMarker.set_color(background_color);
            }
            Marker::Rectangle(rectangle) => rectangle.set_color(color),
            Marker::Circle(circle) => circle.set_color(color),
        });
    }

    fn getColor(&self) -> Color32 {
        let status = self.syncData.as_ref().expect("syncData is None").status;
        status_to_color(status)
    }
}
