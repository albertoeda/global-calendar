# People Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a keyboard-driven PEOPLE modal to Team Clock that adds, edits and deletes `[[person]]` entries in `team.toml` in place.

**Architecture:**
- A pure file layer (`people_file.rs`, toml_edit) reads and rewrites `[[person]]` tables while keeping comments.
- `config::resolve_entry` becomes the single validation rule, shared by file loading and the editor.
- A pure state machine (`app/people_editor.rs`) handles keys; a thin egui function renders it as a `Modal`.
- `app.rs` routes keys to the editor while it is open, performs the writes, and reloads the team.

**Tech Stack:** Rust 2024, eframe/egui 0.37, toml_edit 0.25, chrono-tz.

## Global Constraints
- Must work on Windows, macOS and Linux (no OS-specific code).
- Keyboard-only usable; global shortcuts are suspended while the modal is open.
- Writes must preserve comments, other entries and the `theme` key.
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` must pass. Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

---

### Task 1: `people_file` + shared `config::resolve_entry`

**Files:**
- Create: `src/people_file.rs`
- Modify: `src/lib.rs` (add `pub mod people_file;`)
- Modify: `src/config.rs`:
  - `resolve(p: &RawPerson, …)` becomes `pub fn resolve_entry(p: &Entry, …)`
  - add `impl From<&RawPerson> for Entry`
  - add `pub fn default_path()`

**Interfaces:**
- Produces:
  - `people_file::Entry { name: String, city: String, lat: Option<f64>, lon: Option<f64>, tz: Option<String> }` (Clone, Debug, Default, PartialEq)
  - `people_file::read_entries(&Path) -> anyhow::Result<Vec<Entry>>`
  - `people_file::write_entry(&Path, Option<usize>, &Entry) -> anyhow::Result<()>`
  - `people_file::remove_entry(&Path, usize) -> anyhow::Result<()>`
  - `config::resolve_entry(&Entry, &CityDb) -> Result<Person, String>`
  - `config::default_path() -> PathBuf`

- [ ] **Step 1: Write failing tests** in `src/people_file.rs` (module with stub fns returning `todo!()`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const SAMPLE: &str = "# my team\ntheme = \"nord\"\n\n# lead\n[[person]]\nname = \"Ana\"\ncity = \"Lisbon\"\n\n[[person]]\nname = \"Kim\"\ncity = \"Tórshavn\"\nlat = 62\nlon = -6.77\ntz = \"Atlantic/Faroe\"\n";

    fn file(tag: &str, content: Option<&str>) -> PathBuf {
        let p = std::env::temp_dir().join(format!("team-clock-pf-{}-{tag}.toml", std::process::id()));
        match content {
            Some(c) => std::fs::write(&p, c).unwrap(),
            None => { let _ = std::fs::remove_file(&p); }
        }
        p
    }

    fn entry(name: &str, city: &str) -> Entry {
        Entry { name: name.into(), city: city.into(), ..Entry::default() }
    }

    #[test]
    fn reads_entries_in_order() {
        let p = file("read", Some(SAMPLE));
        let e = read_entries(&p).unwrap();
        assert_eq!(e[0], entry("Ana", "Lisbon"));
        assert_eq!(e[1].lat, Some(62.0));
        assert_eq!(e[1].lon, Some(-6.77));
        assert_eq!(e[1].tz.as_deref(), Some("Atlantic/Faroe"));
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn missing_file_has_no_entries() {
        assert!(read_entries(&file("missing", None)).unwrap().is_empty());
    }

    #[test]
    fn replace_keeps_comments_theme_and_others() {
        let p = file("replace", Some(SAMPLE));
        write_entry(&p, Some(0), &entry("Ana", "Porto")).unwrap();
        let text = std::fs::read_to_string(&p).unwrap();
        for needle in ["# my team", "# lead", "theme = \"nord\"", "city = \"Porto\"", "Atlantic/Faroe"] {
            assert!(text.contains(needle), "missing {needle:?} in:\n{text}");
        }
        assert_eq!(read_entries(&p).unwrap()[0].city, "Porto");
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn append_then_remove() {
        let p = file("append", Some(SAMPLE));
        write_entry(&p, None, &entry("Bo", "Berlin")).unwrap();
        assert_eq!(read_entries(&p).unwrap()[2], entry("Bo", "Berlin"));
        remove_entry(&p, 0).unwrap();
        let names: Vec<String> = read_entries(&p).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Kim", "Bo"]);
        assert!(std::fs::read_to_string(&p).unwrap().contains("# my team"));
        assert!(remove_entry(&p, 5).is_err());
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn cleared_optional_fields_are_removed() {
        let p = file("optional", Some(SAMPLE));
        write_entry(&p, Some(1), &entry("Kim", "Tórshavn")).unwrap();
        let kim = &read_entries(&p).unwrap()[1];
        assert_eq!((kim.lat, kim.lon, kim.tz.clone()), (None, None, None));
        assert!(!std::fs::read_to_string(&p).unwrap().contains("Faroe"));
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn creates_missing_file_and_directories() {
        let dir = std::env::temp_dir().join(format!("team-clock-pf-{}-newdir", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join("sub").join("team.toml");
        write_entry(&p, None, &entry("Ana", "Lisbon")).unwrap();
        assert_eq!(read_entries(&p).unwrap(), vec![entry("Ana", "Lisbon")]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn invalid_toml_is_an_error() {
        let p = file("invalid", Some("[[person]\nname = 1\n"));
        assert!(read_entries(&p).is_err());
        assert!(write_entry(&p, None, &entry("A", "B")).is_err());
        std::fs::remove_file(p).unwrap();
    }
}
```

