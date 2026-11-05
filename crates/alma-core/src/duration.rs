//! Duration and lag literals (`docs/format.md` §4, ADR 0006).

use std::fmt;

use thiserror::Error;

/// Minutes in one hour.
pub const MINUTES_PER_HOUR: i64 = 60;
/// Minutes in one elapsed day.
pub const MINUTES_PER_ELAPSED_DAY: i64 = 24 * MINUTES_PER_HOUR;

/// How working days and weeks convert to minutes (`[settings.schedule]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkUnits {
    /// Working minutes in `1d`.
    pub minutes_per_day: i64,
    /// Working days in `1w`.
    pub days_per_week: i64,
}

impl Default for WorkUnits {
    fn default() -> Self {
        Self {
            minutes_per_day: 8 * MINUTES_PER_HOUR,
            days_per_week: 5,
        }
    }
}

/// A resolved duration: whole minutes, measured either in working time or in elapsed time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Duration {
    /// Length in minutes; negative only for leads.
    pub minutes: i64,
    /// True for elapsed (calendar) time, false for working time.
    pub elapsed: bool,
}

impl Duration {
    /// Zero working time.
    pub const ZERO: Duration = Duration {
        minutes: 0,
        elapsed: false,
    };

    /// A working-time duration of `minutes`.
    pub fn working(minutes: i64) -> Self {
        Self {
            minutes,
            elapsed: false,
        }
    }

    /// An elapsed-time duration of `minutes`.
    pub fn elapsed(minutes: i64) -> Self {
        Self {
            minutes,
            elapsed: true,
        }
    }

    /// Formats the duration with the largest unit that represents it exactly.
    pub fn display(self, units: WorkUnits) -> String {
        let prefix = if self.elapsed { "e" } else { "" };
        let sign = if self.minutes < 0 { "-" } else { "" };
        let m = self.minutes.abs();
        let (day, week) = if self.elapsed {
            (MINUTES_PER_ELAPSED_DAY, 7 * MINUTES_PER_ELAPSED_DAY)
        } else {
            (
                units.minutes_per_day,
                units.minutes_per_day * units.days_per_week,
            )
        };
        for (size, unit) in [(week, "w"), (day, "d"), (MINUTES_PER_HOUR, "h")] {
            if size > 0 && m % size == 0 && m != 0 {
                return format!("{sign}{}{prefix}{unit}", m / size);
            }
        }
        if m == 0 {
            return format!("0{prefix}d");
        }
        // Fractional days with two decimals when exact, otherwise minutes.
        if day > 0 && (m * 100) % day == 0 {
            let hundredths = m * 100 / day;
            let frac = format!("{:02}", hundredths % 100);
            let frac = frac.trim_end_matches('0');
            return format!("{sign}{}.{frac}{prefix}d", hundredths / 100);
        }
        format!("{sign}{m}{prefix}min")
    }
}

/// Why a duration literal is invalid. Reported as `E-FMT-04`.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DurationError {
    /// The text does not match `[-]<number><unit>`.
    #[error(
        "`{0}` is not a duration; expected a number and a unit, like \"15d\", \"4h\" or \"3ed\""
    )]
    Syntax(String),
    /// Unknown unit.
    #[error(
        "unknown unit `{unit}` in `{text}`; use min, h, d, w (working) or emin, eh, ed, ew (elapsed)"
    )]
    Unit {
        /// The whole literal.
        text: String,
        /// The unit found.
        unit: String,
    },
    /// More than two decimals.
    #[error("`{0}` has more than two decimals")]
    Precision(String),
    /// Not a whole number of minutes.
    #[error("`{0}` is not a whole number of minutes")]
    NotWholeMinutes(String),
    /// A negative value where only a non-negative one is allowed.
    #[error("`{0}` is negative; only lags may be negative")]
    Negative(String),
    /// The value is too large.
    #[error("`{0}` is too large")]
    Overflow(String),
}

