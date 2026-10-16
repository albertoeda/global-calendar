//! Local-time maths: what time it is for someone, and whether that's a good time to reach them.

use chrono::{
    DateTime, Duration, LocalResult, NaiveDate, NaiveTime, Offset, TimeZone, Timelike, Utc,
};
use chrono_tz::Tz;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// 09:00–17:00
    Work,
    /// 07:00–09:00 and 17:00–22:00
    Edge,
    /// 22:00–07:00
    Night,
}

pub fn status_at(t: NaiveTime) -> Status {
    match t.hour() {
        9..=16 => Status::Work,
        7..=8 | 17..=21 => Status::Edge,
        _ => Status::Night,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LocalInfo {
    pub time: DateTime<Tz>,
    pub offset_secs: i32,
    /// Calendar days ahead of (+) or behind (−) the viewer.
    pub day_delta: i64,
    pub status: Status,
}

pub fn local_info(instant: DateTime<Utc>, tz: Tz, viewer: Tz) -> LocalInfo {
    let time = instant.with_timezone(&tz);
    let viewer_date = instant.with_timezone(&viewer).date_naive();
    LocalInfo {
        time,
        offset_secs: time.offset().fix().local_minus_utc(),
        day_delta: (time.date_naive() - viewer_date).num_days(),
        status: status_at(time.time()),
    }
}

/// UTC offset of `tz` at `instant`, in seconds.
pub fn offset_secs(instant: DateTime<Utc>, tz: Tz) -> i32 {
    instant.with_timezone(&tz).offset().fix().local_minus_utc()
}

/// "UTC+5:30", "UTC−7", "UTC".
pub fn format_offset(secs: i32) -> String {
    if secs == 0 {
        return "UTC".into();
    }
    let sign = if secs < 0 { '−' } else { '+' };
    let (h, m) = (secs.abs() / 3600, secs.abs() % 3600 / 60);
    if m == 0 {
        format!("UTC{sign}{h}")
    } else {
        format!("UTC{sign}{h}:{m:02}")
    }
}

/// "+1d", "−1d", or "" for the same day.
pub fn format_day_delta(days: i64) -> String {
    match days {
        0 => String::new(),
        d if d > 0 => format!("+{d}d"),
        d => format!("−{}d", -d),
    }
}

/// The instant at `minutes` past local midnight of `date` in `tz`.
///
/// Inside a DST gap (e.g. 02:30 on spring-forward night) the clock reading doesn't exist,
/// so we use the first valid instant after it. In an overlap we take the earlier one.
pub fn instant_from_local(tz: Tz, date: NaiveDate, minutes: i64) -> DateTime<Utc> {
    let mut local = date.and_time(NaiveTime::MIN) + Duration::minutes(minutes);
    for _ in 0..8 {
        match tz.from_local_datetime(&local) {
            LocalResult::Single(t) | LocalResult::Ambiguous(t, _) => return t.with_timezone(&Utc),
            LocalResult::None => local += Duration::minutes(15),
        }
    }
    // No zone has a gap this long; treat the reading as UTC rather than panic.
    Utc.from_utc_datetime(&local)
}

/// Minutes past local midnight for `instant` in `tz`.
pub fn local_minutes(instant: DateTime<Utc>, tz: Tz) -> i64 {
    let t = instant.with_timezone(&tz).time();
    i64::from(t.hour()) * 60 + i64::from(t.minute())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::{America, Asia, Europe, Pacific};

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn london_dst_transitions() {
        // BST ends 2026-10-25 01:00 UTC.
        assert_eq!(offset_secs(utc(2026, 10, 25, 0, 30), Europe::London), 3600);
        assert_eq!(offset_secs(utc(2026, 10, 25, 1, 30), Europe::London), 0);
        // BST starts 2026-03-29 01:00 UTC.
        assert_eq!(offset_secs(utc(2026, 3, 29, 0, 30), Europe::London), 0);
        assert_eq!(offset_secs(utc(2026, 3, 29, 1, 30), Europe::London), 3600);
    }

    #[test]
    fn new_york_dst_transitions() {
        // EDT starts 2026-03-08 07:00 UTC, ends 2026-11-01 06:00 UTC.
        assert_eq!(
            offset_secs(utc(2026, 3, 8, 6, 59), America::New_York),
            -5 * 3600
        );
        assert_eq!(
            offset_secs(utc(2026, 3, 8, 7, 0), America::New_York),
            -4 * 3600
        );
        assert_eq!(
            offset_secs(utc(2026, 11, 1, 6, 0), America::New_York),
            -5 * 3600
        );
    }

    #[test]
    fn fractional_offsets() {
        let t = utc(2026, 1, 15, 12, 0);
        assert_eq!(format_offset(offset_secs(t, Asia::Kolkata)), "UTC+5:30");
        assert_eq!(format_offset(offset_secs(t, Asia::Kathmandu)), "UTC+5:45");
        assert_eq!(format_offset(offset_secs(t, Pacific::Chatham)), "UTC+13:45");
        assert_eq!(format_offset(offset_secs(t, America::Los_Angeles)), "UTC−8");
        assert_eq!(format_offset(0), "UTC");
    }

    #[test]
    fn day_delta_across_date_line() {
        // 20:00 in Los Angeles is already tomorrow in Auckland.
        let t = utc(2026, 10, 16, 3, 0);
        let akl = local_info(t, Pacific::Auckland, America::Los_Angeles);
        assert_eq!(akl.day_delta, 1);
        assert_eq!(format_day_delta(akl.day_delta), "+1d");
        let la = local_info(t, America::Los_Angeles, Pacific::Auckland);
        assert_eq!(la.day_delta, -1);
        assert_eq!(format_day_delta(la.day_delta), "−1d");
    }

    #[test]
    fn statuses() {
        let at = |h| status_at(NaiveTime::from_hms_opt(h, 0, 0).unwrap());
        assert_eq!(at(9), Status::Work);
        assert_eq!(at(16), Status::Work);
        assert_eq!(at(17), Status::Edge);
        assert_eq!(at(7), Status::Edge);
        assert_eq!(at(22), Status::Night);
        assert_eq!(at(3), Status::Night);
    }

    #[test]
    fn local_instant_round_trip_and_dst_gap() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 15).unwrap();
        let t = instant_from_local(Europe::Madrid, date, 14 * 60 + 30);
        assert_eq!(t, utc(2026, 10, 15, 12, 30));
        assert_eq!(local_minutes(t, Europe::Madrid), 14 * 60 + 30);

        // 02:30 doesn't exist in New York on 2026-03-08; we land on 03:00 EDT.
        let gap = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();
        let t = instant_from_local(America::New_York, gap, 2 * 60 + 30);
        assert_eq!(t, utc(2026, 3, 8, 7, 0));
    }
}
