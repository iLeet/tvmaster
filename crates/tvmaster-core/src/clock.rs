//! Time-zone aware helpers. All schedule maths is done in the channel's local
//! time (so "8pm" stays 8pm across daylight saving changes) and converted to
//! UTC epoch milliseconds for storage.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zone(Tz);

impl Zone {
    pub fn parse(name: &str) -> Option<Zone> {
        name.parse::<Tz>().ok().map(Zone)
    }

    pub fn utc() -> Zone {
        Zone(chrono_tz::UTC)
    }

    /// Local wall-clock time at the UTC instant `ms`.
    pub fn local(&self, ms: i64) -> NaiveDateTime {
        DateTime::<Utc>::from_timestamp_millis(ms)
            .unwrap_or_default()
            .with_timezone(&self.0)
            .naive_local()
    }

    /// Offset (local minus UTC) in effect at `ms`.
    pub fn offset_ms(&self, ms: i64) -> i64 {
        self.local(ms).and_utc().timestamp_millis() - ms
    }

    /// UTC instant for a local wall-clock time. Ambiguous times (clocks going
    /// back) take the first occurrence; non-existent times (clocks going
    /// forward) are shifted an hour later.
    pub fn to_utc_ms(&self, local: NaiveDateTime) -> i64 {
        match self.0.from_local_datetime(&local) {
            LocalResult::Single(d) => d.timestamp_millis(),
            LocalResult::Ambiguous(a, _) => a.timestamp_millis(),
            LocalResult::None => self
                .0
                .from_local_datetime(&(local + Duration::hours(1)))
                .earliest()
                .map(|d| d.timestamp_millis())
                .unwrap_or_else(|| local.and_utc().timestamp_millis()),
        }
    }

    /// The next grid boundary at or after `ms`, aligned in local time.
    pub fn next_grid(&self, ms: i64, grid_ms: i64) -> i64 {
        let off = self.offset_ms(ms);
        let local = ms + off;
        let aligned = (local + grid_ms - 1).div_euclid(grid_ms) * grid_ms;
        aligned - off
    }

    /// The latest grid boundary at or before `ms`, aligned in local time.
    pub fn floor_grid(&self, ms: i64, grid_ms: i64) -> i64 {
        let off = self.offset_ms(ms);
        (ms + off).div_euclid(grid_ms) * grid_ms - off
    }

    /// Whole weeks (Monday-based) between the week containing `epoch_ms` and
    /// the week containing `ms`, in local time.
    pub fn week_index(&self, epoch_ms: i64, ms: i64) -> i64 {
        let epoch_date = self.local(epoch_ms).date();
        let monday =
            epoch_date - Duration::days(epoch_date.weekday().num_days_from_monday() as i64);
        (self.local(ms).date() - monday).num_days().div_euclid(7)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;
    const MIN30: i64 = 30 * 60_000;

    fn toronto() -> Zone {
        Zone::parse("America/Toronto").unwrap()
    }

    #[test]
    fn rejects_unknown_zone() {
        assert!(Zone::parse("Mars/Olympus").is_none());
    }

    #[test]
    fn grid_aligns_in_local_time() {
        let ms = 1_794_859_800_000 + 10 * 60_000;
        let z = Zone::utc();
        assert_eq!(z.next_grid(ms, MIN30) % MIN30, 0);
        assert!(z.next_grid(ms, MIN30) >= ms);
        assert!(z.next_grid(ms, MIN30) - ms < MIN30);
        assert_eq!(z.floor_grid(ms, MIN30) % MIN30, 0);
        // already aligned stays put
        let aligned = z.next_grid(ms, MIN30);
        assert_eq!(z.next_grid(aligned, MIN30), aligned);
    }

    #[test]
    fn half_hour_zone_hourly_grid_aligns_to_local_hour() {
        let kolkata = Zone::parse("Asia/Kolkata").unwrap(); // UTC+5:30
        let g = kolkata.next_grid(1_000_000_000_000, HOUR);
        let local = kolkata.local(g);
        assert_eq!(chrono::Timelike::minute(&local), 0);
    }

    #[test]
    fn local_to_utc_roundtrip_across_dst() {
        let z = toronto();
        // 2026-11-01 is the fall-back day in Toronto.
        let before = NaiveDateTime::parse_from_str("2026-10-31 20:00", "%Y-%m-%d %H:%M").unwrap();
        let after = NaiveDateTime::parse_from_str("2026-11-02 20:00", "%Y-%m-%d %H:%M").unwrap();
        let (b, a) = (z.to_utc_ms(before), z.to_utc_ms(after));
        assert_eq!(z.local(b), before);
        assert_eq!(z.local(a), after);
        // 8pm local is 1 hour later in UTC after clocks go back.
        let day = 24 * HOUR;
        assert_eq!(a - b - 2 * day, HOUR);
    }

    #[test]
    fn spring_forward_gap_is_shifted() {
        let z = toronto();
        // 2026-03-08 02:30 does not exist in Toronto.
        let gap = NaiveDateTime::parse_from_str("2026-03-08 02:30", "%Y-%m-%d %H:%M").unwrap();
        let ms = z.to_utc_ms(gap);
        assert_eq!(chrono::Timelike::hour(&z.local(ms)), 3);
    }

    #[test]
    fn week_index_counts_monday_weeks() {
        let z = Zone::utc();
        // Sunday 2026-11-15 belongs to the week of Monday 2026-11-09
        let epoch = z.to_utc_ms(
            NaiveDateTime::parse_from_str("2026-11-15 00:00", "%Y-%m-%d %H:%M").unwrap(),
        );
        assert_eq!(z.week_index(epoch, epoch), 0);
        // the next day (Monday) starts week 1
        assert_eq!(z.week_index(epoch, epoch + 24 * HOUR), 1);
        assert_eq!(z.week_index(epoch, epoch + 24 * HOUR + 6 * 24 * HOUR), 1);
        assert_eq!(z.week_index(epoch, epoch + 24 * HOUR + 7 * 24 * HOUR), 2);
    }
}
