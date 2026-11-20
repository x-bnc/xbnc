//! Time zones (IANA names such as `Europe/Madrid`), with daylight saving
//! time. The time zone database is built into the binary, so this works the
//! same on systems without `tzdata` (containers).

use jiff::tz::TimeZone;

/// A time zone chosen by a user; `UTC` when none is set.
#[derive(Debug, Clone)]
pub struct Zone {
    name: String,
    tz: TimeZone,
}

impl Default for Zone {
    fn default() -> Self {
        Zone {
            name: String::new(),
            tz: TimeZone::UTC,
        }
    }
}

impl Zone {
    /// Parses an IANA name. Empty means UTC; unknown names are an error.
    pub fn parse(name: &str) -> Result<Zone, String> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(Zone::default());
        }
        if name.len() > 64
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "/_-+".contains(c))
        {
            return Err(format!("unknown time zone {name:?}"));
        }
        let tz = jiff::tz::db()
            .get(name)
            .map_err(|_| format!("unknown time zone {name:?}"))?;
        Ok(Zone {
            name: name.to_string(),
            tz,
        })
    }

    /// Like `parse`, but falls back to UTC.
    pub fn parse_or_utc(name: &str) -> Zone {
        Zone::parse(name).unwrap_or_default()
    }

    pub fn name(&self) -> &str {
        if self.name.is_empty() {
            "UTC"
        } else {
            &self.name
        }
    }

    /// Offset from UTC in seconds at `ts_ms` (milliseconds since 1970).
    pub fn offset_secs(&self, ts_ms: i64) -> i32 {
        jiff::Timestamp::from_millisecond(ts_ms)
            .map(|t| self.tz.to_offset(t).seconds())
            .unwrap_or(0)
    }

    /// Local wall-clock parts at `ts_ms`: (year, month, day, hour, minute,
    /// second, weekday with Monday = 1).
    pub fn local(&self, ts_ms: i64) -> (i32, u8, u8, u8, u8, u8, u8) {
        let t = jiff::Timestamp::from_millisecond(ts_ms).unwrap_or(jiff::Timestamp::UNIX_EPOCH);
        let z = t.to_zoned(self.tz.clone());
        (
            z.year() as i32,
            z.month() as u8,
            z.day() as u8,
            z.hour() as u8,
            z.minute() as u8,
            z.second() as u8,
            z.weekday().to_monday_one_offset() as u8,
        )
    }

    /// UTC milliseconds of a local date and time. In the hour skipped when
    /// clocks go forward the later offset is used; in the repeated hour, the
    /// first one.
    pub fn to_utc_ms(&self, y: i32, mo: u8, d: u8, h: u8, mi: u8, s: u8) -> Option<i64> {
        let dt =
            jiff::civil::DateTime::new(y as i16, mo as i8, d as i8, h as i8, mi as i8, s as i8, 0)
                .ok()?;
        let z = self.tz.to_ambiguous_zoned(dt).compatible().ok()?;
        Some(z.timestamp().as_millisecond())
    }
}

/// True if `name` is empty or a known time zone.
pub fn valid(name: &str) -> bool {
    Zone::parse(name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn madrid_summer_and_winter() {
        let z = Zone::parse("Europe/Madrid").unwrap();
        // 2026-07-01 12:00 UTC → 14:00 local (CEST, +2).
        assert_eq!(z.offset_secs(1_782_907_200_000), 7200);
        // 2026-01-15 12:00 UTC → 13:00 local (CET, +1).
        assert_eq!(z.offset_secs(1_768_478_400_000), 3600);
        let (y, mo, d, h, mi, _, _) = z.local(1_782_907_200_000);
        assert_eq!((y, mo, d, h, mi), (2026, 7, 1, 14, 0));
        assert_eq!(z.to_utc_ms(2026, 7, 1, 14, 0, 0), Some(1_782_907_200_000));
        // The day clocks change: 2026-03-29 01:30 is +1, 03:30 is +2.
        let a = z.to_utc_ms(2026, 3, 29, 1, 30, 0).unwrap();
        let b = z.to_utc_ms(2026, 3, 29, 3, 30, 0).unwrap();
        assert_eq!(b - a, 3600 * 1000);
    }

    #[test]
    fn names() {
        assert!(valid("") && valid("America/New_York") && valid("UTC"));
        assert!(!valid("Mars/Olympus") && !valid("../etc/passwd"));
        assert_eq!(Zone::default().name(), "UTC");
        assert_eq!(Zone::parse_or_utc("nope").offset_secs(0), 0);
    }
}
