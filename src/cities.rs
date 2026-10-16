//! Embedded city database (GeoNames, cities with population >= 100k plus capitals).
//! Resolves a free-text city like "Portland, OR" to coordinates and an IANA timezone.

use chrono_tz::Tz;

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
}

impl CityDb {
    /// The database built from `assets/cities.csv`, parsed once on first use.
    pub fn embedded() -> &'static CityDb {
        todo!()
    }

    /// Parses the CSV produced by `tools/gen-assets`. Rows with an unknown timezone are skipped.
    pub fn parse(csv: &str) -> CityDb {
        let _ = csv;
        todo!()
    }

    /// Finds the best match for `query`, e.g. "Lisbon", "sao paulo", "Bangalore", "Portland, US",
    /// "Portland, OR", "Portland, Oregon". Case- and whitespace-insensitive.
    pub fn lookup(&self, query: &str) -> Option<&City> {
        let _ = query;
        todo!()
    }

    pub fn len(&self) -> usize {
        self.cities.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cities.is_empty()
    }
}
