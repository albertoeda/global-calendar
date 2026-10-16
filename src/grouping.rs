//! Groups people into map labels: one per city, merging nearby same-offset cities when labels do not fit.

use crate::config::Person;
use crate::layout::{self, Item, Params, Placement, Rect};
use crate::timecalc;
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
        match self.cities.as_slice() {
            [] => String::new(),
            [one] => one.clone(),
            [a, b] => format!("{a}/{b}"),
            [a, b, rest @ ..] => format!("{a}/{b}+{}", rest.len()),
        }
    }

    /// Merges `other` into `self` (cities/points/members appended; centroid weighted by members).
    fn absorb(&mut self, other: Group) {
        let (n1, n2) = (self.members.len() as f64, other.members.len() as f64);
        let total = (n1 + n2).max(1.0);
        self.lat = (self.lat * n1 + other.lat * n2) / total;
        self.lon = (self.lon * n1 + other.lon * n2) / total;
        if other.members.len() > self.members.len() {
            self.tz = other.tz;
        }
        self.cities.extend(other.cities);
        self.points.extend(other.points);
        self.members.extend(other.members);
    }
}

fn location_key(lat: f64, lon: f64) -> (i64, i64) {
    ((lat * 100.0).round() as i64, (lon * 100.0).round() as i64)
}

/// One group per distinct location (coordinates rounded to 0.01°), in first-seen order.
pub fn group_by_city(people: &[Person]) -> Vec<Group> {
    let mut keys: Vec<(i64, i64)> = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    for (i, p) in people.iter().enumerate() {
        let key = location_key(p.lat, p.lon);
        match keys.iter().position(|k| *k == key) {
            Some(g) => groups[g].members.push(i),
            None => {
                keys.push(key);
                groups.push(Group {
                    cities: vec![p.city.clone()],
                    points: vec![(p.lat, p.lon)],
                    lat: p.lat,
                    lon: p.lon,
                    tz: p.tz,
                    members: vec![i],
                });
            }
        }
    }
    groups
}

pub struct Solved {
    pub groups: Vec<Group>,
    /// One per group, same order.
    pub placements: Vec<Placement>,
}

