//! The PEOPLE modal: browse, add, edit and delete team members, saved straight to team.toml.

use super::style;
use crate::cities::CityDb;
use crate::config::{self, Person};
use crate::people_file::{self, Entry};
use crate::theme::Palette;
use crate::timecalc;
use chrono::{DateTime, Utc};
use eframe::egui::{self, Color32, RichText, Sense, Stroke, text::LayoutJob};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    City,
    Lat,
    Lon,
    Tz,
}

impl Field {
    pub const ALL: [Field; 5] = [Field::Name, Field::City, Field::Lat, Field::Lon, Field::Tz];

    pub fn label(self) -> &'static str {
        match self {
            Field::Name => "name",
            Field::City => "city",
            Field::Lat => "lat",
            Field::Lon => "lon",
            Field::Tz => "tz",
        }
    }

    pub fn step(self, forward: bool) -> Field {
        let n = Self::ALL.len();
        let i = Self::ALL.iter().position(|f| *f == self).unwrap_or(0);
        Self::ALL[if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        }]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    List,
    Form { index: Option<usize>, field: Field },
    ConfirmDelete(usize),
}

/// Editor commands. Typing into fields goes straight to the text inputs instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Enter,
    Escape,
    Tab,
    BackTab,
    Char(char),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    None,
    Close,
    Save { index: Option<usize>, entry: Entry },
    Delete(usize),
}

/// Text of the form inputs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Form {
    pub name: String,
    pub city: String,
    pub lat: String,
    pub lon: String,
    pub tz: String,
}

impl Form {
    fn from_entry(e: &Entry) -> Form {
        let num = |v: Option<f64>| v.map(|x| x.to_string()).unwrap_or_default();
        Form {
            name: e.name.clone(),
            city: e.city.clone(),
            lat: num(e.lat),
            lon: num(e.lon),
            tz: e.tz.clone().unwrap_or_default(),
        }
    }

    pub fn field_mut(&mut self, f: Field) -> &mut String {
        match f {
            Field::Name => &mut self.name,
            Field::City => &mut self.city,
            Field::Lat => &mut self.lat,
            Field::Lon => &mut self.lon,
            Field::Tz => &mut self.tz,
        }
    }

    /// The entry this form describes, or why its numbers don't parse.
    pub fn to_entry(&self) -> Result<Entry, String> {
        let num = |label: &str, s: &str| -> Result<Option<f64>, String> {
            let s = s.trim();
            if s.is_empty() {
                Ok(None)
            } else {
                s.parse()
                    .map(Some)
                    .map_err(|_| format!("{label} must be a number"))
            }
        };
        Ok(Entry {
            name: self.name.trim().to_string(),
            city: self.city.trim().to_string(),
            lat: num("lat", &self.lat)?,
            lon: num("lon", &self.lon)?,
            tz: Some(self.tz.trim().to_string()).filter(|s| !s.is_empty()),
        })
    }
}

pub struct PeopleEditor {
    pub path: PathBuf,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub mode: Mode,
    pub form: Form,
    /// Result of the last save/delete, or a write error.
    pub status: Option<String>,
    /// Set when team.toml can't be parsed: the editor only displays it.
    pub read_only: Option<String>,
    /// Move keyboard focus to the current form field on the next frame.
    pub focus_request: bool,
    /// Scroll the selected row into view on the next frame.
    pub scroll: bool,
}

impl PeopleEditor {
    pub fn open(path: PathBuf) -> PeopleEditor {
        let mut ed = PeopleEditor {
            path,
            entries: Vec::new(),
            selected: 0,
            mode: Mode::List,
            form: Form::default(),
            status: None,
            read_only: None,
            focus_request: false,
            scroll: false,
        };
        ed.reload();
        ed
    }

    /// Re-reads the file, keeping the selection in range.
    pub fn reload(&mut self) {
        match people_file::read_entries(&self.path) {
            Ok(entries) => {
                self.entries = entries;
                self.read_only = None;
            }
            Err(e) => {
                self.entries.clear();
                self.read_only = Some(format!("{e:#}"));
            }
        }
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
    }

