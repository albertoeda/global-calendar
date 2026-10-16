# People editor — design

## Goal
Edit the team (add, edit, delete people) from inside Team Clock, using only the keyboard, without hand-editing `team.toml`.

## Decisions
- **UI:** a modal overlay (like the help overlay), opened with `P` or a `[P] people` button in the timeline bar.
- **Storage:** edit `team.toml` in place with `toml_edit`, preserving comments, formatting and the `theme` key. After every successful write the app reloads the config, so the map reflects the change when the modal closes.
- **No file yet:** the first save creates `team.toml` in the platform config dir (`config::search_paths(None).last()`); a toast reports the path.

## Behaviour
List mode:
- Rows show `name  city  UTC±h` in file order. Entries that don't resolve show `✗ <reason>` in the bad colour.
- Keys:
  - `↑/↓` select
  - `a` add
  - `Enter`/`e` edit
  - `d` delete (then `y` confirm / any other key cancel)
  - `Esc` close

Form mode (add or edit):
- Fields: name, city, lat, lon, tz. The last three are optional and only needed when the city isn't in the database.
- A live status line resolves the entry with the same rules as config loading:
  - success: `✓ <City>, <admin1>, <CC> · <tz> · UTC±h`
  - failure: `✗ <reason>`
- Keys:
  - `Tab`/`Shift+Tab` move between fields
  - `Enter` saves (refused while the status is ✗)
  - `Esc` cancels back to the list

While the modal is open, global shortcuts are suspended so typing is never hijacked.

## Units
- **`src/people_file.rs`** (pure, no egui):
  - `Entry { name, city, lat: Option<f64>, lon: Option<f64>, tz: Option<String> }`
  - `read_entries(path) -> Result<Vec<Entry>>`
  - `write_entry(path, index: Option<usize>, &Entry) -> Result<()>`: replaces the entry at `index`, or appends when `None`; creates the file and its parent dirs if missing
  - `remove_entry(path, index) -> Result<()>`
  - Entries map to `[[person]]` tables; optional fields are removed from the table when `None`.
- **`src/config.rs`:**
  - `resolve_entry(&Entry, &CityDb) -> Result<Person, String>`, shared by `parse` and the editor so the rules can't drift
  - `default_path() -> Option<PathBuf>`
- **`src/app/people_editor.rs`:**
  - state machine `Mode::{List, Form, ConfirmDelete}`
  - key handling as a pure `handle_key(&mut self, KeyInput) -> Action`, so it can be tested without egui
  - rendering inside an `egui::Modal`

## Errors
- Write and parse failures appear on the editor's status line, and the form keeps its contents.
- A malformed `team.toml` (TOML syntax error) opens the editor read-only, with the parse error shown, so nothing gets clobbered.

## Testing
- `people_file`:
  - round-trips that preserve comments, other entries and `theme`
  - insert, replace and remove at index
  - optional-field removal
  - creating a missing file
- `config::resolve_entry` keeps the existing parse tests green.
- `people_editor`:
  - selection bounds
  - add, edit and delete flows, including delete confirmation
  - `Enter` refused while the entry is invalid
- Manual: run the app, then use only the keyboard to add someone in a new city, edit a city, delete a person and confirm the map updates.
