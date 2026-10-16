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
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }

    pub fn contains_rect(&self, other: &Rect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.w <= self.x + self.w
            && other.y + other.h <= self.y + self.h
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// The rect grown by `m` on every side.
    pub fn expand(&self, m: f32) -> Rect {
        Rect::new(self.x - m, self.y - m, self.w + 2.0 * m, self.h + 2.0 * m)
    }

    /// Distance from `p` to the nearest point of the rect (0 if inside).
    pub fn distance_to(&self, p: (f32, f32)) -> f32 {
        let dx = (self.x - p.0).max(p.0 - (self.x + self.w)).max(0.0);
        let dy = (self.y - p.1).max(p.1 - (self.y + self.h)).max(0.0);
        (dx * dx + dy * dy).sqrt()
    }

    /// Moves the rect so it lies inside `bounds` (as far as its size allows).
    pub fn clamped_into(&self, bounds: &Rect) -> Rect {
        let x = self.x.min(bounds.x + bounds.w - self.w).max(bounds.x);
        let y = self.y.min(bounds.y + bounds.h - self.h).max(bounds.y);
        Rect::new(x, y, self.w, self.h)
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

/// Multipliers of the base offset tried (as leader positions) before falling back to the spiral.
const FAR_MULTIPLIERS: [f32; 4] = [2.0, 3.5, 5.0, 7.0];
/// Angular step of the spiral search.
const SPIRAL_ANGLES: usize = 20;

/// The label rect directly east of `anchor` (left edge `marker_radius + gap` right of it,
/// vertically centred). Used as the default/last-resort position.
pub fn east_of(anchor: (f32, f32), size: (f32, f32), params: &Params) -> Rect {
    around(anchor, size, params.marker_radius + params.gap)[0]
}

/// The 8 standard positions at offset `d`, in preference order: E, W, NE, NW, SE, SW, N, S.
fn around((ax, ay): (f32, f32), (w, h): (f32, f32), d: f32) -> [Rect; 8] {
    let mid_y = ay - h / 2.0;
    let above = ay - d - h;
    let below = ay + d;
    let right = ax + d;
    let left = ax - d - w;
    let centre_x = ax - w / 2.0;
    [
        Rect::new(right, mid_y, w, h),
        Rect::new(left, mid_y, w, h),
        Rect::new(right, above, w, h),
        Rect::new(left, above, w, h),
        Rect::new(right, below, w, h),
        Rect::new(left, below, w, h),
        Rect::new(centre_x, above, w, h),
        Rect::new(centre_x, below, w, h),
    ]
}

struct Ctx<'a> {
    obstacles: &'a [Rect],
    placed: Vec<Rect>,
    bounds: Rect,
    padding: f32,
}

impl Ctx<'_> {
    fn is_free(&self, r: &Rect) -> bool {
        self.bounds.contains_rect(r)
            && !self.obstacles.iter().any(|o| o.intersects(r))
            && !self
                .placed
                .iter()
                .any(|p| p.expand(self.padding).intersects(r))
    }
}

fn find_spot(item: &Item, ctx: &Ctx, params: &Params) -> Option<Placement> {
    let base = params.marker_radius + params.gap;
    let (w, h) = item.size;
    let within = |r: &Rect| {
        params
            .max_leader
            .is_none_or(|m| r.distance_to(item.anchor) <= m)
    };

    // 1. Adjacent positions.
    if let Some(r) = around(item.anchor, item.size, base)
        .into_iter()
        .find(|r| ctx.is_free(r))
    {
        return Some(Placement {
            rect: r,
            leader: false,
        });
    }

    // 2. Same positions, further out.
    for k in FAR_MULTIPLIERS {
        let cands = around(item.anchor, item.size, base * k);
        if !cands.iter().any(within) {
            break;
        }
        if let Some(r) = cands.into_iter().find(|r| within(r) && ctx.is_free(r)) {
            return Some(Placement {
                rect: r,
                leader: true,
            });
        }
    }

    // 3. Spiral of candidate centres.
    let b = &ctx.bounds;
    let diag = (b.w * b.w + b.h * b.h).sqrt();
    let half_diag_label = (w * w + h * h).sqrt() / 2.0;
    let step = (h / 2.0).max(2.0);
    let limit = match params.max_leader {
        Some(m) => (m + half_diag_label).min(diag + half_diag_label),
        None => diag + half_diag_label,
    };
    let mut radius = base + h / 2.0;
    let mut ring = 0usize;
    while radius <= limit {
        // Offset each ring by half an angle step so successive rings interleave.
        let phase = if ring % 2 == 1 { 0.5 } else { 0.0 };
        for a in 0..SPIRAL_ANGLES {
            let t = (a as f32 + phase) / SPIRAL_ANGLES as f32 * std::f32::consts::TAU;
            let cx = item.anchor.0 + radius * t.cos();
            let cy = item.anchor.1 - radius * t.sin();
            let r = Rect::new(cx - w / 2.0, cy - h / 2.0, w, h);
            if within(&r) && ctx.is_free(&r) {
                return Some(Placement {
                    rect: r,
                    leader: true,
                });
            }
        }
        radius += step;
        ring += 1;
    }
    None
}

