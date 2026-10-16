//! Regenerates the embedded data assets.
//!
//! ```text
//! gen-assets landmask <ne_50m_land.geojson> <out/landmask.bin>
//! gen-assets cities <cities15000.txt> <countryInfo.txt> <admin1CodesASCII.txt> <out/cities.csv>
//! ```
//!
//! Inputs come from Natural Earth (public domain) and GeoNames (CC-BY 4.0).

use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;

/// Must match `worldmap::MASK_RES` in the app.
const RES: usize = 4; // cells per degree
const W: usize = 360 * RES;
const H: usize = 180 * RES;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("landmask") if args.len() == 3 => landmask(&args[1], &args[2]),
        Some("cities") if args.len() == 5 => cities(&args[1], &args[2], &args[3], &args[4]),
        _ => bail!("usage: gen-assets landmask <geojson> <out> | cities <cities> <countries> <admin1> <out>"),
    }
}

type Ring = Vec<(f64, f64)>;

struct Poly {
    rings: Vec<Ring>,
    min: (f64, f64),
    max: (f64, f64),
}

fn parse_ring(v: &Value) -> Ring {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| Some((p.get(0)?.as_f64()?, p.get(1)?.as_f64()?)))
        .collect()
}

fn parse_polygon(v: &Value) -> Poly {
    let rings: Vec<Ring> = v.as_array().into_iter().flatten().map(parse_ring).collect();
    let mut min = (f64::MAX, f64::MAX);
    let mut max = (f64::MIN, f64::MIN);
    for &(x, y) in rings.iter().flatten() {
        min = (min.0.min(x), min.1.min(y));
        max = (max.0.max(x), max.1.max(y));
    }
    Poly { rings, min, max }
}

/// Even-odd test across all rings, so holes (e.g. the Caspian) are excluded.
fn contains(poly: &Poly, x: f64, y: f64) -> bool {
    if x < poly.min.0 || x > poly.max.0 || y < poly.min.1 || y > poly.max.1 {
        return false;
    }
    let mut inside = false;
    for ring in &poly.rings {
        let n = ring.len();
        let mut j = n.wrapping_sub(1);
        for i in 0..n {
            let (xi, yi) = ring[i];
            let (xj, yj) = ring[j];
            if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                inside = !inside;
            }
            j = i;
        }
    }
    inside
}

fn landmask(input: &str, out: &str) -> Result<()> {
    let json: Value = serde_json::from_str(&fs::read_to_string(input)?)?;
    let mut polys = Vec::new();
    for f in json["features"].as_array().context("no features")? {
        let g = &f["geometry"];
        match g["type"].as_str() {
            Some("Polygon") => polys.push(parse_polygon(&g["coordinates"])),
            Some("MultiPolygon") => {
                for p in g["coordinates"].as_array().into_iter().flatten() {
                    polys.push(parse_polygon(p));
                }
            }
            _ => {}
        }
    }
    // Row-major from 90°N southwards, columns from 180°W eastwards, LSB-first bits.
    let mut bits = vec![0u8; W * H / 8];
    let mut land = 0usize;
    for row in 0..H {
        let lat = 90.0 - (row as f64 + 0.5) / RES as f64;
        for col in 0..W {
            let lon = -180.0 + (col as f64 + 0.5) / RES as f64;
            if polys.iter().any(|p| contains(p, lon, lat)) {
                let i = row * W + col;
                bits[i / 8] |= 1 << (i % 8);
                land += 1;
            }
        }
    }
    fs::write(out, &bits)?;
    eprintln!("{} polygons, {land}/{} land cells", polys.len(), W * H);
    Ok(())
}

const MIN_POPULATION: u64 = 100_000;

fn ascii_alias(s: &str) -> bool {
    s.len() >= 3
        && s.len() <= 40
        && s.chars().all(|c| c.is_ascii_alphabetic() || " -.'".contains(c))
        && s.chars().any(|c| c.is_ascii_lowercase())
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn cities(cities: &str, countries: &str, admin1: &str, out: &str) -> Result<()> {
    let country_names: HashMap<String, String> = fs::read_to_string(countries)?
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            Some((c.first()?.to_string(), c.get(4)?.to_string()))
        })
        .collect();
    let admin1_names: HashMap<String, String> = fs::read_to_string(admin1)?
        .lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            Some((c.first()?.to_string(), c.get(2)?.to_string()))
        })
        .collect();

    let mut rows = Vec::new();
    for line in fs::read_to_string(cities)?.lines() {
        let c: Vec<&str> = line.split('\t').collect();
        if c.len() < 18 {
            continue;
        }
        let population: u64 = c[14].parse().unwrap_or(0);
        if population < MIN_POPULATION && c[7] != "PPLC" {
            continue;
        }
        let (name, ascii, cc, admin1_code, tz) = (c[1], c[2], c[8], c[10], c[17]);
        let mut seen: HashSet<String> = [name.to_lowercase(), ascii.to_lowercase()].into();
        let alts: Vec<&str> = c[3]
            .split(',')
            .filter(|a| ascii_alias(a) && seen.insert(a.to_lowercase()))
            .collect();
        let admin1_name = admin1_names
            .get(&format!("{cc}.{admin1_code}"))
            .map_or("", String::as_str);
        rows.push((population, [
            name.to_string(),
            ascii.to_string(),
            alts.join("|"),
            cc.to_string(),
            country_names.get(cc).cloned().unwrap_or_default(),
            admin1_code.to_string(),
            admin1_name.to_string(),
            c[4].to_string(),
            c[5].to_string(),
            tz.to_string(),
            population.to_string(),
        ]));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    let mut csv = String::from("name,ascii,alts,cc,country,admin1_code,admin1,lat,lon,tz,population\n");
    for (_, fields) in &rows {
        let line: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
        csv.push_str(&line.join(","));
        csv.push('\n');
    }
    fs::write(out, csv)?;
    eprintln!("{} cities", rows.len());
    Ok(())
}