    pub fn handle_key(&mut self, key: Key, db: &CityDb) -> Action {
        match self.mode {
            Mode::List => self.list_key(key),
            Mode::Form { index, field } => self.form_key(key, index, field, db),
            Mode::ConfirmDelete(i) => {
                self.mode = Mode::List;
                if matches!(key, Key::Char('y' | 'Y')) {
                    Action::Delete(i)
                } else {
                    self.status = Some("delete cancelled".into());
                    Action::None
                }
            }
        }
    }

    fn list_key(&mut self, key: Key) -> Action {
        let n = self.entries.len();
        match key {
            Key::Escape => return Action::Close,
            Key::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.scroll = true;
            }
            Key::Down => {
                if n > 0 {
                    self.selected = (self.selected + 1).min(n - 1);
                    self.scroll = true;
                }
            }
            _ if self.read_only.is_some() => {}
            Key::Char('a') => self.start_form(None),
            Key::Enter | Key::Char('e') if n > 0 => self.start_form(Some(self.selected)),
            Key::Char('d') if n > 0 => {
                self.mode = Mode::ConfirmDelete(self.selected);
                self.status = None;
            }
            _ => {}
        }
        Action::None
    }

    fn start_form(&mut self, index: Option<usize>) {
        self.form = index
            .map(|i| Form::from_entry(&self.entries[i]))
            .unwrap_or_default();
        self.mode = Mode::Form {
            index,
            field: Field::Name,
        };
        self.status = None;
        self.focus_request = true;
    }

    fn form_key(&mut self, key: Key, index: Option<usize>, field: Field, db: &CityDb) -> Action {
        match key {
            Key::Escape => {
                self.mode = Mode::List;
                self.status = None;
            }
            Key::Tab | Key::BackTab => {
                self.set_field(field.step(key == Key::Tab));
                self.focus_request = true;
            }
            // Invalid entries stay in the form; the live status line already says why.
            Key::Enter => {
                if let Ok((entry, _)) = self.validate(db) {
                    return Action::Save { index, entry };
                }
            }
            _ => {}
        }
        Action::None
    }

    pub fn set_field(&mut self, field: Field) {
        if let Mode::Form { index, .. } = self.mode {
            self.mode = Mode::Form { index, field };
        }
    }

    /// The form's entry and where it lands, using the same rules as loading team.toml.
    pub fn validate(&self, db: &CityDb) -> Result<(Entry, Person), String> {
        let entry = self.form.to_entry()?;
        let person = config::resolve_entry(&entry, db)?;
        Ok((entry, person))
    }

    /// One line describing where the form's entry lands, for the live status line.
    pub fn describe(&self, db: &CityDb, now: DateTime<Utc>) -> Result<String, String> {
        let (entry, person) = self.validate(db)?;
        let offset = timecalc::format_offset(timecalc::offset_secs(now, person.tz));
        let place = match db.lookup(&entry.city) {
            Some(c) if entry.lat.is_none() && entry.lon.is_none() => {
                [&c.name, &c.admin1, &c.country_code]
                    .iter()
                    .filter(|s| !s.is_empty())
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            }
            _ => format!("{:.2}, {:.2}", person.lat, person.lon),
        };
        Ok(format!("{place} · {} · {offset}", person.tz.name()))
    }

    /// After a successful save or delete: re-read the file and select the affected row.
    pub fn after_write(&mut self, select: Option<usize>, message: String) {
        self.reload();
        if let Some(i) = select {
            self.selected = i.min(self.entries.len().saturating_sub(1));
            self.scroll = true;
        }
        self.mode = Mode::List;
        self.status = Some(message);
    }
}

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}

