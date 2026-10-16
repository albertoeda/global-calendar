//! `team.toml`: who is on the team and where they are.

use crate::cities::CityDb;
use crate::people_file::Entry;
use chrono_tz::Tz;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    pub name: String,
    /// City as the user wrote it, without any ", country" qualifier.
    pub city: String,
    pub lat: f64,
    pub lon: f64,
    pub tz: Tz,
}

#[derive(Clone, Debug, Default)]
pub struct TeamConfig {
    /// The file this was loaded from, if any.
    pub path: Option<PathBuf>,
    pub theme: Option<String>,
    pub people: Vec<Person>,
    /// Human-readable problems. People that did resolve are still in `people`.
    pub errors: Vec<String>,
}

#[derive(Deserialize)]
struct RawConfig {
    theme: Option<String>,
    #[serde(default, rename = "person")]
    people: Vec<RawPerson>,
}

#[derive(Deserialize)]
struct RawPerson {
    #[serde(default)]
    name: String,
    #[serde(default)]
    city: String,
    lat: Option<Num>,
    lon: Option<Num>,
    tz: Option<String>,
}

/// Accepts both `62` and `62.0` in TOML.
#[derive(Deserialize, Clone, Copy)]
#[serde(untagged)]
enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    fn get(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(f) => f,
        }
    }
}

/// Where we look for `team.toml`, in order.
pub fn search_paths(cli: Option<&Path>) -> Vec<PathBuf> {
    if let Some(p) = cli {
        return vec![p.to_path_buf()];
    }
    let mut paths = vec![PathBuf::from("team.toml")];
    if let Some(dirs) = directories::ProjectDirs::from("", "", "team-clock") {
        paths.push(dirs.config_dir().join("team.toml"));
    }
    paths
}

/// Loads the first existing file from [`search_paths`].
pub fn load(cli: Option<&Path>) -> TeamConfig {
    let paths = search_paths(cli);
    let Some(path) = paths.iter().find(|p| p.is_file()) else {
        let looked = paths
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return TeamConfig {
            errors: vec![format!(
                "No team.toml found. Looked in: {looked}. See examples/team.toml."
            )],
            ..TeamConfig::default()
        };
    };
    let mut cfg = match std::fs::read_to_string(path) {
        Ok(text) => parse(&text, CityDb::embedded()),
        Err(e) => TeamConfig {
            errors: vec![format!("Could not read {}: {e}", path.display())],
            ..TeamConfig::default()
        },
    };
    cfg.path = Some(path.clone());
    cfg
}

/// Parses config text. The returned `path` is `None`.
pub fn parse(text: &str, db: &CityDb) -> TeamConfig {
    let raw: RawConfig = match toml::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            return TeamConfig {
                errors: vec![format!("team.toml is not valid TOML: {e}")],
                ..TeamConfig::default()
            };
        }
    };
    let mut cfg = TeamConfig {
        theme: raw.theme,
        ..TeamConfig::default()
    };
    for (i, p) in raw.people.iter().enumerate() {
        match resolve_entry(&Entry::from(p), db) {
            Ok(person) => cfg.people.push(person),
            Err(msg) => cfg
                .errors
                .push(format!("person #{} \"{}\": {msg}", i + 1, p.name.trim())),
        }
    }
    cfg
}

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

/// Validates one entry and places it, using the city database unless lat, lon and tz are all given.
pub fn resolve_entry(p: &Entry, db: &CityDb) -> Result<Person, String> {
    let name = p.name.trim();
    if name.is_empty() {
        return Err("name is empty".to_string());
    }
    let tz = match &p.tz {
        Some(s) => Some(
            s.trim()
                .parse::<Tz>()
                .map_err(|_| format!("unknown timezone \"{s}\""))?,
        ),
        None => None,
    };
    let (lat, lon) = (p.lat, p.lon);
    let (lat, lon, tz) = match (lat, lon, tz) {
        (Some(lat), Some(lon), Some(tz)) => (lat, lon, tz),
        _ => {
            let city = db.lookup(&p.city).ok_or_else(|| {
                format!(
                    "unknown city \"{}\" — add lat, lon and tz to place it manually",
                    p.city.trim()
                )
            })?;
            (
                lat.unwrap_or(city.lat),
                lon.unwrap_or(city.lon),
                tz.unwrap_or(city.tz),
            )
        }
    };
    if !(-90.0..=90.0).contains(&lat) {
        return Err(format!("lat {lat} is outside -90..90"));
    }
    if !(-180.0..=180.0).contains(&lon) {
        return Err(format!("lon {lon} is outside -180..180"));
    }
    let city = p.city.split(',').next().unwrap_or("").trim().to_string();
    Ok(Person {
        name: name.to_string(),
        city,
        lat,
        lon,
        tz,
    })
}

