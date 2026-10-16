//! The map canvas: day/night dot matrix, team markers and decluttered label blocks.

use super::style;
use crate::config::Person;
use crate::grouping::{self, Group};
use crate::layout::{self, Params};
use crate::solar;
use crate::theme::{Palette, mix};
use crate::timecalc::{self, Status};
use crate::worldmap::{self, Dot, LandMask, Projection};
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use eframe::egui::{
    self, Align2, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, text::LayoutJob, vec2,
};

const PAD_X: f32 = 7.0;
const PAD_Y: f32 = 4.0;
/// Name lines wrap at about this many characters.
const WRAP_CHARS: usize = 28;
/// Unfocused labels show at most this many name lines.
const MAX_NAME_LINES: usize = 2;

/// What a run of label text is, so it can be coloured by theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    City,
    Time(Status),
    Dim,
    Name,
}

pub type Line = Vec<(String, Role)>;

/// The text of a label block: a header (`City 14:30 +1d`) then names wrapped across lines.
/// Focused blocks list every name and add the UTC offset and timezone.
pub fn label_lines(
    group: &Group,
    people: &[Person],
    instant: DateTime<Utc>,
    viewer: Tz,
    focused: bool,
) -> Vec<Line> {
    let info = timecalc::local_info(instant, group.tz, viewer);
    let mut header = vec![
        (group.title(), Role::City),
        (" ".into(), Role::Dim),
        (
            info.time.format("%H:%M").to_string(),
            Role::Time(info.status),
        ),
    ];
    let delta = timecalc::format_day_delta(info.day_delta);
    if !delta.is_empty() {
        header.push((format!(" {delta}"), Role::Dim));
    }
    let mut lines = vec![header];

    let names: Vec<&str> = group
        .members
        .iter()
        .map(|&i| people[i].name.as_str())
        .collect();
    let mut wrapped = wrap_names(&names, WRAP_CHARS);
    if !focused && wrapped.len() > MAX_NAME_LINES {
        // Re-wrap with room for a "+N" marker on the last visible line.
        let shown: usize = wrapped[..MAX_NAME_LINES]
            .iter()
            .map(Vec::len)
            .sum::<usize>()
            - 1;
        let mut short = wrap_names(&names[..shown], WRAP_CHARS);
        short.truncate(MAX_NAME_LINES);
        let shown: usize = short.iter().map(Vec::len).sum();
        let more = format!("+{}", names.len() - shown);
        match short.last_mut() {
            Some(last) => last.push(more.leak()),
            None => short.push(vec![more.leak()]),
        }
        wrapped = short;
    }
    for chunk in wrapped {
        lines.push(vec![(chunk.join(" · "), Role::Name)]);
    }
    if focused {
        let zone = if group.cities.len() > 1 {
            group.cities.join(", ")
        } else {
            group.tz.name().to_string()
        };
        lines.push(vec![(
            format!("{} · {zone}", timecalc::format_offset(info.offset_secs)),
            Role::Dim,
        )]);
    }
    lines
}

/// Greedy wrap of names into lines no longer than `width` characters (joined with " · ").
fn wrap_names<'a>(names: &[&'a str], width: usize) -> Vec<Vec<&'a str>> {
    let mut lines: Vec<Vec<&str>> = Vec::new();
    let mut len = 0;
    for &name in names {
        let n = name.chars().count();
        match lines.last_mut() {
            Some(line) if len + 3 + n <= width => {
                line.push(name);
                len += 3 + n;
            }
            _ => {
                lines.push(vec![name]);
                len = n;
            }
        }
    }
    lines
}

fn line_chars(line: &Line) -> usize {
    line.iter().map(|(s, _)| s.chars().count()).sum()
}

pub struct MapInput<'a> {
    pub pal: &'a Palette,
    pub people: &'a [Person],
    pub instant: DateTime<Utc>,
    pub viewer: Tz,
    /// First member index of the focused group.
    pub focus: Option<usize>,
    pub errors: &'a [String],
}

#[derive(Default)]
pub struct MapView {
    dots: Option<(f64, Vec<Dot>)>,
    /// (lon, first member) for each group drawn last frame, west to east. Drives Tab focus.
    pub order: Vec<(f64, usize)>,
}

