//! The dot-matrix world: an embedded land mask plus an equirectangular projection.

/// Mask cells per degree. Must match `tools/gen-assets`.
pub const MASK_RES: usize = 4;
/// Latitude range drawn on screen (polar regions are cropped like the reference design).
pub const LAT_TOP: f64 = 84.0;
pub const LAT_BOTTOM: f64 = -58.0;

const MASK_W: usize = 360 * MASK_RES;
const MASK_H: usize = 180 * MASK_RES;
const RES: f64 = MASK_RES as f64;

/// Minimum land fraction for a grid cell to get a dot.
const DOT_THRESHOLD: f32 = 0.3;
/// Target on-screen spacing between dots.
const TARGET_DOT_SPACING_PX: f64 = 9.5;

static MASK_BITS: &[u8] = include_bytes!("../assets/landmask.bin");
static EMBEDDED: LandMask = LandMask { bits: MASK_BITS };

const _: () = assert!(MASK_BITS.len() == MASK_W * MASK_H / 8);

pub struct LandMask {
    bits: &'static [u8],
}

impl LandMask {
    pub fn embedded() -> &'static LandMask {
        &EMBEDDED
    }

    pub fn is_land(&self, lat: f64, lon: f64) -> bool {
        self.cell(row_of(lat), col_of(lon))
    }

    /// Fraction (0..=1) of mask cells inside the box that are land.
    pub fn coverage(&self, lat_min: f64, lat_max: f64, lon_min: f64, lon_max: f64) -> f32 {
        // Cell centres: lat = 90 - (row + 0.5) / RES, lon = -180 + (col + 0.5) / RES.
        let r0 = (RES * (90.0 - lat_max) - 0.5).ceil().max(0.0) as i64;
        let r1 = (RES * (90.0 - lat_min) - 0.5)
            .floor()
            .min(MASK_H as f64 - 1.0) as i64;
        let c0 = (RES * (lon_min + 180.0) - 0.5).ceil() as i64;
        let c1 = (RES * (lon_max + 180.0) - 0.5).floor() as i64;
        if r1 < r0 || c1 < c0 {
            let land = self.is_land((lat_min + lat_max) / 2.0, (lon_min + lon_max) / 2.0);
            return if land { 1.0 } else { 0.0 };
        }
        let (mut land, mut total) = (0u32, 0u32);
        for r in r0..=r1 {
            for c in c0..=c1 {
                total += 1;
                let col = c.rem_euclid(MASK_W as i64) as usize;
                if self.cell(r as usize, col) {
                    land += 1;
                }
            }
        }
        land as f32 / total as f32
    }

    fn cell(&self, row: usize, col: usize) -> bool {
        let i = row * MASK_W + col;
        (self.bits[i / 8] >> (i % 8)) & 1 == 1
    }
}

/// Row of the cell containing `lat` (clamped to the mask).
fn row_of(lat: f64) -> usize {
    let r = ((90.0 - lat) * RES).floor();
    r.clamp(0.0, MASK_H as f64 - 1.0) as usize
}

/// Column of the cell containing `lon` (wrapped around the globe).
fn col_of(lon: f64) -> usize {
    let c = ((lon + 180.0) * RES).floor() as i64;
    c.rem_euclid(MASK_W as i64) as usize
}

/// Screen rectangle the map occupies, in pixels. Degrees are square (equal px per degree on both axes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

const LON_SPAN: f32 = 360.0;
const LAT_SPAN: f32 = (LAT_TOP - LAT_BOTTOM) as f32;

impl Projection {
    /// The largest map rect that fits inside the given area, centred in it.
    pub fn fit(x: f32, y: f32, w: f32, h: f32) -> Projection {
        let scale = (w / LON_SPAN).min(h / LAT_SPAN).max(0.0);
        let (mw, mh) = (LON_SPAN * scale, LAT_SPAN * scale);
        Projection {
            x: x + (w - mw) / 2.0,
            y: y + (h - mh) / 2.0,
            w: mw,
            h: mh,
        }
    }

    pub fn project(&self, lat: f64, lon: f64) -> (f32, f32) {
        let px = self.x + ((lon + 180.0) as f32 / LON_SPAN) * self.w;
        let py = self.y + ((LAT_TOP - lat) as f32 / LAT_SPAN) * self.h;
        (px, py)
    }