/// Sets the top-level `theme` key in the file, preserving comments and formatting.
pub fn save_theme(path: &Path, theme_id: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("could not read {}", path.display()))?;
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .with_context(|| format!("could not parse {}", path.display()))?;
    doc["theme"] = toml_edit::value(theme_id);
    std::fs::write(path, doc.to_string())
        .with_context(|| format!("could not write {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(text: &str) -> TeamConfig {
        parse(text, CityDb::embedded())
    }

    #[test]
    fn resolve_entry_shares_rules() {
        use crate::people_file::Entry;
        let ok = Entry {
            name: "Ana".into(),
            city: "Lisbon".into(),
            ..Entry::default()
        };
        assert_eq!(
            resolve_entry(&ok, CityDb::embedded()).unwrap().tz,
            chrono_tz::Europe::Lisbon
        );
        let bad = Entry {
            name: "X".into(),
            city: "Nowhereville".into(),
            ..Entry::default()
        };
        assert!(
            resolve_entry(&bad, CityDb::embedded())
                .unwrap_err()
                .contains("unknown city")
        );
    }

    #[test]
    fn happy_path() {
        let cfg = p(r#"
theme = "catppuccin-mocha"
[[person]]
name = " Ana "
city = "Lisbon"
[[person]]
name = "Kim"
city = "Tórshavn"
lat = 62.0
lon = -6.77
tz = "Atlantic/Faroe"
[[person]]
name = "Bo"
city = "Portland, OR"
extra = "ignored"
"#);
        assert!(cfg.errors.is_empty(), "{:?}", cfg.errors);
        assert_eq!(cfg.path, None);
        assert_eq!(cfg.theme.as_deref(), Some("catppuccin-mocha"));
        assert_eq!(cfg.people.len(), 3);
        assert_eq!(cfg.people[0].name, "Ana");
        assert_eq!(cfg.people[0].tz, chrono_tz::Europe::Lisbon);
        assert_eq!(cfg.people[1].tz, chrono_tz::Atlantic::Faroe);
        assert_eq!(cfg.people[1].lat, 62.0);
        assert_eq!(cfg.people[2].city, "Portland");
        assert_eq!(cfg.people[2].tz, chrono_tz::America::Los_Angeles);
    }

    #[test]
    fn empty_file() {
        let cfg = p("");
        assert!(cfg.errors.is_empty());
        assert!(cfg.people.is_empty());
        assert_eq!(cfg.theme, None);
    }

    #[test]
    fn manual_place_without_known_city() {
        let cfg = p(r#"
[[person]]
name = "X"
city = "Nowhereville"
lat = 10
lon = 20
tz = "UTC"
"#);
        assert!(cfg.errors.is_empty(), "{:?}", cfg.errors);
        assert_eq!(cfg.people[0].lat, 10.0);
        assert_eq!(cfg.people[0].city, "Nowhereville");
    }

    #[test]
    fn partial_overrides() {
        let cfg = p(r#"
[[person]]
name = "Ana"
city = "Lisbon"
tz = "Europe/Madrid"
lat = 1.5
"#);
        assert!(cfg.errors.is_empty(), "{:?}", cfg.errors);
        let a = &cfg.people[0];
        assert_eq!(a.tz, chrono_tz::Europe::Madrid);
        assert_eq!(a.lat, 1.5);
        let lisbon = CityDb::embedded().lookup("Lisbon").unwrap();
        assert_eq!(a.lon, lisbon.lon);
    }

    #[test]
    fn errors() {
        let cfg = p(r#"
[[person]]
name = "Ok"
city = "London"
[[person]]
name = "Zed"
city = "Nowhereville, XX"
lat = 1.0
[[person]]
name = "Tim"
city = "Lisbon"
tz = "Mars/Base"
[[person]]
name = "Lat"
city = "Lisbon"
lat = 91
[[person]]
name = "Lon"
city = "Lisbon"
lon = -181.0
[[person]]
name = "  "
city = "Lisbon"
"#);
        assert_eq!(cfg.people.len(), 1);
        assert_eq!(cfg.people[0].name, "Ok");
        assert_eq!(cfg.errors.len(), 5, "{:?}", cfg.errors);
        assert_eq!(
            cfg.errors[0],
            "person #2 \"Zed\": unknown city \"Nowhereville, XX\" — add lat, lon and tz to place it manually"
        );
        assert_eq!(
            cfg.errors[1],
            "person #3 \"Tim\": unknown timezone \"Mars/Base\""
        );
        assert!(cfg.errors[2].starts_with("person #4 \"Lat\": lat"));
        assert!(cfg.errors[3].starts_with("person #5 \"Lon\": lon"));
        assert!(cfg.errors[4].starts_with("person #6"));
    }

    #[test]
    fn syntax_error() {
        let cfg = p("theme = \"x\"\n[[person]\nname = 1\n");
        assert!(cfg.people.is_empty());
        assert_eq!(cfg.errors.len(), 1);
        assert!(cfg.errors[0].contains("line"), "{}", cfg.errors[0]);
    }

    #[test]
    fn search_paths_cli() {
        let cli = Path::new("some/where.toml");
        assert_eq!(search_paths(Some(cli)), vec![cli.to_path_buf()]);
        let def = search_paths(None);
        assert_eq!(def[0], PathBuf::from("team.toml"));
        assert!(def.len() <= 2);
    }

    #[test]
    fn load_missing_and_existing() {
        let missing = std::env::temp_dir().join(format!(
            "team-clock-missing-{}-{:?}.toml",
            std::process::id(),
            std::thread::current().id()
        ));
        let cfg = load(Some(&missing));
        assert_eq!(cfg.errors.len(), 1);
        assert!(cfg.errors[0].starts_with("No team.toml found. Looked in: "));
        assert!(cfg.errors[0].ends_with("See examples/team.toml."));

        let path = temp_file("load");
        std::fs::write(&path, "[[person]]\nname = \"Ana\"\ncity = \"Lisbon\"\n").unwrap();
        let cfg = load(Some(&path));
        let _ = std::fs::remove_file(&path);
        assert_eq!(cfg.path.as_deref(), Some(path.as_path()));
        assert_eq!(cfg.people.len(), 1);
    }

    fn temp_file(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "team-clock-test-{tag}-{}-{nanos}.toml",
            std::process::id()
        ))
    }

    #[test]
    fn save_theme_round_trip() {
        let path = temp_file("theme");
        let original = "# my team\n[[person]]\nname = \"Ana\" # lead\ncity = \"Lisbon\"\n";
        std::fs::write(&path, original).unwrap();

        save_theme(&path, "nord").unwrap();
        let after_insert = std::fs::read_to_string(&path).unwrap();
        save_theme(&path, "dracula").unwrap();
        let after_update = std::fs::read_to_string(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        for text in [&after_insert, &after_update] {
            assert!(text.contains("# my team"), "{text}");
            assert!(text.contains("# lead"), "{text}");
        }
        let a = p(&after_insert);
        assert_eq!(a.theme.as_deref(), Some("nord"));
        assert_eq!(a.people.len(), 1);
        let b = p(&after_update);
        assert_eq!(b.theme.as_deref(), Some("dracula"));
        assert_eq!(b.people.len(), 1);
        assert_eq!(after_update.matches("theme").count(), 1);
    }
}