Add to `src/config.rs` tests:

```rust
    #[test]
    fn resolve_entry_shares_rules() {
        use crate::people_file::Entry;
        let ok = Entry { name: "Ana".into(), city: "Lisbon".into(), ..Entry::default() };
        assert_eq!(resolve_entry(&ok, CityDb::embedded()).unwrap().tz, chrono_tz::Europe::Lisbon);
        let bad = Entry { name: "X".into(), city: "Nowhereville".into(), ..Entry::default() };
        assert!(resolve_entry(&bad, CityDb::embedded()).unwrap_err().contains("unknown city"));
    }
```

- [ ] **Step 2: Run** `cargo test --lib people_file config`. Expected: FAIL. `todo!()` panics, and `resolve_entry` doesn't exist yet.

- [ ] **Step 3: Implement `src/people_file.rs`**

```rust
//! Reads and edits the `[[person]]` entries of `team.toml` in place, keeping comments,
//! formatting and every other key intact.

use anyhow::{Context, Result, bail};
use std::path::Path;
use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

/// One `[[person]]` table as written in the file, before its city is resolved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub name: String,
    pub city: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    pub tz: Option<String>,
}

/// Entries in file order. A missing file has none.
pub fn read_entries(path: &Path) -> Result<Vec<Entry>> {
    let doc = load(path)?;
    let Some(item) = doc.get("person") else {
        return Ok(Vec::new());
    };
    let tables = item.as_array_of_tables().context(NOT_TABLES)?;
    Ok(tables.iter().map(entry_from).collect())
}

/// Replaces the entry at `index`, or appends a new one when `index` is `None`.
/// Creates the file (and its directories) if needed.
pub fn write_entry(path: &Path, index: Option<usize>, entry: &Entry) -> Result<()> {
    let mut doc = load(path)?;
    let tables = people_mut(&mut doc)?;
    match index {
        Some(i) => {
            let len = tables.len();
            let table = tables
                .get_mut(i)
                .with_context(|| format!("no person #{} (the file has {len})", i + 1))?;
            fill(table, entry);
        }
        None => {
            let mut table = Table::new();
            fill(&mut table, entry);
            tables.push(table);
        }
    }
    save(path, &doc)
}

pub fn remove_entry(path: &Path, index: usize) -> Result<()> {
    let mut doc = load(path)?;
    let tables = people_mut(&mut doc)?;
    if index >= tables.len() {
        bail!("no person #{} (the file has {})", index + 1, tables.len());
    }
    tables.remove(index);
    save(path, &doc)
}

const NOT_TABLES: &str = "`person` in team.toml must be a list of [[person]] tables";

fn load(path: &Path) -> Result<DocumentMut> {
    if !path.exists() {
        return Ok(DocumentMut::new());
    }
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    text.parse()
        .with_context(|| format!("{} is not valid TOML", path.display()))
}

fn save(path: &Path, doc: &DocumentMut) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("could not create {}", dir.display()))?;
    }
    std::fs::write(path, doc.to_string())
        .with_context(|| format!("could not write {}", path.display()))
}

fn people_mut(doc: &mut DocumentMut) -> Result<&mut ArrayOfTables> {
    doc.entry("person")
        .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
        .as_array_of_tables_mut()
        .context(NOT_TABLES)
}

fn entry_from(t: &Table) -> Entry {
    let text = |k: &str| t.get(k).and_then(Item::as_str).map(str::to_string);
    let num = |k: &str| {
        t.get(k)
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
    };
    Entry {
        name: text("name").unwrap_or_default(),
        city: text("city").unwrap_or_default(),
        lat: num("lat"),
        lon: num("lon"),
        tz: text("tz"),
    }
}

/// Writes `e` into `t`, dropping optional keys that are now empty.
fn fill(t: &mut Table, e: &Entry) {
    t["name"] = value(e.name.trim());
    t["city"] = value(e.city.trim());
    for (key, v) in [("lat", e.lat), ("lon", e.lon)] {
        match v {
            Some(x) => t[key] = value(x),
            None => {
                t.remove(key);
            }
        }
    }
    match e.tz.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(tz) => t["tz"] = value(tz),
        None => {
            t.remove("tz");
        }
    }
}
```