    pub fn px_per_deg(&self) -> f32 {
        self.w / LON_SPAN
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dot {
    pub lat: f64,
    pub lon: f64,
}

/// Centres of the `step_deg`-sized grid cells within the drawn latitude range that are mostly land.
pub fn dot_grid(mask: &LandMask, step_deg: f64) -> Vec<Dot> {
    let mut dots = Vec::new();
    if step_deg.is_nan() || step_deg <= 0.0 {
        return dots;
    }
    let cols = (360.0 / step_deg).floor() as usize;
    let rows = ((LAT_TOP - LAT_BOTTOM) / step_deg).floor() as usize;
    for r in 0..rows {
        let lat_max = LAT_TOP - r as f64 * step_deg;
        let lat_min = lat_max - step_deg;
        for c in 0..cols {
            let lon_min = -180.0 + c as f64 * step_deg;
            let lon_max = lon_min + step_deg;
            if mask.coverage(lat_min, lat_max, lon_min, lon_max) >= DOT_THRESHOLD {
                dots.push(Dot {
                    lat: (lat_min + lat_max) / 2.0,
                    lon: (lon_min + lon_max) / 2.0,
                });
            }
        }
    }
    dots
}

/// Grid step (degrees) giving roughly 9–10 px between dots for a map this wide.
pub fn step_for_width(map_width_px: f32) -> f64 {
    if !map_width_px.is_finite() || map_width_px <= 0.0 {
        return 4.0;
    }
    let raw = 360.0 / (map_width_px as f64 / TARGET_DOT_SPACING_PX);
    ((raw * 4.0).round() / 4.0).clamp(1.0, 4.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask() -> &'static LandMask {
        LandMask::embedded()
    }

    #[test]
    fn known_land_points() {
        assert!(mask().is_land(51.5074, -0.1278), "London");
        assert!(mask().is_land(40.7128, -74.0060), "New York");
        assert!(mask().is_land(-33.8688, 151.2093), "Sydney");
        assert!(mask().is_land(-1.2921, 36.8219), "Nairobi");
    }

    #[test]
    fn known_water_points() {
        assert!(!mask().is_land(30.0, -40.0), "mid-Atlantic");
        assert!(!mask().is_land(0.0, -150.0), "mid-Pacific");
        assert!(!mask().is_land(42.0, 50.5), "Caspian Sea");
    }

    #[test]
    fn is_land_wraps_and_clamps() {
        assert_eq!(
            mask().is_land(51.5, -0.13),
            mask().is_land(51.5, 359.87),
            "longitude wraps"
        );
        // Out-of-range latitudes clamp instead of panicking.
        assert!(mask().is_land(-95.0, 0.0), "Antarctica");
        let _ = mask().is_land(95.0, 0.0);
    }

    #[test]
    fn coverage_bounds() {
        assert_eq!(mask().coverage(29.0, 31.0, -41.0, -39.0), 0.0);
        let c = mask().coverage(-25.0, -20.0, 130.0, 135.0);
        assert!(c > 0.95, "central Australia: {c}");
        // A tiny box falls back to the nearest cell.
        assert_eq!(mask().coverage(51.5, 51.51, -0.13, -0.12), 1.0);
    }

    #[test]
    fn projection_corners_and_aspect() {
        let p = Projection::fit(10.0, 20.0, 1000.0, 1000.0);
        assert!((p.w / p.h - 360.0 / 142.0).abs() < 1e-4);
        assert!((p.w - 1000.0).abs() < 1e-3);
        assert!((p.y - (20.0 + (1000.0 - p.h) / 2.0)).abs() < 1e-3);
        let (x0, y0) = p.project(LAT_TOP, -180.0);
        assert!((x0 - p.x).abs() < 1e-3 && (y0 - p.y).abs() < 1e-3);
        let (x1, y1) = p.project(LAT_BOTTOM, 180.0);
        assert!((x1 - (p.x + p.w)).abs() < 1e-3 && (y1 - (p.y + p.h)).abs() < 1e-3);
        assert!((p.px_per_deg() - p.w / 360.0).abs() < 1e-6);

        // Wide area: height-limited and centred horizontally.
        let q = Projection::fit(0.0, 0.0, 2000.0, 284.0);
        assert!((q.h - 284.0).abs() < 1e-3 && (q.w - 720.0).abs() < 1e-3);
        assert!((q.x - 640.0).abs() < 1e-3);
    }

    #[test]
    fn dot_grid_is_plausible() {
        let dots = dot_grid(mask(), 2.0);
        eprintln!("dot_grid(2.0): {} dots", dots.len());
        // ~4100 at the time of writing: equirectangular cells over-weight high-latitude land
        // (Canada, Siberia, Greenland), so this is above the naive 29%-of-the-globe estimate.
        assert!((1500..=4500).contains(&dots.len()), "{}", dots.len());
        assert!(
            dots.iter()
                .any(|d| (d.lat - 51.5).abs() <= 2.0 && (d.lon + 0.1).abs() <= 2.0),
            "dot near London"
        );
        for d in &dots {
            assert!(d.lat < LAT_TOP && d.lat > LAT_BOTTOM);
            assert!(d.lon > -180.0 && d.lon < 180.0);
        }
    }

    #[test]
    fn step_for_width_cases() {
        assert_eq!(step_for_width(100.0), 4.0);
        assert_eq!(step_for_width(855.0), 4.0);
        assert_eq!(step_for_width(1368.0), 2.5);
        assert_eq!(step_for_width(1710.0), 2.0);
        assert_eq!(step_for_width(3420.0), 1.0);
        assert_eq!(step_for_width(10000.0), 1.0);
        assert_eq!(step_for_width(0.0), 4.0);
    }

    #[test]
    #[ignore = "visual check: prints an ASCII render of the dot grid"]
    fn ascii_render() {
        let step = 2.0;
        let cols = (360.0 / step) as usize;
        let rows = ((LAT_TOP - LAT_BOTTOM) / step) as usize;
        let mut grid = vec![vec![' '; cols]; rows];
        for d in dot_grid(mask(), step) {
            let r = ((LAT_TOP - d.lat) / step) as usize;
            let c = ((d.lon + 180.0) / step) as usize;
            grid[r][c] = '#';
        }
        for row in grid {
            println!("|{}|", row.into_iter().collect::<String>());
        }
    }
}
