use walkers::TileId;
use walkers::sources::{Attribution, TileSource};

pub struct Waze;

impl TileSource for Waze {
    fn tile_url(&self, tile_id: TileId) -> String {
        format!(
            "https://www.waze.com/row-tiles/live/base/{}/{}/{}/tile.png?highres=true",
            tile_id.zoom, tile_id.x, tile_id.y
        )
    }

    fn attribution(&self) -> Attribution {
        Attribution {
            text: "Waze",
            url: "https://www.waze.com/fr/live-map",
            logo_light: None,
            logo_dark: None,
        }
    }
}
