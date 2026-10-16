//! The dot-matrix world: an embedded land mask plus an equirectangular projection.

/// Mask cells per degree. Must match `tools/gen-assets`.
pub const MASK_RES: usize = 4;
/// Latitude range drawn on screen (polar regions are cropped like the reference design).
pub const LAT_TOP: f64 = 84.0;
pub const LAT_BOTTOM: f64 = -58.0;

pub struct LandMask {
    bits: &'static [u8],
}

impl LandMask {
    pub fn embedded() -> &'static LandMask {
        todo!()
    }

    pub fn is_land(&self, lat: f64, lon: f64) -> bool {
        let _ = (lat, lon, self.bits);
        todo!()
    }

    /// Fraction (0..=1) of mask cells inside the box that are land.
    pub fn coverage(&self, lat_min: f64, lat_max: f64, lon_min: f64, lon_max: f64) -> f32 {
        let _ = (lat_min, lat_max, lon_min, lon_max);
        todo!()
    }
}

/// Screen rectangle the map occupies, in pixels. Degrees are square (equal px per degree on both axes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Projection {
    /// The largest map rect that fits inside the given area, centred in it.
    pub fn fit(x: f32, y: f32, w: f32, h: f32) -> Projection {
        let _ = (x, y, w, h);
        todo!()
    }

    pub fn project(&self, lat: f64, lon: f64) -> (f32, f32) {
        let _ = (lat, lon);
        todo!()
    }

    pub fn px_per_deg(&self) -> f32 {
        todo!()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dot {
    pub lat: f64,
    pub lon: f64,
}

/// Centres of the `step_deg`-sized grid cells within the drawn latitude range that are mostly land.
pub fn dot_grid(mask: &LandMask, step_deg: f64) -> Vec<Dot> {
    let _ = (mask, step_deg);
    todo!()
}

/// Grid step (degrees) giving roughly 9–10 px between dots for a map this wide.
pub fn step_for_width(map_width_px: f32) -> f64 {
    let _ = map_width_px;
    todo!()
}
