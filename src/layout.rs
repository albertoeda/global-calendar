//! Label placement: puts each label near its anchor without overlapping other labels or markers.
//! Pure geometry, no egui types, so it can be tested in isolation.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    /// True if the interiors overlap (touching edges do not count).
    pub fn intersects(&self, other: &Rect) -> bool {
        let _ = other;
        todo!()
    }

    pub fn contains_rect(&self, other: &Rect) -> bool {
        let _ = other;
        todo!()
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Item {
    /// Point the label belongs to (screen px).
    pub anchor: (f32, f32),
    /// Label width and height (px).
    pub size: (f32, f32),
    /// Higher priority items are placed first and get the best spots.
    pub priority: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub rect: Rect,
    /// The label is far enough from its anchor that a leader line should be drawn.
    pub leader: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    /// Markers are treated as squares of this half-size that labels must not cover.
    pub marker_radius: f32,
    /// Distance between a marker and an adjacent label.
    pub gap: f32,
    /// Minimum empty space between two labels.
    pub padding: f32,
    /// How far from its anchor a label may be pushed. `None` = anywhere inside `bounds`.
    pub max_leader: Option<f32>,
    /// Groups this close (px) with the same UTC offset may be merged when labels do not fit.
    pub merge_radius: f32,
}

/// Places `items` inside `bounds`, avoiding each other and the `obstacles` (marker centres).
/// Returns one entry per item, in input order; `None` if no free spot was found.
pub fn place(
    items: &[Item],
    obstacles: &[(f32, f32)],
    bounds: Rect,
    params: &Params,
) -> Vec<Option<Placement>> {
    let _ = (items, obstacles, bounds, params);
    todo!()
}