impl MapView {
    /// Draws the map. Returns the first member of a label the user clicked, if any.
    pub fn show(&mut self, ui: &mut egui::Ui, input: MapInput<'_>) -> Option<usize> {
        let pal = input.pal;
        let (canvas, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
        let painter = ui.painter_at(canvas);
        painter.rect_filled(canvas, 0.0, pal.bg);

        let area = canvas.shrink(12.0);
        let proj = Projection::fit(area.left(), area.top(), area.width(), area.height());
        self.draw_dots(&painter, &proj, input.instant, pal);

        let cw = ui
            .ctx()
            .fonts_mut(|f| f.glyph_width(&style::mono(style::LABEL_FONT), '0'));
        let row_h = ui
            .ctx()
            .fonts_mut(|f| f.row_height(&style::mono(style::LABEL_FONT)));
        let marker_r = (proj.px_per_deg() * 0.9).clamp(3.0, 5.0);
        let focused = |g: &Group| input.focus.is_some_and(|f| g.members.contains(&f));
        let project = |lat: f64, lon: f64| proj.project(lat, lon);
        let measure = |g: &Group| {
            let lines = label_lines(g, input.people, input.instant, input.viewer, focused(g));
            let chars = lines.iter().map(line_chars).max().unwrap_or(0);
            (
                chars as f32 * cw + 2.0 * PAD_X,
                lines.len() as f32 * row_h + 2.0 * PAD_Y,
            )
        };
        let bounds = layout::Rect::new(
            canvas.left() + 4.0,
            canvas.top() + 4.0,
            canvas.width() - 8.0,
            canvas.height() - 8.0,
        );
        let params = Params {
            marker_radius: marker_r + 1.5,
            gap: 4.0,
            padding: 4.0,
            max_leader: Some(90.0),
            merge_radius: 140.0,
        };
        let solved = grouping::solve(
            input.people,
            input.instant,
            project,
            measure,
            bounds,
            &params,
        );

        let mut order: Vec<(f64, usize)> = solved
            .groups
            .iter()
            .map(|g| (g.lon, g.members[0]))
            .collect();
        order.sort_by(|a, b| a.0.total_cmp(&b.0));
        self.order = order;

        // Leaders under markers, markers under labels.
        for (g, p) in solved.groups.iter().zip(&solved.placements) {
            let r = to_rect(p.rect);
            if p.leader || g.points.len() > 1 {
                for &(lat, lon) in &g.points {
                    let (x, y) = proj.project(lat, lon);
                    let target = Pos2::new(x, y);
                    let from =
                        Pos2::new(x.clamp(r.left(), r.right()), y.clamp(r.top(), r.bottom()));
                    painter.line_segment([from, target], Stroke::new(1.0, pal.subtext));
                }
            }
        }
        for g in &solved.groups {
            for &(lat, lon) in &g.points {
                let (x, y) = proj.project(lat, lon);
                let c = Pos2::new(x, y);
                painter.circle_filled(c, marker_r + 2.0, pal.bg);
                painter.circle_filled(c, marker_r, pal.accent);
                if focused(g) {
                    painter.circle_stroke(c, marker_r + 4.0, Stroke::new(1.5, pal.accent));
                }
            }
        }

        let mut clicked = None;
        for (i, (g, p)) in solved.groups.iter().zip(&solved.placements).enumerate() {
            let r = to_rect(p.rect);
            let is_focused = focused(g);
            let border = if is_focused {
                Stroke::new(1.5, pal.accent)
            } else {
                Stroke::new(1.0, pal.border)
            };
            painter.rect(r, 0.0, pal.panel, border, StrokeKind::Inside);
            let lines = label_lines(g, input.people, input.instant, input.viewer, is_focused);
            for (row, line) in lines.iter().enumerate() {
                let mut job = LayoutJob::default();
                for (text, role) in line {
                    let font = if *role == Role::City || matches!(role, Role::Time(_)) {
                        style::bold(style::LABEL_FONT)
                    } else {
                        style::mono(style::LABEL_FONT)
                    };
                    job.append(
                        text,
                        0.0,
                        egui::TextFormat::simple(font, role_color(pal, *role)),
                    );
                }
                let galley = painter.layout_job(job);
                let pos = Pos2::new(r.left() + PAD_X, r.top() + PAD_Y + row as f32 * row_h);
                painter.galley(pos, galley, pal.text);
            }
            let resp = ui.interact(r, ui.scope_id().with(("label", i)), Sense::click());
            if resp.clicked() {
                clicked = Some(g.members[0]);
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }

        if input.errors.is_empty() {
            if input.people.is_empty() {
                painter.text(
                    canvas.center(),
                    Align2::CENTER_CENTER,
                    "No team members yet — add some to team.toml",
                    style::mono(14.0),
                    pal.subtext,
                );
            }
        } else {
            draw_errors(&painter, canvas, input.errors, pal);
        }
        clicked
    }

    fn draw_dots(
        &mut self,
        painter: &egui::Painter,
        proj: &Projection,
        instant: DateTime<Utc>,
        pal: &Palette,
    ) {
        let step = worldmap::step_for_width(proj.w);
        if self.dots.as_ref().is_none_or(|(s, _)| *s != step) {
            self.dots = Some((step, worldmap::dot_grid(LandMask::embedded(), step)));
        }
        let Some((_, dots)) = &self.dots else { return };
        let sub = solar::subsolar_point(instant);
        let radius = (proj.px_per_deg() * step as f32 * 0.34).max(1.0);
        for d in dots {
            let (x, y) = proj.project(d.lat, d.lon);
            let day = solar::daylight(sub, d.lat, d.lon);
            painter.circle_filled(
                Pos2::new(x, y),
                radius,
                mix(pal.land_night, pal.land_day, day),
            );
        }
    }
}

fn to_rect(r: layout::Rect) -> Rect {
    Rect::from_min_size(Pos2::new(r.x, r.y), vec2(r.w, r.h))
}

fn role_color(pal: &Palette, role: Role) -> Color32 {
    match role {
        Role::City => pal.accent,
        Role::Time(s) => style::status_color(pal, s),
        Role::Dim => pal.subtext,
        Role::Name => pal.text,
    }
}

const MAX_ERRORS: usize = 6;

fn draw_errors(painter: &egui::Painter, canvas: Rect, errors: &[String], pal: &Palette) {
    let mut lines: Vec<String> = errors
        .iter()
        .take(MAX_ERRORS)
        .map(|e| format!("! {e}"))
        .collect();
    if errors.len() > MAX_ERRORS {
        lines.push(format!("  …and {} more", errors.len() - MAX_ERRORS));
    }
    lines.push("  fix team.toml and press R to reload".into());
    let font = style::mono(12.0);
    let galleys: Vec<_> = lines
        .into_iter()
        .enumerate()
        .map(|(i, l)| {
            let color = if i < errors.len().min(MAX_ERRORS) {
                pal.bad
            } else {
                pal.subtext
            };
            painter.layout(l, font.clone(), color, canvas.width() - 60.0)
        })
        .collect();
    let w = galleys.iter().map(|g| g.size().x).fold(0.0, f32::max) + 2.0 * PAD_X;
    let h: f32 = galleys.iter().map(|g| g.size().y).sum::<f32>() + 2.0 * PAD_Y;
    let r = Rect::from_min_size(
        Pos2::new(canvas.left() + 12.0, canvas.bottom() - 12.0 - h),
        vec2(w, h),
    );
    painter.rect(
        r,
        0.0,
        pal.panel,
        Stroke::new(1.0, pal.bad),
        StrokeKind::Inside,
    );
    let mut y = r.top() + PAD_Y;
    for g in galleys {
        let gh = g.size().y;
        painter.galley(Pos2::new(r.left() + PAD_X, y), g, pal.text);
        y += gh;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use chrono_tz::{America, Europe};

    fn person(name: &str) -> Person {
        Person {
            name: name.into(),
            city: "Paris".into(),
            lat: 48.85,
            lon: 2.35,
            tz: Europe::Paris,
        }
    }

    fn group(n: usize) -> Group {
        Group {
            cities: vec!["Paris".into()],
            points: vec![(48.85, 2.35)],
            lat: 48.85,
            lon: 2.35,
            tz: Europe::Paris,
            members: (0..n).collect(),
        }
    }

    fn text(line: &Line) -> String {
        line.iter().map(|(s, _)| s.as_str()).collect()
    }

    #[test]
    fn header_shows_city_time_and_day_delta() {
        let people = vec![person("Ana")];
        let t = Utc.with_ymd_and_hms(2026, 10, 15, 23, 30, 0).unwrap(); // 01:30 Paris, 16:30 LA
        let lines = label_lines(&group(1), &people, t, America::Los_Angeles, false);
        assert_eq!(text(&lines[0]), "Paris 01:30 +1d");
        assert_eq!(lines[0][2].1, Role::Time(Status::Night));
        assert_eq!(text(&lines[1]), "Ana");
    }

    #[test]
    fn long_name_lists_are_truncated_unless_focused() {
        let names = [
            "Alexandra",
            "Bartholomew",
            "Catherine",
            "Dimitri",
            "Evangeline",
            "Fitzgerald",
            "Guinevere",
        ];
        let people: Vec<Person> = names.iter().map(|n| person(n)).collect();
        let t = Utc.with_ymd_and_hms(2026, 10, 15, 12, 0, 0).unwrap();
        let g = group(people.len());

        let short = label_lines(&g, &people, t, Europe::Paris, false);
        assert_eq!(short.len(), 1 + MAX_NAME_LINES);
        assert!(text(short.last().unwrap()).contains('+'));
        assert!(short.iter().all(|l| line_chars(l) <= WRAP_CHARS + 4));

        let full = label_lines(&g, &people, t, Europe::Paris, true);
        let all: String = full.iter().map(text).collect();
        assert!(names.iter().all(|n| all.contains(n)));
        assert!(text(full.last().unwrap()).starts_with("UTC+2"));
    }

    #[test]
    fn wrap_keeps_long_names_whole() {
        let lines = wrap_names(&["A-very-long-name-that-exceeds-the-width", "Bo"], 10);
        assert_eq!(
            lines,
            vec![vec!["A-very-long-name-that-exceeds-the-width"], vec!["Bo"]]
        );
    }
}
