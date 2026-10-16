# Global Team Clock — Rust world-map time viewer

## Context
A distributed team needs to see at a glance what time it is for everyone, and what time it *would* be for everyone at a chosen moment (e.g. "if I book 4pm, who's asleep?"). The repo (`C:\Users\Alberto\Dev\global-calendar`) is empty with no commits, and Rust isn't installed yet. The visual reference is a dot-matrix world map on a dark navy background, with highlighted dots in an accent colour.

Decisions confirmed with the user:
- **Native GUI using eframe/egui**, styled like a terminal: monospace font everywhere, flat panels, box-drawing borders and a status bar.
- **Team data comes from a hand-edited `team.toml`.** Users write only the name and city. A built-in city database supplies coordinates and the IANA timezone.
- **Map shading is a day/night terminator.** Dots in daylight are bright and night dots are dimmed. The band moves as the slider moves.
- **Declutter strategy:** group people by city, then place labels with a collision solver, using leader lines when needed. If labels still collide, merge nearby cities that share the same UTC offset into one block.

## Setup (first step)
- Install the Rust toolchain: `winget install Rustlang.Rustup`, then `rustup default stable`, which uses the MSVC target. This needs the user's OK at execution time.
- `cargo init --name team-clock`, add a `.gitignore`, and write a spec copy to `docs/superpowers/specs/2026-10-15-team-clock-design.md`.