/// Parses a duration literal. `allow_negative` is true only for lags.
pub fn parse_duration(
    text: &str,
    units: WorkUnits,
    allow_negative: bool,
) -> Result<Duration, DurationError> {
    let syntax = || DurationError::Syntax(text.to_string());
    let (negative, body) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let split = body
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .ok_or_else(syntax)?;
    let (number, unit) = body.split_at(split);
    if number.is_empty() {
        return Err(syntax());
    }
    let (int_part, frac_part) = match number.split_once('.') {
        Some((i, f)) => (i, f),
        None => (number, ""),
    };
    if int_part.is_empty() || (number.contains('.') && frac_part.is_empty()) {
        return Err(syntax());
    }
    if frac_part.len() > 2 {
        return Err(DurationError::Precision(text.to_string()));
    }
    let overflow = || DurationError::Overflow(text.to_string());
    let int: i64 = int_part.parse().map_err(|_| overflow())?;
    let frac: i64 = if frac_part.is_empty() {
        0
    } else {
        let f: i64 = frac_part.parse().map_err(|_| syntax())?;
        if frac_part.len() == 1 { f * 10 } else { f }
    };
    let hundredths = int
        .checked_mul(100)
        .and_then(|v| v.checked_add(frac))
        .ok_or_else(overflow)?;

    let (elapsed, base) = match unit {
        "min" => (false, 1),
        "h" => (false, MINUTES_PER_HOUR),
        "d" => (false, units.minutes_per_day),
        "w" => (false, units.minutes_per_day * units.days_per_week),
        "emin" => (true, 1),
        "eh" => (true, MINUTES_PER_HOUR),
        "ed" => (true, MINUTES_PER_ELAPSED_DAY),
        "ew" => (true, 7 * MINUTES_PER_ELAPSED_DAY),
        _ => {
            return Err(DurationError::Unit {
                text: text.to_string(),
                unit: unit.to_string(),
            });
        }
    };
    let scaled = hundredths.checked_mul(base).ok_or_else(overflow)?;
    if scaled % 100 != 0 {
        return Err(DurationError::NotWholeMinutes(text.to_string()));
    }
    let mut minutes = scaled / 100;
    if negative {
        if !allow_negative && minutes != 0 {
            return Err(DurationError::Negative(text.to_string()));
        }
        minutes = -minutes;
    }
    Ok(Duration { minutes, elapsed })
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display(WorkUnits::default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> Result<Duration, DurationError> {
        parse_duration(s, WorkUnits::default(), true)
    }

    #[test]
    fn working_units_use_settings() {
        assert_eq!(p("15d"), Ok(Duration::working(15 * 480)));
        assert_eq!(p("4h"), Ok(Duration::working(240)));
        assert_eq!(p("2w"), Ok(Duration::working(10 * 480)));
        assert_eq!(p("30min"), Ok(Duration::working(30)));
        let units = WorkUnits {
            minutes_per_day: 9 * 60,
            days_per_week: 6,
        };
        assert_eq!(
            parse_duration("1w", units, false),
            Ok(Duration::working(6 * 540))
        );
    }

    #[test]
    fn elapsed_units() {
        assert_eq!(p("3ed"), Ok(Duration::elapsed(3 * 1440)));
        assert_eq!(p("1ew"), Ok(Duration::elapsed(7 * 1440)));
        assert_eq!(p("2eh"), Ok(Duration::elapsed(120)));
        assert_eq!(p("5emin"), Ok(Duration::elapsed(5)));
    }

    #[test]
    fn decimals_and_leads() {
        assert_eq!(p("0.5d"), Ok(Duration::working(240)));
        assert_eq!(p("1.25h"), Ok(Duration::working(75)));
        assert_eq!(p("-2d"), Ok(Duration::working(-960)));
        assert_eq!(p("0d"), Ok(Duration::ZERO));
    }

    #[test]
    fn rejects_bad_literals() {
        assert!(matches!(p("0.33h"), Err(DurationError::NotWholeMinutes(_))));
        assert!(matches!(p("1.234d"), Err(DurationError::Precision(_))));
        assert!(matches!(p("3mo"), Err(DurationError::Unit { .. })));
        assert!(matches!(p("d"), Err(DurationError::Syntax(_))));
        assert!(matches!(p("15"), Err(DurationError::Syntax(_))));
        assert!(matches!(p("1.d"), Err(DurationError::Syntax(_))));
        assert!(matches!(p(" 1d"), Err(DurationError::Syntax(_))));
        assert!(matches!(
            parse_duration("-1d", WorkUnits::default(), false),
            Err(DurationError::Negative(_))
        ));
    }

    #[test]
    fn display_round_trips() {
        let u = WorkUnits::default();
        for s in [
            "15d", "4h", "2w", "3ed", "-2d", "0.5d", "30min", "1ew", "0d",
        ] {
            let d = parse_duration(s, u, true).unwrap();
            assert_eq!(parse_duration(&d.display(u), u, true).unwrap(), d, "{s}");
        }
        assert_eq!(Duration::working(15 * 480).display(u), "3w");
        assert_eq!(Duration::working(240).display(u), "4h");
    }
}