/// Places `items` inside `bounds`, avoiding each other and the `obstacles` (marker centres).
/// Returns one entry per item, in input order; `None` if no free spot was found.
pub fn place(
    items: &[Item],
    obstacles: &[(f32, f32)],
    bounds: Rect,
    params: &Params,
) -> Vec<Option<Placement>> {
    let r = params.marker_radius;
    let obstacle_rects: Vec<Rect> = obstacles
        .iter()
        .map(|&(x, y)| Rect::new(x - r, y - r, 2.0 * r, 2.0 * r))
        .collect();
    let mut order: Vec<usize> = (0..items.len()).collect();
    // Stable sort keeps input order among equal priorities.
    order.sort_by(|&a, &b| items[b].priority.cmp(&items[a].priority));

    let mut ctx = Ctx {
        obstacles: &obstacle_rects,
        placed: Vec::with_capacity(items.len()),
        bounds,
        padding: params.padding,
    };
    let mut out = vec![None; items.len()];
    for i in order {
        if let Some(p) = find_spot(&items[i], &ctx, params) {
            ctx.placed.push(p.rect);
            out[i] = Some(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn params(max_leader: Option<f32>) -> Params {
        Params {
            marker_radius: 3.0,
            gap: 4.0,
            padding: 2.0,
            max_leader,
            merge_radius: 40.0,
        }
    }

    fn item(x: f32, y: f32, w: f32, h: f32) -> Item {
        Item {
            anchor: (x, y),
            size: (w, h),
            priority: 1,
        }
    }

    fn obstacle_rect(o: (f32, f32), r: f32) -> Rect {
        Rect::new(o.0 - r, o.1 - r, 2.0 * r, 2.0 * r)
    }

    #[test]
    fn rect_basics() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(a.intersects(&Rect::new(5.0, 5.0, 10.0, 10.0)));
        assert!(!a.intersects(&Rect::new(10.0, 0.0, 10.0, 10.0)));
        assert!(a.contains_rect(&Rect::new(0.0, 0.0, 10.0, 10.0)));
        assert!(!a.contains_rect(&Rect::new(1.0, 1.0, 10.0, 1.0)));
    }

    #[test]
    fn single_item_goes_east() {
        let p = params(Some(100.0));
        let bounds = Rect::new(0.0, 0.0, 800.0, 400.0);
        let out = place(
            &[item(100.0, 100.0, 80.0, 20.0)],
            &[(100.0, 100.0)],
            bounds,
            &p,
        );
        let pl = out[0].unwrap();
        assert!(!pl.leader);
        assert_eq!(pl.rect, Rect::new(107.0, 90.0, 80.0, 20.0));
    }

    #[test]
    fn item_at_right_edge_goes_west() {
        let p = params(Some(100.0));
        let bounds = Rect::new(0.0, 0.0, 800.0, 400.0);
        let out = place(
            &[item(790.0, 200.0, 80.0, 20.0)],
            &[(790.0, 200.0)],
            bounds,
            &p,
        );
        let pl = out[0].unwrap();
        assert!(!pl.leader);
        assert_eq!(pl.rect, Rect::new(790.0 - 7.0 - 80.0, 190.0, 80.0, 20.0));
    }

    #[test]
    fn same_anchor_both_placed() {
        let p = params(Some(100.0));
        let bounds = Rect::new(0.0, 0.0, 800.0, 400.0);
        let items = [
            item(300.0, 200.0, 80.0, 20.0),
            item(300.0, 200.0, 80.0, 20.0),
        ];
        let out = place(&items, &[(300.0, 200.0)], bounds, &p);
        let a = out[0].unwrap().rect;
        let b = out[1].unwrap().rect;
        assert!(!a.expand(p.padding).intersects(&b));
    }

    #[test]
    fn priority_gets_best_spot_and_order_is_preserved() {
        let p = params(Some(100.0));
        let bounds = Rect::new(0.0, 0.0, 800.0, 400.0);
        let mut items = [
            item(300.0, 200.0, 80.0, 20.0),
            item(300.0, 200.0, 80.0, 20.0),
        ];
        items[1].priority = 5;
        let out = place(&items, &[(300.0, 200.0)], bounds, &p);
        assert_eq!(out[1].unwrap().rect.x, 307.0);
        assert_eq!(out[1].unwrap().rect.y, 190.0);
        assert_ne!(out[0].unwrap().rect, out[1].unwrap().rect);
    }

    #[test]
    fn labels_never_cover_obstacles() {
        let p = params(None);
        let bounds = Rect::new(0.0, 0.0, 600.0, 300.0);
        let obstacles: Vec<(f32, f32)> = (0..10)
            .flat_map(|i| (0..5).map(move |j| (100.0 + i as f32 * 15.0, 100.0 + j as f32 * 15.0)))
            .collect();
        let items: Vec<Item> = obstacles
            .iter()
            .take(12)
            .map(|&(x, y)| item(x, y, 60.0, 18.0))
            .collect();
        let out = place(&items, &obstacles, bounds, &p);
        for pl in out.iter().flatten() {
            for &o in &obstacles {
                assert!(!pl.rect.intersects(&obstacle_rect(o, p.marker_radius)));
            }
        }
        assert!(out.iter().all(Option::is_some));
    }

    #[test]
    fn none_when_bounds_too_small() {
        let p = params(None);
        let bounds = Rect::new(0.0, 0.0, 50.0, 30.0);
        let out = place(&[item(25.0, 15.0, 80.0, 20.0)], &[(25.0, 15.0)], bounds, &p);
        assert_eq!(out, vec![None]);
    }

    #[test]
    fn max_leader_is_respected() {
        let p = params(Some(10.0));
        let bounds = Rect::new(0.0, 0.0, 800.0, 400.0);
        let items = [
            item(300.0, 200.0, 80.0, 20.0),
            item(300.0, 200.0, 80.0, 20.0),
            item(300.0, 200.0, 80.0, 20.0),
            item(300.0, 200.0, 80.0, 20.0),
            item(300.0, 200.0, 80.0, 20.0),
            item(300.0, 200.0, 80.0, 20.0),
            item(300.0, 200.0, 80.0, 20.0),
        ];
        let out = place(&items, &[(300.0, 200.0)], bounds, &p);
        for pl in out.iter().flatten() {
            assert!(pl.rect.distance_to((300.0, 200.0)) <= 10.0);
        }
        assert!(out.iter().any(Option::is_none));
    }

    #[test]
    fn timing_60_items() {
        let p = params(Some(120.0));
        let bounds = Rect::new(0.0, 0.0, 1600.0, 800.0);
        // Deterministic pseudo-random scatter, with a dense cluster (Europe-like).
        let mut s: u32 = 12345;
        let mut rnd = || {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (s >> 8) as f32 / (1u32 << 24) as f32
        };
        let items: Vec<Item> = (0..60)
            .map(|i| {
                let (x, y) = if i < 20 {
                    (780.0 + rnd() * 80.0, 200.0 + rnd() * 60.0)
                } else {
                    (rnd() * 1600.0, rnd() * 800.0)
                };
                item(x, y, 90.0 + rnd() * 40.0, 36.0)
            })
            .collect();
        let obstacles: Vec<(f32, f32)> = items.iter().map(|i| i.anchor).collect();
        let start = std::time::Instant::now();
        let n = 20;
        for _ in 0..n {
            std::hint::black_box(place(&items, &obstacles, bounds, &p));
        }
        let per = start.elapsed() / n;
        eprintln!("place(60 items): {per:?} per call");
    }

    fn arb_items() -> impl Strategy<Value = Vec<Item>> {
        prop::collection::vec(
            (
                0.0f32..1200.0,
                0.0f32..600.0,
                60.0f32..160.0,
                18.0f32..60.0,
                0u32..4,
            )
                .prop_map(|(x, y, w, h, priority)| Item {
                    anchor: (x, y),
                    size: (w, h),
                    priority,
                }),
            1..30,
        )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn placements_are_valid(items in arb_items()) {
            let p = params(None);
            let bounds = Rect::new(0.0, 0.0, 1200.0, 600.0);
            let obstacles: Vec<(f32, f32)> = items.iter().map(|i| i.anchor).collect();
            let out = place(&items, &obstacles, bounds, &p);
            prop_assert_eq!(out.len(), items.len());
            let placed: Vec<Rect> = out.iter().flatten().map(|pl| pl.rect).collect();
            for (i, a) in placed.iter().enumerate() {
                prop_assert!(bounds.contains_rect(a));
                for &o in &obstacles {
                    prop_assert!(!a.intersects(&obstacle_rect(o, p.marker_radius)));
                }
                for b in &placed[i + 1..] {
                    prop_assert!(!a.expand(p.padding).intersects(b));
                    prop_assert!(!b.expand(p.padding).intersects(a));
                }
            }
            if items.len() <= 8 {
                prop_assert!(out.iter().all(Option::is_some));
            }
        }
    }
}
