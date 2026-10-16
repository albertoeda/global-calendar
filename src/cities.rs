//! Embedded city database (GeoNames, cities with population >= 100k plus capitals).
//! Resolves a free-text city like "Portland, OR" to coordinates and an IANA timezone.

use chrono_tz::Tz;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq)]
pub struct City {
    pub name: String,
    pub country_code: String,
    pub country: String,
    pub admin1: String,
    pub lat: f64,
    pub lon: f64,
    pub tz: Tz,
    pub population: u64,
}

pub struct CityDb {
    cities: Vec<City>,
    /// Normalized admin1 code per city (parallel to `cities`); only used for qualifier matching.
    admin1_codes: Vec<String>,
    /// Normalized name -> (city index, matched a primary name rather than an alternate).
    index: HashMap<String, Vec<(usize, bool)>>,
}

const COLUMNS: usize = 11;

impl CityDb {
    /// The database built from `assets/cities.csv`, parsed once on first use.
    pub fn embedded() -> &'static CityDb {
        static DB: OnceLock<CityDb> = OnceLock::new();
        DB.get_or_init(|| CityDb::parse(include_str!("../assets/cities.csv")))
    }

    /// Parses the CSV produced by `tools/gen-assets`. Rows with an unknown timezone are skipped.
    pub fn parse(csv: &str) -> CityDb {
        let mut db = CityDb {
            cities: Vec::new(),
            admin1_codes: Vec::new(),
            index: HashMap::new(),
        };
        for line in csv.lines().skip(1) {
            if line.trim().is_empty() {
                continue;
            }
            let f = split_csv_line(line);
            if f.len() != COLUMNS {
                continue;
            }
            let (Ok(tz), Ok(lat), Ok(lon)) = (
                f[9].trim().parse::<Tz>(),
                f[7].trim().parse::<f64>(),
                f[8].trim().parse::<f64>(),
            ) else {
                continue;
            };
            let idx = db.cities.len();
            let mut add = |key: &str, primary: bool| {
                let key = normalize(key);
                if key.is_empty() {
                    return;
                }
                let entry = db.index.entry(key).or_default();
                match entry.iter_mut().find(|(i, _)| *i == idx) {
                    Some(existing) => existing.1 |= primary,
                    None => entry.push((idx, primary)),
                }
            };
            add(&f[0], true);
            add(&f[1], true);
            for alt in f[2].split('|') {
                add(alt, false);
            }
            db.admin1_codes.push(normalize(&f[5]));
            db.cities.push(City {
                name: f[0].trim().to_string(),
                country_code: f[3].trim().to_string(),
                country: f[4].trim().to_string(),
                admin1: f[6].trim().to_string(),
                lat,
                lon,
                tz,
                population: f[10].trim().parse().unwrap_or(0),
            });
        }
        db
    }

    /// Finds the best match for `query`, e.g. "Lisbon", "sao paulo", "Bangalore", "Portland, US",
    /// "Portland, OR", "Portland, Oregon". Case- and whitespace-insensitive.
    pub fn lookup(&self, query: &str) -> Option<&City> {
        let (city_part, qualifier) = match query.split_once(',') {
            Some((c, q)) => (c, Some(normalize(q)).filter(|q| !q.is_empty())),
            None => (query, None),
        };
        let candidates = self.index.get(&normalize(city_part))?;
        candidates
            .iter()
            .filter(|(i, _)| match &qualifier {
                None => true,
                Some(q) => {
                    let c = &self.cities[*i];
                    *q == normalize(&c.country_code)
                        || *q == normalize(&c.country)
                        || *q == self.admin1_codes[*i]
                        || *q == normalize(&c.admin1)
                }
            })
            // Primary-name matches first, then larger population.
            .max_by_key(|(i, primary)| (*primary, self.cities[*i].population, usize::MAX - i))
            .map(|(i, _)| &self.cities[*i])
    }

    pub fn len(&self) -> usize {
        self.cities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cities.is_empty()
    }
}

