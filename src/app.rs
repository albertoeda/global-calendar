//! The egui application: map canvas, slider bar, status bar, help overlay, keybindings.

mod map_view;
mod slider;
mod style;

use crate::config::{self, TeamConfig};
use crate::theme::{self, Palette, THEMES};
use crate::timecalc;
use chrono::{DateTime, Duration, NaiveDate, Utc};
use chrono_tz::Tz;
use eframe::egui::{self, Align, Key, Layout, Modifiers, RichText};
use std::path::PathBuf;
use std::time::Instant;

/// Slider and arrow-key resolution.
pub const STEP_MINUTES: i64 = 15;
const DAY_MINUTES: i64 = 24 * 60;
const TOAST_SECS: u64 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Follow the clock.
    Live,
    /// A chosen viewer-local date and time (minutes past midnight).
    Scrub { date: NaiveDate, minutes: i64 },
}

/// Moves `minutes` by `delta` and snaps to the 15-minute grid in the direction of travel,
/// rolling over into neighbouring days.
pub fn step(date: NaiveDate, minutes: i64, delta: i64) -> (NaiveDate, i64) {
    let target = minutes + delta;
    let snapped = if delta >= 0 {
        target.div_euclid(STEP_MINUTES) * STEP_MINUTES
    } else {
        -(-target).div_euclid(STEP_MINUTES) * STEP_MINUTES
    };
    normalize(date, snapped)
}

fn normalize(date: NaiveDate, minutes: i64) -> (NaiveDate, i64) {
    (
        date + Duration::days(minutes.div_euclid(DAY_MINUTES)),
        minutes.rem_euclid(DAY_MINUTES),
    )
}

/// "+2h15m", "−45m", "+1d 3h".
pub fn format_delta(minutes: i64) -> String {
    let sign = if minutes < 0 { '−' } else { '+' };
    let m = minutes.abs();
    let (d, h, m) = (m / DAY_MINUTES, m % DAY_MINUTES / 60, m % 60);
    match (d, h, m) {
        (0, 0, m) => format!("{sign}{m}m"),
        (0, h, 0) => format!("{sign}{h}h"),
        (0, h, m) => format!("{sign}{h}h{m:02}m"),
        (d, 0, _) => format!("{sign}{d}d"),
        (d, h, _) => format!("{sign}{d}d {h}h"),
    }
}

/// The user's IANA zone, falling back to UTC when it can't be detected.
fn detect_viewer_tz() -> Tz {
    iana_time_zone::get_timezone()
        .ok()
        .and_then(|name| name.parse().ok())
        .unwrap_or(Tz::UTC)
}

/// "Europe/Madrid" → "Madrid", "America/Argentina/Buenos_Aires" → "Buenos Aires".
fn zone_city(tz: Tz) -> String {
    tz.name().rsplit('/').next().unwrap_or("").replace('_', " ")
}

pub struct TeamClockApp {
    cli_path: Option<PathBuf>,
    config: TeamConfig,
    /// Bumped on every reload so caches keyed on the team invalidate.
    generation: u64,
    viewer_tz: Tz,
    theme: usize,
    mode: Mode,
    /// First member index of the focused group (stable when groups merge).
    focus: Option<usize>,
    show_help: bool,
    toast: Option<(String, Instant)>,
    map: map_view::MapView,
    availability: slider::AvailabilityCache,
}

impl TeamClockApp {
    pub fn new(cc: &eframe::CreationContext<'_>, cli_path: Option<PathBuf>) -> Self {
        style::install_fonts(&cc.egui_ctx);
        let mut app = TeamClockApp {
            cli_path,
            config: TeamConfig::default(),
            generation: 0,
            viewer_tz: detect_viewer_tz(),
            theme: theme::DEFAULT_THEME,
            mode: Mode::Live,
            focus: None,
            show_help: false,
            toast: None,
            map: map_view::MapView::default(),
            availability: slider::AvailabilityCache::default(),
        };
        app.load_config();
        style::apply(&cc.egui_ctx, app.palette());
        app
    }

    fn palette(&self) -> &Palette {
        &THEMES[self.theme].palette
    }

    fn load_config(&mut self) {
        self.config = config::load(self.cli_path.as_deref());
        self.generation += 1;
        self.focus = None;
        if let Some(name) = self.config.theme.clone() {
            match theme::find(&name) {
                Some(i) => self.theme = i,
                None => self.config.errors.push(format!(
                    "unknown theme \"{name}\" — available: {}",
                    THEMES.iter().map(|t| t.id).collect::<Vec<_>>().join(", ")
                )),
            }
        }
    }

