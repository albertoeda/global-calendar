//! Groups people into map labels: one per city, merging nearby same-offset cities when labels do not fit.

use crate::config::Person;
use crate::layout::{Params, Placement, Rect};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// Display names of the cities in this group (one unless merged).
    pub cities: Vec<String>,
    /// Location (lat, lon) of each city in the group, same order as `cities`.
    pub points: Vec<(f64, f64)>,
    /// Label anchor: member-weighted centroid of `points`.
    pub lat: f64,
    pub lon: f64,
    /// Timezone of the first city.
    pub tz: Tz,
    /// Indexes into the people slice.
    pub members: Vec<usize>,
}

impl Group {
    /// "London", "Paris/Brussels", "Paris/Brussels+2".
    pub fn title(&self) -> String {
        todo!()
    }
}

/// One group per distinct location (coordinates rounded to 0.01°), in first-seen order.
pub fn group_by_city(people: &[Person]) -> Vec<Group> {
    let _ = people;
    todo!()
}

pub struct Solved {
    pub groups: Vec<Group>,
    /// One per group, same order.
    pub placements: Vec<Placement>,
}

/// Groups people and places a label for every group.
///
/// 1. One group per city; place labels with `params.max_leader`.
/// 2. While some label has no spot (at most 3 rounds): merge each unplaced group into the nearest
///    group within `params.merge_radius` px whose UTC offset at `instant` is the same, then re-place.
/// 3. Re-place with no leader limit; anything still unplaced sits at its anchor, clamped into bounds.
pub fn solve(
    people: &[Person],
    instant: DateTime<Utc>,
    project: impl Fn(f64, f64) -> (f32, f32),
    measure: impl Fn(&Group) -> (f32, f32),
    bounds: Rect,
    params: &Params,
) -> Solved {
    let _ = (people, instant, &project, &measure, bounds, params);
    todo!()
}