/// Trim, lowercase and collapse inner whitespace.
fn normalize(s: &str) -> String {
    s.split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Splits one CSV line. Fields may be double-quoted, with `""` as an escaped quote.
fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cur.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
        } else {
            match c {
                '"' => in_quotes = true,
                ',' => fields.push(std::mem::take(&mut cur)),
                _ => cur.push(c),
            }
        }
    }
    fields.push(cur);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(q: &str) -> Option<&'static City> {
        CityDb::embedded().lookup(q)
    }

    #[test]
    fn embedded_is_large() {
        assert!(CityDb::embedded().len() > 5000);
        assert!(!CityDb::embedded().is_empty());
    }

    #[test]
    fn simple_and_whitespace() {
        let c = lookup("Lisbon").unwrap();
        assert_eq!(c.tz, chrono_tz::Europe::Lisbon);
        assert_eq!(c.country_code, "PT");
        let ny = lookup("  new   york ").unwrap();
        assert_eq!(ny.name, "New York City");
        assert_eq!(ny.tz, chrono_tz::America::New_York);
    }

    #[test]
    fn accents_and_ascii() {
        assert_eq!(lookup("Sao Paulo").unwrap().name, "São Paulo");
        assert_eq!(lookup("São Paulo").unwrap().name, "São Paulo");
        assert_eq!(lookup("SÃO PAULO").unwrap().name, "São Paulo");
    }

    #[test]
    fn alternate_names() {
        let c = lookup("Bangalore").unwrap();
        assert_eq!(c.name, "Bengaluru");
        assert_eq!(c.tz, chrono_tz::Asia::Kolkata);
    }

    #[test]
    fn qualifiers() {
        let p = lookup("Portland").unwrap();
        assert_eq!(p.admin1, "Oregon");
        assert_eq!(lookup("Portland, US").unwrap().admin1, "Oregon");
        assert_eq!(lookup("Portland, OR").unwrap().admin1, "Oregon");
        assert_eq!(lookup("portland ,  oregon").unwrap().admin1, "Oregon");
        assert_eq!(lookup("Portland, United States").unwrap().admin1, "Oregon");
        // Portland, Maine (pop. ~68k) is below the database's 100k cutoff, so a Maine
        // qualifier finds nothing rather than silently picking Oregon.
        assert!(lookup("Portland, ME").is_none());
        assert!(lookup("Portland, Maine").is_none());

        let l = lookup("London").unwrap();
        assert_eq!(l.country_code, "GB");
        let on = lookup("London, CA").unwrap();
        assert_eq!(on.admin1, "Ontario");
        assert_eq!(on.tz, chrono_tz::America::Toronto);
        assert_eq!(lookup("London, Ontario").unwrap().admin1, "Ontario");
    }

    #[test]
    fn unknown() {
        assert!(lookup("Nowhereville").is_none());
        assert!(lookup("").is_none());
        assert!(lookup("Lisbon, Narnia").is_none());
    }

    #[test]
    fn primary_beats_alt() {
        let csv = "name,ascii,alts,cc,country,admin1_code,admin1,lat,lon,tz,population\n\
                   Big,Big,Smallville,XX,Xland,01,One,1,1,UTC,1000\n\
                   Smallville,Smallville,,YY,Yland,02,Two,2,2,UTC,10\n";
        let db = CityDb::parse(csv);
        assert_eq!(db.lookup("smallville").unwrap().name, "Smallville");
    }

    #[test]
    fn csv_quotes_and_bad_tz() {
        let csv = "name,ascii,alts,cc,country,admin1_code,admin1,lat,lon,tz,population\n\
                   \"Foo, \"\"Bar\"\"\",Foo Bar,Fb|Foob,XX,\"Land, The \",01,One,1.5,-2.5,Europe/Lisbon,500\n\
                   Bad,Bad,,XX,Land,01,One,0,0,Mars/Olympus,900\n";
        let db = CityDb::parse(csv);
        assert_eq!(db.len(), 1);
        let c = db.lookup("foo bar").unwrap();
        assert_eq!(c.name, "Foo, \"Bar\"");
        assert_eq!(c.country, "Land, The");
        assert_eq!(c.lat, 1.5);
        assert_eq!(c.lon, -2.5);
        assert_eq!(c.population, 500);
        assert!(db.lookup("Foob").is_some());
        assert!(db.lookup("foo bar, land, the").is_some());
        assert!(db.lookup("Bad").is_none());
    }
}