/// Draws the modal. Returns a command from a clicked button or the backdrop.
pub fn show(
    ctx: &egui::Context,
    ed: &mut PeopleEditor,
    pal: &Palette,
    db: &CityDb,
    now: DateTime<Utc>,
) -> Option<Key> {
    let mut pressed = None;
    let modal = egui::Modal::new(egui::Id::unique("people"))
        .backdrop_color(Color32::from_black_alpha(140))
        .frame(
            egui::Frame::NONE
                .fill(pal.panel)
                .stroke(Stroke::new(1.0, pal.accent))
                .inner_margin(egui::Margin::same(16)),
        )
        .show(ctx, |ui| {
            ui.set_width(660.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("PEOPLE ({})", ed.entries.len()))
                        .font(style::bold(15.0))
                        .color(pal.accent),
                );
                // Long paths are cut short (hover shows the full one) so the modal keeps its width.
                ui.add(
                    egui::Label::new(
                        RichText::new(ed.path.display().to_string())
                            .font(style::mono(11.0))
                            .color(pal.subtext),
                    )
                    .truncate(),
                );
            });
            ui.add_space(6.0);
            if let Some(err) = &ed.read_only {
                ui.label(RichText::new(format!("✗ {err}")).color(pal.bad));
                ui.label(
                    RichText::new("fix team.toml by hand, then press R to reload")
                        .color(pal.subtext),
                );
            }
            list(ui, ed, pal, db, now);
            ui.add_space(6.0);
            match ed.mode {
                Mode::Form { .. } => form(ui, ed, pal, db, now),
                Mode::ConfirmDelete(i) => {
                    let name = ed.entries.get(i).map_or("", |e| e.name.as_str());
                    ui.label(
                        RichText::new(format!("delete {name}?  y = yes · any other key = no"))
                            .color(pal.warn),
                    );
                }
                Mode::List => {}
            }
            if let Some(msg) = &ed.status {
                let color = if msg.starts_with('✗') {
                    pal.bad
                } else {
                    pal.accent
                };
                ui.label(RichText::new(msg).color(color));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let mut button = |ui: &mut egui::Ui, label: &str, key: Key| {
                    if ui.button(label).clicked() {
                        pressed = Some(key);
                    }
                };
                match ed.mode {
                    Mode::List => {
                        if ed.read_only.is_none() {
                            button(ui, "[a] add", Key::Char('a'));
                            if !ed.entries.is_empty() {
                                button(ui, "[e] edit", Key::Char('e'));
                                button(ui, "[d] delete", Key::Char('d'));
                            }
                        }
                        button(ui, "[Esc] close", Key::Escape);
                        ui.label(RichText::new("↑/↓ select").color(pal.subtext));
                    }
                    Mode::Form { .. } => {
                        button(ui, "[Enter] save", Key::Enter);
                        button(ui, "[Esc] cancel", Key::Escape);
                        ui.label(RichText::new("Tab next field").color(pal.subtext));
                    }
                    Mode::ConfirmDelete(_) => {
                        button(ui, "[y] delete", Key::Char('y'));
                        button(ui, "[n] keep", Key::Char('n'));
                    }
                }
            });
        });
    if modal.should_close() && pressed.is_none() {
        pressed = Some(Key::Escape);
    }
    pressed
}

