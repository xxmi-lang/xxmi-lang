//! Structured logs, one JSON object per line on stdout (Railway indexes them as fields).
//! Callers pass sizes, timings and counts only: never the text submitted to `lint_ini`.

use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Map, Value};

/// `2026-11-18T14:00:18.123Z`, like `Date.prototype.toISOString`.
pub fn iso_timestamp() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let millis = now.as_millis() as i64;
    let (days, ms_of_day) = (millis.div_euclid(86_400_000), millis.rem_euclid(86_400_000));
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        ms_of_day / 3_600_000,
        ms_of_day / 60_000 % 60,
        ms_of_day / 1000 % 60,
        ms_of_day % 1000
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Error => "error",
        }
    }
}

/// Writes to stdout unless `quiet` (tests).
#[derive(Clone, Copy, Debug, Default)]
pub struct Logger {
    pub quiet: bool,
}

impl Logger {
    pub fn log(&self, level: Level, msg: &str, fields: Value) {
        if self.quiet {
            return;
        }

        let mut line = Map::new();
        line.insert("ts".into(), Value::String(iso_timestamp()));
        line.insert("level".into(), level.as_str().into());
        line.insert("msg".into(), msg.into());
        if let Value::Object(extra) = fields {
            line.extend(extra);
        }

        let mut text = Value::Object(line).to_string();
        text.push('\n');
        // A closed stdout must not take the server down.
        let _ = std::io::stdout().lock().write_all(text.as_bytes());
    }

    pub fn info(&self, msg: &str, fields: Value) {
        self.log(Level::Info, msg, fields);
    }

    pub fn warn(&self, msg: &str, fields: Value) {
        self.log(Level::Warn, msg, fields);
    }

    pub fn error(&self, msg: &str, fields: Value) {
        self.log(Level::Error, msg, fields);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_round_trip_known_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_453), (2025, 12, 31));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
    }

    #[test]
    fn timestamps_look_like_iso_strings() {
        let ts = iso_timestamp();
        assert_eq!(ts.len(), 24);
        assert!(ts.ends_with('Z') && ts.as_bytes()[10] == b'T');
    }
}