    fn instant(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        match self.mode {
            Mode::Live => now,
            Mode::Scrub { date, minutes } => {
                timecalc::instant_from_local(self.viewer_tz, date, minutes)
            }
        }
    }

    /// Current slider position as (viewer-local date, minutes past midnight).
    fn slot(&self, now: DateTime<Utc>) -> (NaiveDate, i64) {
        match self.mode {
            Mode::Live => (
                now.with_timezone(&self.viewer_tz).date_naive(),
                timecalc::local_minutes(now, self.viewer_tz),
            ),
            Mode::Scrub { date, minutes } => (date, minutes),
        }
    }

    fn shift_minutes(&mut self, delta: i64) {
        let (date, minutes) = self.slot(Utc::now());
        let (date, minutes) = step(date, minutes, delta);
        self.mode = Mode::Scrub { date, minutes };
    }

    fn shift_days(&mut self, days: i64) {
        let (date, minutes) = self.slot(Utc::now());
        let minutes = minutes / STEP_MINUTES * STEP_MINUTES;
        self.mode = Mode::Scrub {
            date: date + Duration::days(days),
            minutes,
        };
    }

    fn set_minutes(&mut self, minutes: i64) {
        let (date, _) = self.slot(Utc::now());
        self.mode = Mode::Scrub { date, minutes };
    }

    fn cycle_theme(&mut self, ctx: &egui::Context, forward: bool) {
        self.theme = theme::next(self.theme, forward);
        style::apply(ctx, self.palette());
        let t = &THEMES[self.theme];
        let saved = match &self.config.path {
            Some(path) => match config::save_theme(path, t.id) {
                Ok(()) => " (saved)".to_string(),
                Err(e) => format!(" (not saved: {e})"),
            },
            None => String::new(),
        };
        self.toast(format!("theme: {}{saved}", t.name));
    }

    fn cycle_focus(&mut self, forward: bool) {
        let order = &self.map.order;
        if order.is_empty() {
            return;
        }
        let current = self
            .focus
            .and_then(|f| order.iter().position(|&(_, first)| first == f));
        let n = order.len();
        let next = match (current, forward) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        self.focus = Some(order[next].1);
    }