## Crates
`eframe`/`egui` (UI), `chrono` + `chrono-tz` (time maths with an embedded tzdb, DST-correct), `iana-time-zone` (detects the user's local zone), `serde` + `toml` (config), `directories` (config dir fallback), `anyhow`. Dev: `proptest` (layout invariants).

## Module layout (`src/`)
| File | Responsibility |
|---|---|
| `main.rs` | Parses CLI (`team-clock [path/to/team.toml]`), loads config, launches eframe |
| `config.rs` | `team.toml` schema and loading. Search order: CLI arg → `./team.toml` → platform config dir via `directories` (`%APPDATA%\team-clock`, `~/Library/Application Support/team-clock`, `~/.config/team-clock`). Also holds the persisted theme name |
| `cities.rs` | Embedded city DB (`assets/cities.csv`: name, country, lat, lon, tz, about 1–2k cities with population >300k, from GeoNames cities15000). Case-insensitive lookup with `"Portland, US"` disambiguation |
| `timecalc.rs` | Given a reference `DateTime<Utc>` → each person's local time, UTC offset, day delta (−1/+1) and status (work 09–17 / edge 07–09, 17–22 / night) |
| `solar.rs` | Subsolar point for an instant, giving a day/night/twilight test for a lat/lon |
| `worldmap.rs` | Embedded land mask (`assets/landmask.bin`: 1° bit grid) and an equirectangular projection cropped to lat 84°N to 58°S, like the reference. Produces dot centres |
| `grouping.rs` | Groups people by resolved city. Merges groups when needed (see below) |
| `layout.rs` | Pure label-placement solver (no egui types): anchors + label sizes in, label rects + leader flags out |
| `theme.rs` | Palette struct (bg, land, land_night, accent, text, dim, ok/warn/bad) and built-in schemes |
| `app.rs` | egui `App`: map canvas, slider bar, status bar, help overlay, keybindings |

Assets are embedded with `include_bytes!`/`include_str!`, so the result is a single `.exe` with no runtime files besides `team.toml`. Font: embedded JetBrains Mono (OFL).

The land mask is generated once by `tools/gen_landmask` (a small bin crate, not shipped). It rasterises Natural Earth 110m land GeoJSON into the bit grid, and the output is committed.

## Config format
```toml
theme = "catppuccin-mocha"   # optional
[[person]]
name = "Ana"
city = "Lisbon"
[[person]]
name = "Raj"
city = "Bangalore"
# override when the city isn't in the DB:
[[person]]
name = "Kim"
city = "Tórshavn"
lat = 62.0; lon = -6.77; tz = "Atlantic/Faroe"
```
Errors (unknown city, bad tz, malformed TOML) appear as a readable list in an error panel inside the app, with line and name. The app still shows everyone who resolved correctly.

## Rendering
- **Map:** one filled circle per land cell, and the grid scales to the window. The dot colour is `land` in daylight and `land_night` at night, with a one-step blend in twilight (sun altitude from −6° to 0°). Ocean is `bg`.
- **Markers:** an accent-coloured ring or dot at each city group.
- **Label block** (monospace, bordered with box-drawing style):
  ```
  ┌ Lisbon 14:30 +0 ─┐
  │ Ana · João       │
  └──────────────────┘
  ```
  The time is coloured by status: ok, warn or bad (night). A day delta shows as `+1d`/`−1d`. Long name lists wrap at about 3 names per line.
- **Merge rule** (in `grouping.rs` + `layout.rs`):
  1. Group by city.
  2. Run the solver.
  3. If any label still can't be placed without overlapping, merge the colliding groups that have the same UTC offset and lie within N px of each other. The merged block is headed `London/Paris 14:30`.
  4. Re-run the solver, at most 3 iterations.
- **Solver:**
  1. Place groups greedily, east to west, ordered by number of people.
  2. Try 8 positions around the anchor at increasing radii.
  3. Fall back to the nearest free spot along a spiral, with a thin leader line.
  4. Labels must also avoid other markers and stay inside the canvas.

## Time slider & controls
- **Status bar (top):** `YOU: Madrid 14:30 CEST · Thu 15 Oct` plus a `[LIVE]` or `[SCRUB]` indicator.
- **Slider (bottom):** covers the selected local day, 00:00 to 24:00, in 15-minute steps. A tick ruler shows hours, plus coloured bands for when most of the team is in working hours.
- **Now button:** `[ N ] Now` returns to live mode. In live mode the time ticks every 30 seconds.
- **Keyboard (everything works without a mouse):**
  - `←/→` ±15 min; `Shift+←/→` ±1 h; `[`/`]` ±1 day; `Home/End` start or end of day
  - `N` return to now
  - `T`/`Shift+T` cycle themes forward or back; the choice is saved to config
  - `Tab`/`Shift+Tab` cycle focus through city groups, which highlights the block and shows the full name list
  - `?` help overlay; `R` reload `team.toml`; `Q`/`Esc` quit or close the overlay
- The mouse works too: drag the slider or click the buttons. Focus rings are visible for keyboard users.

## Themes (`theme.rs`)
Catppuccin Latte / Frappé / Macchiato / Mocha, Gruvbox Dark / Light, Nord, Dracula, Tokyo Night, Solarized Dark / Light, and One Dark. Each theme maps its official palette onto the role slots. Default: Catppuccin Mocha, which is closest to the reference.

## Testing
- `timecalc`: DST transitions (London and New York on their change weekends), half-hour and 45-minute zones (India, Nepal, Chatham), day-delta across the date line.
- `solar`: the subsolar latitude is about 0° at the equinoxes and about ±23.44° at the solstices; noon at Greenwich is daylight.
- `layout`: a proptest checks that for random anchors, no two placed rects overlap and all of them stay in bounds. A merge test checks that 4 people in London, Paris, Brussels and Amsterdam on a small canvas end up in at most 2 blocks.
- `cities`/`config`: lookup, disambiguation, overrides, and error reporting.
- `cargo clippy -- -D warnings` and `cargo fmt --check`.

## Verification
1. Run `cargo test` and `cargo clippy`.
2. Run `cargo run --release -- examples/team.toml`. The sample team covers about 12 people in about 9 cities, including a dense Europe cluster, India (+5:30), Auckland and SF.
3. Check manually, using only the keyboard:
   - no overlapping labels at the default and minimum window sizes
   - the slider moves the terminator and the times
   - `N` restores live mode
   - `T` cycles every theme
   - `Tab` focus works
4. Use the `run` skill to take screenshots and compare them with the reference.

## Cross-platform (Windows, macOS, Linux)
- No OS-specific code. All assets are embedded, the config dir comes from `directories`, and the local zone comes from `iana-time-zone`.
- eframe features: `glow`, `wayland` + `x11` (JetBrains Mono embedded for the terminal look).
- Keybindings are single keys, the same on all OSes. macOS also gets ⌘Q.
- CI: `.github/workflows/ci.yml` runs `fmt`, `clippy`, `test` and a release build on ubuntu/macos/windows. Ubuntu installs `libxkbcommon-dev libwayland-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libgl1-mesa-dev`.
- README documents build deps per OS.

## Housekeeping
- Commit in logical steps on a feature branch (`main` has no commits, so make the first commit the scaffold).
- Log a terse plan summary in the shared Todoist project under a `global-calendar` section with the `plan` label, after checking that the section exists.
