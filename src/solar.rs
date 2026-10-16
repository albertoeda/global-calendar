//! Where the sun is overhead, so the map can shade day and night.
//!
//! Uses the low-precision almanac formulas (accurate to ~0.01°), plenty for a dot map.

use chrono::{DateTime, Utc};

/// Latitude and longitude, in degrees, of the point where the sun is at the zenith.
pub fn subsolar_point(t: DateTime<Utc>) -> (f64, f64) {
    // Days since J2000.0 (2000-01-01 12:00 UTC).
    let n = (t.timestamp() as f64 - 946_728_000.0) / 86_400.0;
    let mean_lon = (280.460 + 0.985_647_4 * n).to_radians();
    let anomaly = (357.528 + 0.985_600_3 * n).to_radians();
    let ecl_lon = mean_lon
        + (1.915f64.to_radians() * anomaly.sin())
        + (0.020f64.to_radians() * (2.0 * anomaly).sin());
    let obliquity = (23.439 - 0.000_000_4 * n).to_radians();

    let declination = (obliquity.sin() * ecl_lon.sin()).asin();
    let right_ascension = (obliquity.cos() * ecl_lon.sin()).atan2(ecl_lon.cos());
    let gmst_deg = 280.460_618_37 + 360.985_647_366_29 * n;

    let lon = normalize_lon(right_ascension.to_degrees() - gmst_deg);
    (declination.to_degrees(), lon)
}

/// Altitude of the sun above the horizon, in degrees, seen from `(lat, lon)`.
pub fn sun_altitude(subsolar: (f64, f64), lat: f64, lon: f64) -> f64 {
    let (dec, sub_lon) = (subsolar.0.to_radians(), subsolar.1.to_radians());
    let (lat, lon) = (lat.to_radians(), lon.to_radians());
    (lat.sin() * dec.sin() + lat.cos() * dec.cos() * (lon - sub_lon).cos())
        .clamp(-1.0, 1.0)
        .asin()
        .to_degrees()
}

/// How much daylight a place gets: 1 in full day, 0 at night, ramping through civil twilight.
pub fn daylight(subsolar: (f64, f64), lat: f64, lon: f64) -> f32 {
    let alt = sun_altitude(subsolar, lat, lon);
    ((alt + 6.0) / 6.0).clamp(0.0, 1.0) as f32
}

fn normalize_lon(lon: f64) -> f64 {
    (lon + 180.0).rem_euclid(360.0) - 180.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap()
    }

    #[test]
    fn equinox_sun_is_over_equator() {
        let (lat, _) = subsolar_point(utc(2026, 3, 20, 14));
        assert!(lat.abs() < 0.5, "lat {lat}");
        let (lat, _) = subsolar_point(utc(2026, 9, 23, 0));
        assert!(lat.abs() < 0.5, "lat {lat}");
    }

    #[test]
    fn solstice_sun_is_over_tropics() {
        let (lat, _) = subsolar_point(utc(2026, 6, 21, 8));
        assert!((lat - 23.44).abs() < 0.1, "lat {lat}");
        let (lat, _) = subsolar_point(utc(2026, 12, 21, 20));
        assert!((lat + 23.44).abs() < 0.1, "lat {lat}");
    }

    #[test]
    fn noon_utc_sun_is_near_greenwich() {
        // Equation of time keeps it within ~4.5° of the meridian.
        let (_, lon) = subsolar_point(utc(2026, 4, 15, 12));
        assert!(lon.abs() < 4.5, "lon {lon}");
        let (_, lon) = subsolar_point(utc(2026, 4, 15, 18));
        assert!((lon + 90.0).abs() < 4.5, "lon {lon}");
    }

    #[test]
    fn day_and_night() {
        let sub = subsolar_point(utc(2026, 4, 15, 12));
        assert_eq!(daylight(sub, 51.5, 0.0), 1.0); // London at noon
        assert_eq!(daylight(sub, -36.8, 174.7), 0.0); // Auckland at midnight
    }
}