    fn toast(&mut self, msg: String) {
        self.toast = Some((msg, Instant::now()));
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        // Consume keys before any widget sees them, so Tab etc. don't drive egui's own focus.
        let (keys, texts) = ctx.input_mut(|i| {
            let mut keys = Vec::new();
            for (mods, key) in [
                (Modifiers::NONE, Key::ArrowRight),
                (Modifiers::NONE, Key::ArrowLeft),
                (Modifiers::SHIFT, Key::ArrowRight),
                (Modifiers::SHIFT, Key::ArrowLeft),
                (Modifiers::NONE, Key::Home),
                (Modifiers::NONE, Key::End),
                (Modifiers::NONE, Key::Tab),
                (Modifiers::SHIFT, Key::Tab),
                (Modifiers::NONE, Key::Escape),
                (Modifiers::COMMAND, Key::Q),
            ] {
                while i.consume_key(mods, key) {
                    keys.push((mods, key));
                }
            }
            let texts: Vec<String> = i
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) => Some(t.clone()),
                    _ => None,
                })
                .collect();
            (keys, texts)
        });

        for (mods, key) in keys {
            let shift = mods.shift;
            match key {
                Key::ArrowRight => self.shift_minutes(if shift { 60 } else { STEP_MINUTES }),
                Key::ArrowLeft => self.shift_minutes(if shift { -60 } else { -STEP_MINUTES }),
                Key::Home => self.set_minutes(0),
                Key::End => self.set_minutes(DAY_MINUTES - STEP_MINUTES),
                Key::Tab => self.cycle_focus(!shift),
                Key::Escape if self.show_help => self.show_help = false,
                Key::Escape if self.focus.is_some() => self.focus = None,
                Key::Escape | Key::Q => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                _ => {}
            }
        }
        for text in texts {
            for c in text.chars() {
                match c {
                    'n' | 'N' => self.mode = Mode::Live,
                    't' => self.cycle_theme(ctx, true),
                    'T' => self.cycle_theme(ctx, false),
                    '[' => self.shift_days(-1),
                    ']' => self.shift_days(1),
                    '?' => self.show_help = !self.show_help,
                    'r' | 'R' => {
                        self.load_config();
                        style::apply(ctx, self.palette());
                        self.toast(format!("reloaded: {} people", self.config.people.len()));
                    }
                    'q' | 'Q' => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    _ => {}
                }
            }
        }
    }

    fn status_bar(&mut self, ui: &mut egui::Ui, now: DateTime<Utc>, instant: DateTime<Utc>) {
        let pal = *self.palette();
        ui.horizontal_centered(|ui| {
            ui.label(
                RichText::new(" TEAM CLOCK ")
                    .font(style::bold(13.0))
                    .color(pal.bg)
                    .background_color(pal.accent),
            );
            let local = instant.with_timezone(&self.viewer_tz);
            ui.label(RichText::new("YOU").color(pal.subtext));
            ui.label(
                RichText::new(zone_city(self.viewer_tz))
                    .color(pal.accent)
                    .strong(),
            );
            ui.label(RichText::new(local.format("%H:%M").to_string()).font(style::bold(14.0)));
            ui.label(RichText::new(local.format("%Z · %a %d %b").to_string()).color(pal.subtext));
            ui.label(RichText::new("│").color(pal.border));
            match self.mode {
                Mode::Live => {
                    ui.label(RichText::new("● LIVE").color(pal.ok));
                }
                Mode::Scrub { .. } => {
                    let delta = (instant - now).num_minutes();
                    ui.label(RichText::new("◆ PREVIEW").color(pal.warn));
                    ui.label(
                        RichText::new(format!("{} from now", format_delta(delta)))
                            .color(pal.subtext),
                    );
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(RichText::new(THEMES[self.theme].name).color(pal.subtext));
                ui.label(RichText::new("│").color(pal.border));
                let people = self.config.people.len();
                ui.label(
                    RichText::new(format!(
                        "{people} {}",
                        if people == 1 { "person" } else { "people" }
                    ))
                    .color(pal.subtext),
                );
                if let Some((msg, _)) = &self.toast {
                    ui.label(RichText::new("│").color(pal.border));
                    ui.label(RichText::new(msg).color(pal.accent));
                }
            });
        });
    }

    fn timeline(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, now: DateTime<Utc>) {
        let pal = *self.palette();
        let (date, minutes) = self.slot(now);
        let now_local = now.with_timezone(&self.viewer_tz);
        let now_minutes = (now_local.date_naive() == date)
            .then(|| timecalc::local_minutes(now, self.viewer_tz) as f64);
        let picked = slider::show(
            ui,
            &mut self.availability,
            slider::SliderInput {
                pal: &pal,
                people: &self.config.people,
                viewer: self.viewer_tz,
                date,
                generation: self.generation,
                minutes: minutes as f64,
                now_minutes,
                live: self.mode == Mode::Live,
            },
        );
        if let Some(m) = picked {
            self.mode = Mode::Scrub { date, minutes: m };
        }

        ui.horizontal(|ui| {
            let hint = |ui: &mut egui::Ui, key: &str, what: &str| {
                ui.label(RichText::new(key).color(pal.accent));
                ui.label(RichText::new(what).color(pal.subtext));
            };
            ui.label(RichText::new(date.format("%a %d %b %Y").to_string()).color(pal.text));
            ui.label(RichText::new("│").color(pal.border));
            hint(ui, "←/→", "15m");
            hint(ui, "⇧←/→", "1h");
            hint(ui, "[ ]", "day");
            hint(ui, "Tab", "focus");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("[?] help").clicked() {
                    self.show_help = !self.show_help;
                }
                if ui.button("[T] theme").clicked() {
                    self.cycle_theme(ctx, true);
                }
                let live = self.mode == Mode::Live;
                let now_btn = ui.add_enabled(!live, egui::Button::new("[N] now"));
                if now_btn.clicked() {
                    self.mode = Mode::Live;
                }
            });
        });
    }

    fn help(&mut self, ctx: &egui::Context) {
        let pal = *self.palette();
        let rows = [
            ("← / →", "move time ±15 minutes"),
            ("Shift + ← / →", "move time ±1 hour"),
            ("[ / ]", "previous / next day"),
            ("Home / End", "start / end of day"),
            ("N", "back to now (live)"),
            ("T / Shift+T", "next / previous theme"),
            ("Tab / Shift+Tab", "focus next / previous city"),
            ("R", "reload team.toml"),
            ("?", "toggle this help"),
            ("Esc", "close help · clear focus · quit"),
            ("Q / Ctrl+Q / ⌘Q", "quit"),
        ];
        let modal = egui::Modal::new(egui::Id::unique("help"))
            .backdrop_color(theme::mix(pal.bg, egui::Color32::BLACK, 0.3).gamma_multiply(0.7))
            .frame(
                egui::Frame::NONE
                    .fill(pal.panel)
                    .stroke(egui::Stroke::new(1.0, pal.accent))
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("KEYBOARD")
                        .font(style::bold(15.0))
                        .color(pal.accent),
                );
                ui.add_space(6.0);
                egui::Grid::new("keys")
                    .num_columns(2)
                    .spacing([24.0, 4.0])
                    .show(ui, |ui| {
                        for (k, what) in rows {
                            ui.label(RichText::new(k).color(pal.accent));
                            ui.label(RichText::new(what).color(pal.text));
                            ui.end_row();
                        }
                    });
                ui.add_space(8.0);
                ui.label(RichText::new("time colours").color(pal.subtext));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("■ 09–17 work").color(pal.ok));
                    ui.label(RichText::new("■ 07–09 / 17–22").color(pal.warn));
                    ui.label(RichText::new("■ night").color(pal.bad));
                });
                if let Some(path) = &self.config.path {
                    ui.add_space(8.0);
                    ui.label(RichText::new(format!("team: {}", path.display())).color(pal.subtext));
                }
            });
        if modal.should_close() {
            self.show_help = false;
        }
    }
}

