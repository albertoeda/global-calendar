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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const SAMPLE: &str = "# my team\ntheme = \"nord\"\n\n# lead\n[[person]]\nname = \"Ana\"\ncity = \"Lisbon\"\n\n[[person]]\nname = \"Kim\"\ncity = \"Tórshavn\"\nlat = 62\nlon = -6.77\ntz = \"Atlantic/Faroe\"\n";

    fn file(tag: &str, content: Option<&str>) -> PathBuf {
        let p =
            std::env::temp_dir().join(format!("team-clock-pf-{}-{tag}.toml", std::process::id()));
        match content {
            Some(c) => std::fs::write(&p, c).unwrap(),
            None => {
                let _ = std::fs::remove_file(&p);
            }
        }
        p
    }

    fn entry(name: &str, city: &str) -> Entry {
        Entry {
            name: name.into(),
            city: city.into(),
            ..Entry::default()
        }
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
        for needle in [
            "# my team",
            "# lead",
            "theme = \"nord\"",
            "city = \"Porto\"",
            "Atlantic/Faroe",
        ] {
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
        let names: Vec<String> = read_entries(&p)
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
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