Add `pub mod people_file;` to `src/lib.rs` (alphabetical, after `layout`).

In `src/config.rs`:
- Add `use crate::people_file::Entry;`
- Add this conversion:

```rust
impl From<&RawPerson> for Entry {
    fn from(p: &RawPerson) -> Entry {
        Entry {
            name: p.name.clone(),
            city: p.city.clone(),
            lat: p.lat.map(Num::get),
            lon: p.lon.map(Num::get),
            tz: p.tz.clone(),
        }
    }
}

/// Where a new team.toml is created when none exists yet: the platform config dir.
pub fn default_path() -> PathBuf {
    search_paths(None)
        .pop()
        .unwrap_or_else(|| PathBuf::from("team.toml"))
}
```

- In `parse`, replace `match resolve(p, db)` with `match resolve_entry(&Entry::from(p), db)`.
- Rename `fn resolve(p: &RawPerson, db: &CityDb)` to `pub fn resolve_entry(p: &Entry, db: &CityDb)`. Give it the doc comment `/// Validates one entry and places it, using the city database unless lat, lon and tz are all given.`
- Inside it, replace `let lat = p.lat.map(Num::get); let lon = p.lon.map(Num::get);` with `let (lat, lon) = (p.lat, p.lon);`.

- [ ] **Step 4: Run** `cargo test --lib`. Expected: all tests pass, including the existing config tests.

- [ ] **Step 5: Commit** with `git add src/people_file.rs src/lib.rs src/config.rs && git commit -m "Add in-place team.toml person editing and shared entry validation"`.

---

### Task 2: Editor state machine (pure)

**Files:**
- Create: `src/app/people_editor.rs`, with state and logic only. Rendering is added in Task 3.
- Modify: `src/app.rs`: add `mod people_editor;`.

**Interfaces:**
- Consumes: Task 1's `Entry`, `read_entries`, `resolve_entry`.
- Produces:
  - `Field { Name, City, Lat, Lon, Tz }`
  - `Mode { List, Form { index: Option<usize>, field: Field }, ConfirmDelete(usize) }`
  - `Key { Up, Down, Enter, Escape, Tab, BackTab, Char(char) }`
  - `Action { None, Close, Save { index: Option<usize>, entry: Entry }, Delete(usize) }`
  - `PeopleEditor { path, entries, selected, mode, form: Form, status, read_only, focus_request, scroll }`, with these methods:
    - `open(PathBuf) -> Self`
    - `reload()`
    - `handle_key(Key, &CityDb) -> Action`
    - `validate(&CityDb) -> Result<(Entry, Person), String>`
    - `describe(&CityDb, DateTime<Utc>) -> Result<String, String>`
    - `after_write(Option<usize>, String)`
    - `set_field(Field)`