fn list(ui: &mut egui::Ui, ed: &mut PeopleEditor, pal: &Palette, db: &CityDb, now: DateTime<Utc>) {
    if ed.entries.is_empty() {
        if ed.read_only.is_none() {
            ui.label(RichText::new("no one yet — press a to add someone").color(pal.subtext));
        }
        return;
    }
    let mut clicked = None;
    let scroll = std::mem::take(&mut ed.scroll);
    egui::ScrollArea::vertical()
        .max_height(240.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for (i, e) in ed.entries.iter().enumerate() {
                let selected = i == ed.selected;
                let (status, status_color) = match config::resolve_entry(e, db) {
                    Ok(p) => (
                        timecalc::format_offset(timecalc::offset_secs(now, p.tz)),
                        pal.subtext,
                    ),
                    Err(err) => (format!("✗ {err}"), pal.bad),
                };
                let (fg, bg) = if selected {
                    (pal.bg, pal.accent)
                } else {
                    (pal.text, Color32::TRANSPARENT)
                };
                let format = |color| egui::TextFormat {
                    font_id: style::mono(13.0),
                    color,
                    background: bg,
                    ..Default::default()
                };
                let mut job = LayoutJob::default();
                let marker = if selected { '›' } else { ' ' };
                job.append(
                    &format!(
                        "{marker} {:<16} {:<22} ",
                        clip(&e.name, 16),
                        clip(&e.city, 22)
                    ),
                    0.0,
                    format(fg),
                );
                job.append(
                    &status,
                    0.0,
                    format(if selected { pal.bg } else { status_color }),
                );
                let resp = ui.add(egui::Label::new(job).sense(Sense::click()).truncate());
                if resp.clicked() {
                    clicked = Some(i);
                }
                if selected && scroll {
                    resp.scroll_to_me(None);
                }
            }
        });
    if let Some(i) = clicked
        && ed.mode == Mode::List
    {
        ed.selected = i;
    }
}

