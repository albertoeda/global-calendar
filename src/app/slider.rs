//! The 24-hour timeline: hour ruler, team-availability band and a draggable handle.

use super::style;
use crate::config::Person;
use crate::theme::{Palette, mix};
use crate::timecalc::{self, Status};
use chrono::NaiveDate;
use chrono_tz::Tz;
use eframe::egui::{self, Align2, Pos2, Rect, Sense, Stroke, StrokeKind, vec2};

const SLOTS: usize = (24 * 60 / super::STEP_MINUTES) as usize;
const HEIGHT: f32 = 46.0;

/// Fraction of the team inside working hours for each 15-minute slot of a viewer-local day.
#[derive(Default)]
pub struct AvailabilityCache {
    key: Option<(NaiveDate, Tz, u64)>,
    work: Vec<f32>,
}

impl AvailabilityCache {
    fn get(&mut self, people: &[Person], viewer: Tz, date: NaiveDate, generation: u64) -> &[f32] {
        if self.key != Some((date, viewer, generation)) {
            self.key = Some((date, viewer, generation));
            self.work = (0..SLOTS)
                .map(|slot| {
                    if people.is_empty() {
                        return 0.0;
                    }
                    let minutes = slot as i64 * super::STEP_MINUTES;
                    let t = timecalc::instant_from_local(viewer, date, minutes);
                    let working = people
                        .iter()
                        .filter(|p| timecalc::local_info(t, p.tz, viewer).status == Status::Work)
                        .count();
                    working as f32 / people.len() as f32
                })
                .collect();
        }
        &self.work
    }
}

pub struct SliderInput<'a> {
    pub pal: &'a Palette,
    pub people: &'a [Person],
    pub viewer: Tz,
    pub date: NaiveDate,
    pub generation: u64,
    /// Handle position, minutes past viewer-local midnight.
    pub minutes: f64,
    /// Where "now" is on this day, if it is today.
    pub now_minutes: Option<f64>,
    pub live: bool,
}

/// Draws the timeline. Returns the minute the user picked with the mouse, snapped to 15 minutes.
pub fn show(
    ui: &mut egui::Ui,
    cache: &mut AvailabilityCache,
    input: SliderInput<'_>,
) -> Option<i64> {
    let pal = input.pal;
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), HEIGHT), Sense::click_and_drag());
    let painter = ui.painter_at(rect.expand(2.0));

    let left = rect.left() + 18.0;
    let right = rect.right() - 18.0;
    let x_at = |minutes: f64| left + (right - left) * (minutes / (24.0 * 60.0)) as f32;
    let band = Rect::from_min_max(
        Pos2::new(left, rect.top() + 16.0),
        Pos2::new(right, rect.top() + 28.0),
    );

    // Availability band: brighter where more of the team is working.
    let work = cache.get(input.people, input.viewer, input.date, input.generation);
    let slot_w = (right - left) / SLOTS as f32;
    painter.rect_filled(band, 0.0, mix(pal.panel, pal.border, 0.6));
    for (i, &frac) in work.iter().enumerate() {
        if frac > 0.0 {
            let x = left + i as f32 * slot_w;
            let r = Rect::from_min_max(
                Pos2::new(x, band.top()),
                Pos2::new(x + slot_w + 0.5, band.bottom()),
            );
            painter.rect_filled(r, 0.0, mix(pal.panel, pal.ok, 0.25 + 0.6 * frac));
        }
    }
    painter.rect_stroke(band, 0.0, Stroke::new(1.0, pal.border), StrokeKind::Outside);

    // Hour ruler.
    for h in 0..=24 {
        let x = x_at(f64::from(h) * 60.0);
        let len = if h % 3 == 0 { 5.0 } else { 3.0 };
        painter.line_segment(
            [
                Pos2::new(x, band.bottom() + 1.0),
                Pos2::new(x, band.bottom() + 1.0 + len),
            ],
            Stroke::new(1.0, pal.subtext),
        );
        if h % 3 == 0 {
            painter.text(
                Pos2::new(x, band.bottom() + 7.0),
                Align2::CENTER_TOP,
                format!("{h:02}"),
                style::mono(style::SMALL_FONT),
                pal.subtext,
            );
        }
    }

    if let Some(now) = input.now_minutes {
        let x = x_at(now);
        painter.line_segment(
            [
                Pos2::new(x, band.top() - 2.0),
                Pos2::new(x, band.bottom() + 2.0),
            ],
            Stroke::new(1.0, pal.text),
        );
    }

    // Handle with its time printed above it.
    let hx = x_at(input.minutes);
    painter.rect_filled(
        Rect::from_center_size(
            Pos2::new(hx, band.center().y),
            vec2(4.0, band.height() + 8.0),
        ),
        0.0,
        pal.accent,
    );
    let total = input.minutes.round() as i64;
    let label = format!(
        "{:02}:{:02}{}",
        total / 60 % 24,
        total % 60,
        if input.live { " now" } else { "" }
    );
    let text_x = hx.clamp(rect.left() + 30.0, rect.right() - 30.0);
    painter.text(
        Pos2::new(text_x, rect.top()),
        Align2::CENTER_TOP,
        label,
        style::bold(style::SMALL_FONT + 1.0),
        pal.accent,
    );

    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }
    if (response.dragged() || response.clicked())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let frac = ((pos.x - left) / (right - left)).clamp(0.0, 1.0);
        let minutes = (f64::from(frac) * 24.0 * 60.0 / super::STEP_MINUTES as f64).round() as i64
            * super::STEP_MINUTES;
        return Some(minutes.min(24 * 60 - super::STEP_MINUTES));
    }
    None
}