const MAX_MERGE_ROUNDS: usize = 3;

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
    let mut groups = group_by_city(people);

    // Markers never move, whatever the grouping.
    let obstacles: Vec<(f32, f32)> = groups
        .iter()
        .flat_map(|g| g.points.iter().map(|&(lat, lon)| project(lat, lon)))
        .collect();

    let items_for = |groups: &[Group]| -> Vec<Item> {
        groups
            .iter()
            .map(|g| Item {
                anchor: project(g.lat, g.lon),
                size: measure(g),
                priority: g.members.len() as u32,
            })
            .collect()
    };

    let mut items = items_for(&groups);
    let mut placements = layout::place(&items, &obstacles, bounds, params);

    for _ in 0..MAX_MERGE_ROUNDS {
        if placements.iter().all(Option::is_some) {
            break;
        }
        let offsets: Vec<i32> = groups
            .iter()
            .map(|g| timecalc::offset_secs(instant, g.tz))
            .collect();
        let mut slots: Vec<Option<Group>> = groups.into_iter().map(Some).collect();
        let mut merged = vec![false; slots.len()];
        let mut any = false;

        for i in 0..slots.len() {
            if placements[i].is_some() || merged[i] || slots[i].is_none() {
                continue;
            }
            let (ax, ay) = items[i].anchor;
            let nearest = (0..slots.len())
                .filter(|&j| j != i && !merged[j] && slots[j].is_some())
                .filter(|&j| offsets[j] == offsets[i])
                .map(|j| {
                    let (bx, by) = items[j].anchor;
                    (j, (bx - ax).hypot(by - ay))
                })
                .filter(|&(_, d)| d <= params.merge_radius)
                .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
            if let Some((j, _)) = nearest {
                let (keep, gone) = (i.min(j), i.max(j));
                let other = slots[gone].take().expect("slot checked above");
                slots[keep]
                    .as_mut()
                    .expect("slot checked above")
                    .absorb(other);
                merged[keep] = true;
                merged[gone] = true;
                any = true;
            }
        }

        groups = slots.into_iter().flatten().collect();
        if !any {
            break;
        }
        items = items_for(&groups);
        placements = layout::place(&items, &obstacles, bounds, params);
    }

    if placements.iter().any(Option::is_none) {
        let relaxed = Params {
            max_leader: None,
            ..*params
        };
        placements = layout::place(&items, &obstacles, bounds, &relaxed);
    }

    let placements = placements
        .into_iter()
        .zip(&items)
        .map(|(p, item)| {
            p.unwrap_or_else(|| Placement {
                rect: layout::east_of(item.anchor, item.size, params).clamped_into(&bounds),
                leader: false,
            })
        })
        .collect();

    Solved { groups, placements }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::Europe;

    fn person(name: &str, city: &str, lat: f64, lon: f64, tz: Tz) -> Person {
        Person {
            name: name.into(),
            city: city.into(),
            lat,
            lon,
            tz,
        }
    }

    fn group(cities: &[&str]) -> Group {
        Group {
            cities: cities.iter().map(|c| c.to_string()).collect(),
            points: vec![(0.0, 0.0); cities.len()],
            lat: 0.0,
            lon: 0.0,
            tz: Tz::UTC,
            members: (0..cities.len()).collect(),
        }
    }

    fn europe() -> Vec<Person> {
        vec![
            person("Ann", "London", 51.5074, -0.1278, Europe::London),
            person("Bob", "Paris", 48.8566, 2.3522, Europe::Paris),
            person("Cem", "Brussels", 50.8503, 4.3517, Europe::Brussels),
            person("Dia", "Amsterdam", 52.3676, 4.9041, Europe::Amsterdam),
        ]
    }

    fn params() -> Params {
        Params {
            marker_radius: 3.0,
            gap: 4.0,
            padding: 2.0,
            max_leader: Some(20.0),
            merge_radius: 60.0,
        }
    }

    fn equirect(w: f32, h: f32) -> impl Fn(f64, f64) -> (f32, f32) {
        move |lat, lon| {
            (
                ((lon + 180.0) / 360.0) as f32 * w,
                ((90.0 - lat) / 180.0) as f32 * h,
            )
        }
    }

    fn instant() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).unwrap()
    }

    #[test]
    fn titles() {
        assert_eq!(group(&["London"]).title(), "London");
        assert_eq!(group(&["Paris", "Brussels"]).title(), "Paris/Brussels");
        assert_eq!(
            group(&["Paris", "Brussels", "Amsterdam", "Berlin"]).title(),
            "Paris/Brussels+2"
        );
        assert_eq!(
            group(&["Paris", "Brussels", "Amsterdam"]).title(),
            "Paris/Brussels+1"
        );
    }

    #[test]
    fn same_coordinates_one_group() {
        let people = vec![
            person("Ann", "London", 51.5074, -0.1278, Europe::London),
            person("Bob", "Paris", 48.8566, 2.3522, Europe::Paris),
            person("Cat", "London", 51.5071, -0.1281, Europe::London),
        ];
        let groups = group_by_city(&people);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].cities, vec!["London".to_string()]);
        assert_eq!(groups[0].members, vec![0, 2]);
        assert_eq!(groups[0].points, vec![(51.5074, -0.1278)]);
        assert_eq!(groups[1].members, vec![1]);
    }

    #[test]
    fn cramped_canvas_merges_same_offset_only() {
        let people = europe();
        let bounds = Rect::new(0.0, 0.0, 300.0, 150.0);
        let solved = solve(
            &people,
            instant(),
            equirect(300.0, 150.0),
            |_| (110.0, 40.0),
            bounds,
            &params(),
        );
        assert!(solved.groups.len() <= 2, "groups: {:?}", solved.groups);
        assert_eq!(solved.placements.len(), solved.groups.len());
        let london = solved
            .groups
            .iter()
            .find(|g| g.members.contains(&0))
            .unwrap();
        assert_eq!(london.members, vec![0]);
        assert_eq!(london.cities, vec!["London".to_string()]);
        // Every member is in exactly one group.
        let mut all: Vec<usize> = solved
            .groups
            .iter()
            .flat_map(|g| g.members.clone())
            .collect();
        all.sort();
        assert_eq!(all, vec![0, 1, 2, 3]);
        for p in &solved.placements {
            assert!(bounds.contains_rect(&p.rect));
        }
    }

    #[test]
    fn big_canvas_no_merging() {
        let people = europe();
        let bounds = Rect::new(0.0, 0.0, 4000.0, 2000.0);
        let solved = solve(
            &people,
            instant(),
            equirect(4000.0, 2000.0),
            |_| (110.0, 40.0),
            bounds,
            &params(),
        );
        assert_eq!(solved.groups.len(), 4);
        assert_eq!(solved.placements.len(), 4);
        for (g, i) in solved.groups.iter().zip(0..) {
            assert_eq!(g.members, vec![i]);
        }
    }

    #[test]
    fn every_group_gets_a_placement_even_when_impossible() {
        let people = europe();
        let bounds = Rect::new(0.0, 0.0, 120.0, 60.0);
        let solved = solve(
            &people,
            instant(),
            equirect(120.0, 60.0),
            |_| (110.0, 40.0),
            bounds,
            &params(),
        );
        assert_eq!(solved.placements.len(), solved.groups.len());
        assert!(!solved.groups.is_empty());
        for p in &solved.placements {
            assert!(bounds.contains_rect(&p.rect));
        }
    }

    #[test]
    fn empty_people() {
        let solved = solve(
            &[],
            instant(),
            equirect(300.0, 150.0),
            |_| (110.0, 40.0),
            Rect::new(0.0, 0.0, 300.0, 150.0),
            &params(),
        );
        assert!(solved.groups.is_empty());
        assert!(solved.placements.is_empty());
    }
}