- [ ] **Step 1: Write failing tests** at the bottom of `src/app/people_editor.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "[[person]]\nname = \"Ana\"\ncity = \"Lisbon\"\n\n[[person]]\nname = \"Kim\"\ncity = \"Tórshavn\"\nlat = 62\nlon = -6.77\ntz = \"Atlantic/Faroe\"\n";

    fn editor(tag: &str, content: &str) -> PeopleEditor {
        let p = std::env::temp_dir().join(format!("team-clock-ed-{}-{tag}.toml", std::process::id()));
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
        assert_eq!(ed.mode, Mode::Form { index: None, field: Field::Name });
        ed.form.name = "Bo".into();
        ed.handle_key(Key::Tab, db());
        assert_eq!(ed.mode, Mode::Form { index: None, field: Field::City });
        ed.form.city = "Berlin".into();
        let action = ed.handle_key(Key::Enter, db());
        assert_eq!(
            action,
            Action::Save { index: None, entry: Entry { name: "Bo".into(), city: "Berlin".into(), ..Entry::default() } }
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
        assert!(ed.describe(db(), Utc::now()).unwrap_err().contains("unknown city"));
        ed.form.lat = "abc".into();
        assert_eq!(ed.validate(db()).unwrap_err(), "lat must be a number");
    }

    #[test]
    fn edit_prefills_the_form() {
        let mut ed = editor("edit", SAMPLE);
        ed.handle_key(Key::Down, db());
        ed.handle_key(Key::Char('e'), db());
        assert_eq!(ed.mode, Mode::Form { index: Some(1), field: Field::Name });
        assert_eq!((ed.form.lat.as_str(), ed.form.tz.as_str()), ("62", "Atlantic/Faroe"));
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
        crate::people_file::write_entry(&ed.path, None, &Entry { name: "Bo".into(), city: "Berlin".into(), ..Entry::default() }).unwrap();
        ed.after_write(Some(2), "saved Bo".into());
        assert_eq!(ed.entries.len(), 3);
        assert_eq!(ed.selected, 2);
        assert_eq!(ed.status.as_deref(), Some("saved Bo"));
    }
}
```