impl eframe::App for TeamClockApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_input(&ctx);
        if self
            .toast
            .as_ref()
            .is_some_and(|(_, at)| at.elapsed().as_secs() >= TOAST_SECS)
        {
            self.toast = None;
        }

        let now = Utc::now();
        let instant = self.instant(now);
        let pal = *self.palette();

        egui::Panel::top("status")
            .resizable(false)
            .frame(style::bar_frame(&pal))
            .show(ui, |ui| self.status_bar(ui, now, instant));
        egui::Panel::bottom("timeline")
            .resizable(false)
            .frame(style::bar_frame(&pal))
            .show(ui, |ui| self.timeline(ui, &ctx, now));

        let instant = self.instant(now); // the timeline may have moved it
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(pal.bg))
            .show(ui, |ui| {
                let clicked = self.map.show(
                    ui,
                    map_view::MapInput {
                        pal: &pal,
                        people: &self.config.people,
                        instant,
                        viewer: self.viewer_tz,
                        focus: self.focus,
                        errors: &self.config.errors,
                    },
                );
                if let Some(first) = clicked {
                    self.focus = if self.focus == Some(first) {
                        None
                    } else {
                        Some(first)
                    };
                }
            });

        if self.show_help {
            self.help(&ctx);
        }

        // Wake up for the next minute (live clock) or when the toast expires.
        let secs_into_minute = u64::try_from(now.timestamp().rem_euclid(60)).unwrap_or(0);
        let mut wake = std::time::Duration::from_secs(60 - secs_into_minute);
        if let Some((_, at)) = &self.toast {
            wake =
                wake.min(std::time::Duration::from_secs(TOAST_SECS).saturating_sub(at.elapsed()));
        }
        ctx.request_repaint_after(wake);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        self.palette().bg.to_normalized_gamma_f32()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn stepping_snaps_in_direction_of_travel() {
        assert_eq!(step(d(15), 14 * 60 + 37, 15), (d(15), 14 * 60 + 45));
        assert_eq!(step(d(15), 14 * 60 + 37, -15), (d(15), 14 * 60 + 30));
        assert_eq!(step(d(15), 14 * 60 + 30, 60), (d(15), 15 * 60 + 30));
        assert_eq!(step(d(15), 14 * 60 + 30, -15), (d(15), 14 * 60 + 15));
    }

    #[test]
    fn stepping_rolls_over_days() {
        assert_eq!(step(d(15), 23 * 60 + 45, 15), (d(16), 0));
        assert_eq!(step(d(15), 0, -15), (d(14), 23 * 60 + 45));
        assert_eq!(step(d(15), 30, -60), (d(14), 23 * 60 + 30));
    }

    #[test]
    fn deltas() {
        assert_eq!(format_delta(0), "+0m");
        assert_eq!(format_delta(-45), "−45m");
        assert_eq!(format_delta(120), "+2h");
        assert_eq!(format_delta(135), "+2h15m");
        assert_eq!(format_delta(-(24 * 60 + 180)), "−1d 3h");
        assert_eq!(format_delta(48 * 60), "+2d");
    }

    #[test]
    fn zone_city_names() {
        assert_eq!(zone_city(chrono_tz::Europe::Madrid), "Madrid");
        assert_eq!(
            zone_city(chrono_tz::America::Argentina::Buenos_Aires),
            "Buenos Aires"
        );
        assert_eq!(zone_city(Tz::UTC), "UTC");
    }
}
