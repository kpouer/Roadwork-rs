use egui::Color32;

#[derive(Debug, Clone)]
pub(crate) enum Marker {
    Polygon(PolygonMarker),
    Rectangle(RectangleMarker),
    Circle(CircleMarker),
}

#[derive(Debug, Clone)]
struct BaseMarker {
    longitude: f64,
    latitude: f64,
    x: i32,
    y: i32,
    color: Color32,
    draggable: bool,
}

pub(crate) trait BaseMarkerTrait {
    fn set_color(&mut self, color: Color32);
}

#[derive(Debug, Clone)]
pub(crate) struct PolygonMarker {
    base: BaseMarker,
    border_color: Color32,
}

impl PolygonMarker {
    pub(crate) fn setBorderColor(&mut self, color: Color32) {
        self.border_color = color;
    }
}

impl BaseMarkerTrait for PolygonMarker {
    fn set_color(&mut self, color: Color32) {
        self.base.color = color;
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RectangleMarker {
    base: BaseMarker,
}

impl BaseMarkerTrait for RectangleMarker {
    fn set_color(&mut self, color: Color32) {
        self.base.color = color;
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CircleMarker {
    base: BaseMarker,
}

impl BaseMarkerTrait for CircleMarker {
    fn set_color(&mut self, color: Color32) {
        self.base.color = color;
    }
}