- [ ] **Step 2: Run** `cargo test --lib people_editor`. Expected: FAIL to compile (the types don't exist yet).

- [ ] **Step 3: Implement** in `src/app/people_editor.rs`:

```rust
//! The PEOPLE modal: browse, add, edit and delete team members, saved straight to team.toml.

use crate::cities::CityDb;
use crate::config::{self, Person};
use crate::people_file::{self, Entry};
use crate::timecalc;
use chrono::{DateTime, Utc};
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
        Self::ALL[if forward { (i + 1) % n } else { (i + n - 1) % n }]
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
                s.parse().map(Some).map_err(|_| format!("{label} must be a number"))
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
            Key::Char('d') if n > 0 => self.mode = Mode::ConfirmDelete(self.selected),
            _ => {}
        }
        Action::None
    }

    fn start_form(&mut self, index: Option<usize>) {
        self.form = index
            .map(|i| Form::from_entry(&self.entries[i]))
            .unwrap_or_default();
        self.mode = Mode::Form { index, field: Field::Name };
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
            Some(c) if entry.lat.is_none() && entry.lon.is_none() => [&c.name, &c.admin1, &c.country_code]
                .iter()
                .filter(|s| !s.is_empty())
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", "),
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
```

Add `mod people_editor;` to the module list at the top of `src/app.rs`. The UI lands in Task 3, so to keep clippy quiet until then, add `#![allow(dead_code)]` at the top of `people_editor.rs` temporarily.

- [ ] **Step 4: Run** `cargo test --lib people_editor`. Expected: 9 tests pass.
- [ ] **Step 5: Commit** with `git add src/app/people_editor.rs src/app.rs && git commit -m "Add people editor state machine"`.

---

### Task 3: Render the modal and wire it into the app

**Files:**
- Modify: `src/app/people_editor.rs`: add the `show()` render function and remove `#![allow(dead_code)]`.
- Modify: `src/app.rs`:
  - add the `people` field and `open_people` / `people_input` / `people_key` / `reload_team`
  - add the `[P] people` button
  - add a help row
  - render the modal in `ui()`
- Modify: `README.md`: add a `P` row to the keyboard table.

**Interfaces:**
- Consumes: everything from Task 2, plus `style::{mono, bold}` and `Palette`.
- Produces: `people_editor::show(ctx: &egui::Context, ed: &mut PeopleEditor, pal: &Palette, db: &CityDb, now: DateTime<Utc>) -> Option<Key>`. It returns a key for clicked buttons or a backdrop click (`Escape`).

- [ ] **Step 1: Add rendering** to `src/app/people_editor.rs`:

```rust
use super::style;
use crate::theme::Palette;
use eframe::egui::{self, Color32, RichText, Sense, Stroke, text::LayoutJob};

fn clip(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}

/// Draws the modal. Returns a command from a clicked button or the backdrop.
pub fn show(ctx: &egui::Context, ed: &mut PeopleEditor, pal: &Palette, db: &CityDb, now: DateTime<Utc>) -> Option<Key> {
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
                ui.label(RichText::new(format!("PEOPLE ({})", ed.entries.len())).font(style::bold(15.0)).color(pal.accent));
                ui.label(RichText::new(ed.path.display().to_string()).font(style::mono(11.0)).color(pal.subtext));
            });
            ui.add_space(6.0);
            if let Some(err) = &ed.read_only {
                ui.label(RichText::new(format!("✗ {err}")).color(pal.bad));
                ui.label(RichText::new("fix team.toml by hand, then press R to reload").color(pal.subtext));
            }
            list(ui, ed, pal, db, now);
            ui.add_space(6.0);
            match ed.mode {
                Mode::Form { .. } => form(ui, ed, pal, db, now),
                Mode::ConfirmDelete(i) => {
                    let name = ed.entries.get(i).map_or("", |e| e.name.as_str());
                    ui.label(RichText::new(format!("delete {name}?  y = yes · any other key = no")).color(pal.warn));
                }
                Mode::List => {}
            }
            if let Some(msg) = &ed.status {
                let color = if msg.starts_with('✗') { pal.bad } else { pal.accent };
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
                    Ok(p) => (timecalc::format_offset(timecalc::offset_secs(now, p.tz)), pal.subtext),
                    Err(err) => (format!("✗ {err}"), pal.bad),
                };
                let (fg, bg) = if selected { (pal.bg, pal.accent) } else { (pal.text, Color32::TRANSPARENT) };
                let format = |color| egui::TextFormat { font_id: style::mono(13.0), color, background: bg, ..Default::default() };
                let mut job = LayoutJob::default();
                let marker = if selected { '›' } else { ' ' };
                job.append(&format!("{marker} {:<16} {:<22} ", clip(&e.name, 16), clip(&e.city, 22)), 0.0, format(fg));
                job.append(&status, 0.0, format(if selected { pal.bg } else { status_color }));
                let resp = ui.add(egui::Label::new(job).sense(Sense::click()).truncate());
                if resp.clicked() {
                    clicked = Some(i);
                }
                if selected && scroll {
                    resp.scroll_to_me(None);
                }
            }
        });
    if let Some(i) = clicked {
        if ed.mode == Mode::List {
            ed.selected = i;
        }
    }
}

fn form(ui: &mut egui::Ui, ed: &mut PeopleEditor, pal: &Palette, db: &CityDb, now: DateTime<Utc>) {
    let Mode::Form { index, field } = ed.mode else { return };
    let title = if index.is_some() { "EDIT" } else { "ADD" };
    ui.label(RichText::new(title).font(style::bold(13.0)).color(pal.accent));
    let focus_now = std::mem::take(&mut ed.focus_request);
    let mut clicked_field = None;
    egui::Grid::new("person-form").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        for f in Field::ALL {
            let hint = match f {
                Field::Name => "Ana",
                Field::City => "Lisbon  ·  Portland, OR",
                Field::Lat | Field::Lon => "optional",
                Field::Tz => "optional, e.g. Europe/Lisbon",
            };
            ui.label(RichText::new(f.label()).color(if f == field { pal.accent } else { pal.subtext }));
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
```

- [ ] **Step 2: Wire into `src/app.rs`.**
  - Imports: `use crate::cities::CityDb; use crate::people_file; use people_editor::{Action as EdAction, Key as EdKey, PeopleEditor};`
  - Field `people: Option<PeopleEditor>`, initialised to `None` in `new`.
  - Methods:

```rust
    fn open_people(&mut self) {
        let path = self
            .config
            .path
            .clone()
            .or_else(|| self.cli_path.clone())
            .unwrap_or_else(config::default_path);
        self.show_help = false;
        self.people = Some(PeopleEditor::open(path));
    }

    fn reload_team(&mut self, ctx: &egui::Context) {
        self.load_config();
        style::apply(ctx, self.palette());
    }

    /// While the editor is open it owns the keyboard. In the form, typing goes to the inputs.
    fn people_input(&mut self, ctx: &egui::Context) {
        let in_form = matches!(
            self.people.as_ref().map(|e| e.mode),
            Some(people_editor::Mode::Form { .. })
        );
        let keys = ctx.input_mut(|i| {
            let mut keys = Vec::new();
            for (mods, key, k) in [
                (Modifiers::SHIFT, Key::Tab, EdKey::BackTab),
                (Modifiers::NONE, Key::Tab, EdKey::Tab),
                (Modifiers::NONE, Key::ArrowUp, EdKey::Up),
                (Modifiers::NONE, Key::ArrowDown, EdKey::Down),
                (Modifiers::NONE, Key::Enter, EdKey::Enter),
                (Modifiers::NONE, Key::Escape, EdKey::Escape),
            ] {
                while i.consume_key(mods, key) {
                    keys.push(k);
                }
            }
            if !in_form {
                i.events.retain(|e| match e {
                    egui::Event::Text(t) => {
                        keys.extend(t.chars().map(EdKey::Char));
                        false
                    }
                    _ => true,
                });
            }
            keys
        });
        for k in keys {
            self.people_key(ctx, k);
        }
    }

    fn people_key(&mut self, ctx: &egui::Context, key: EdKey) {
        let Some(ed) = self.people.as_mut() else { return };
        let mut changed = false;
        match ed.handle_key(key, CityDb::embedded()) {
            EdAction::None => {}
            EdAction::Close => self.people = None,
            EdAction::Save { index, entry } => {
                let created = !ed.path.exists();
                let select = index.unwrap_or(ed.entries.len());
                match people_file::write_entry(&ed.path, index, &entry) {
                    Ok(()) => {
                        let msg = if created {
                            format!("created {}", ed.path.display())
                        } else {
                            format!("saved {}", entry.name)
                        };
                        ed.after_write(Some(select), msg);
                        changed = true;
                    }
                    Err(e) => ed.status = Some(format!("✗ {e:#}")),
                }
            }
            EdAction::Delete(i) => {
                let name = ed.entries.get(i).map(|e| e.name.clone()).unwrap_or_default();
                match people_file::remove_entry(&ed.path, i) {
                    Ok(()) => {
                        ed.after_write(Some(i), format!("deleted {name}"));
                        changed = true;
                    }
                    Err(e) => ed.status = Some(format!("✗ {e:#}")),
                }
            }
        }
        if changed {
            self.reload_team(ctx);
        }
    }
```

  - In `handle_input`, as the first statement: `if self.people.is_some() { self.people_input(ctx); return; }`. In the text `match c`, add `'p' | 'P' => self.open_people(),`. Change the `'r' | 'R'` arm to call `self.reload_team(ctx);` before the toast.
  - In `timeline`'s right-to-left button group, after `[T] theme`: `if ui.button("[P] people").clicked() { self.open_people(); }`.
  - In `help` rows, after the `R` row: `("P", "edit people"),`.
  - In `ui()`, after the help block:

```rust
        let pressed = self.people.as_mut().and_then(|ed| {
            people_editor::show(&ctx, ed, &pal, CityDb::embedded(), now)
        });
        if let Some(k) = pressed {
            self.people_key(&ctx, k);
        }
```

  - `README.md` keyboard table: add the row `| P | Edit people (add / edit / delete) |`.

- [ ] **Step 3: Run** `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`. Expected: clean, with all tests passing.

- [ ] **Step 4: Manual check.** Build with `cargo build --release`. Launch with a scratch copy of `examples/team.toml` and drive it with `drive.ps1` keystrokes:
  1. `p` opens the list.
  2. `{DOWN}`, then `e`; type a new city with `{TAB}`, then `{ENTER}`. The status shows "saved" and the map moves the person.
  3. `a`, type the name and city "Nowhereville", then `{ENTER}`: the form stays open showing ✗. Then fix the city and press `{ENTER}`.
  4. `d`, then `y` deletes the row.
  5. `{ESC}` closes; check the TOML still has its comments.

  Take a screenshot after each step.

- [ ] **Step 5: Commit** with `git add -A && git commit -m "Add PEOPLE modal to add, edit and delete team members"`.