fn form(ui: &mut egui::Ui, ed: &mut PeopleEditor, pal: &Palette, db: &CityDb, now: DateTime<Utc>) {
    let Mode::Form { index, field } = ed.mode else {
        return;
    };
    let title = if index.is_some() { "EDIT" } else { "ADD" };
    ui.label(
        RichText::new(title)
            .font(style::bold(13.0))
            .color(pal.accent),
    );
    let focus_now = std::mem::take(&mut ed.focus_request);
    let mut clicked_field = None;
    egui::Grid::new("person-form")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            for f in Field::ALL {
                let hint = match f {
                    Field::Name => "Ana",
                    Field::City => "Lisbon  ·  Portland, OR",
                    Field::Lat | Field::Lon => "optional",
                    Field::Tz => "optional, e.g. Europe/Lisbon",
                };
                let label_color = if f == field { pal.accent } else { pal.subtext };
                ui.label(RichText::new(f.label()).color(label_color));
                let resp = ui.add(
                    egui::TextEdit::singleline(ed.form.field_mut(f))
                        .hint_text(hint)
                        .desired_width(380.0)
                        .font(style::mono(14.0)),
                );
                if f == field && focus_now {
                    resp.request_focus();
                }
                if resp.gained_focus() && f != field {
                    clicked_field = Some(f);
                }
                ui.end_row();
            }
        });
    if let Some(f) = clicked_field {
        ed.set_field(f);
    }
    match ed.describe(db, now) {
        Ok(s) => ui.label(RichText::new(format!("✓ {s}")).color(pal.ok)),
        Err(e) => ui.label(RichText::new(format!("✗ {e}")).color(pal.bad)),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "[[person]]\nname = \"Ana\"\ncity = \"Lisbon\"\n\n[[person]]\nname = \"Kim\"\ncity = \"Tórshavn\"\nlat = 62\nlon = -6.77\ntz = \"Atlantic/Faroe\"\n";

    fn editor(tag: &str, content: &str) -> PeopleEditor {
        let p =
            std::env::temp_dir().join(format!("team-clock-ed-{}-{tag}.toml", std::process::id()));
        std::fs::write(&p, content).unwrap();
        PeopleEditor::open(p)
    }

    fn db() -> &'static CityDb {
        CityDb::embedded()
    }

    #[test]
    fn navigation_stays_in_bounds() {
        let mut ed = editor("nav", SAMPLE);
        ed.handle_key(Key::Up, db());
        assert_eq!(ed.selected, 0);
        ed.handle_key(Key::Down, db());
        ed.handle_key(Key::Down, db());
        assert_eq!(ed.selected, 1);
        assert_eq!(ed.handle_key(Key::Escape, db()), Action::Close);
    }

    #[test]
    fn add_flow_saves_a_new_entry() {
        let mut ed = editor("add", SAMPLE);
        ed.handle_key(Key::Char('a'), db());
        assert_eq!(
            ed.mode,
            Mode::Form {
                index: None,
                field: Field::Name
            }
        );
        ed.form.name = "Bo".into();
        ed.handle_key(Key::Tab, db());
        assert_eq!(
            ed.mode,
            Mode::Form {
                index: None,
                field: Field::City
            }
        );
        ed.form.city = "Berlin".into();
        let action = ed.handle_key(Key::Enter, db());
        assert_eq!(
            action,
            Action::Save {
                index: None,
                entry: Entry {
                    name: "Bo".into(),
                    city: "Berlin".into(),
                    ..Entry::default()
                }
            }
        );
    }

    #[test]
    fn invalid_entry_is_not_saved() {
        let mut ed = editor("invalid", SAMPLE);
        ed.handle_key(Key::Char('a'), db());
        ed.form.name = "Bo".into();
        ed.form.city = "Nowhereville".into();
        assert_eq!(ed.handle_key(Key::Enter, db()), Action::None);
        assert!(matches!(ed.mode, Mode::Form { .. }));
        assert!(
            ed.describe(db(), Utc::now())
                .unwrap_err()
                .contains("unknown city")
        );
        ed.form.lat = "abc".into();
        assert_eq!(ed.validate(db()).unwrap_err(), "lat must be a number");
    }

    #[test]
    fn edit_prefills_the_form() {
        let mut ed = editor("edit", SAMPLE);
        ed.handle_key(Key::Down, db());
        ed.handle_key(Key::Char('e'), db());
        assert_eq!(
            ed.mode,
            Mode::Form {
                index: Some(1),
                field: Field::Name
            }
        );
        assert_eq!(
            (ed.form.lat.as_str(), ed.form.tz.as_str()),
            ("62", "Atlantic/Faroe")
        );
        assert_eq!(ed.handle_key(Key::Escape, db()), Action::None);
        assert_eq!(ed.mode, Mode::List);
    }

    #[test]
    fn delete_needs_confirmation() {
        let mut ed = editor("delete", SAMPLE);
        ed.handle_key(Key::Char('d'), db());
        assert_eq!(ed.mode, Mode::ConfirmDelete(0));
        assert_eq!(ed.handle_key(Key::Char('n'), db()), Action::None);
        assert_eq!(ed.mode, Mode::List);
        ed.handle_key(Key::Char('d'), db());
        assert_eq!(ed.handle_key(Key::Char('y'), db()), Action::Delete(0));
    }

    #[test]
    fn tab_cycles_fields() {
        assert_eq!(Field::Tz.step(true), Field::Name);
        assert_eq!(Field::Name.step(false), Field::Tz);
    }

    #[test]
    fn describe_names_the_place() {
        let mut ed = editor("describe", SAMPLE);
        ed.handle_key(Key::Enter, db());
        let d = ed.describe(db(), Utc::now()).unwrap();
        assert!(d.starts_with("Lisbon"), "{d}");
        assert!(d.contains("Europe/Lisbon"), "{d}");
    }

    #[test]
    fn broken_file_is_read_only() {
        let mut ed = editor("broken", "[[person]\n");
        assert!(ed.read_only.is_some());
        ed.handle_key(Key::Char('a'), db());
        assert_eq!(ed.mode, Mode::List);
    }

    #[test]
    fn after_write_rereads_and_selects() {
        let mut ed = editor("after", SAMPLE);
        crate::people_file::write_entry(
            &ed.path,
            None,
            &Entry {
                name: "Bo".into(),
                city: "Berlin".into(),
                ..Entry::default()
            },
        )
        .unwrap();
        ed.after_write(Some(2), "saved Bo".into());
        assert_eq!(ed.entries.len(), 3);
        assert_eq!(ed.selected, 2);
        assert_eq!(ed.status.as_deref(), Some("saved Bo"));
    }
}
