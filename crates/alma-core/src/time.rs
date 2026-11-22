//! Points in time as integer minutes (ADR 0005).

use jiff::civil::Date;

/// Minutes since 1970-01-01T00:00, civil time with no zone.
pub type Instant = i64;

/// Minutes in one day.
pub const DAY: i64 = 24 * 60;

const EPOCH: Date = Date::constant(1970, 1, 1);

/// Days since 1970-01-01.
pub fn day_number(date: Date) -> i64 {
    EPOCH
        .until(date)
        .map(|span| i64::from(span.get_days()))
        .unwrap_or(0)
}

/// The date of a day number.
pub fn date_of_day(day: i64) -> Date {
    let span = jiff::Span::new().days(day);
    EPOCH.checked_add(span).unwrap_or(EPOCH)
}

/// The instant at 00:00 of `date`.
pub fn start_of(date: Date) -> Instant {
    day_number(date) * DAY
}

/// The day number that contains `t`.
pub fn day_of(t: Instant) -> i64 {
    t.div_euclid(DAY)
}

/// Minute of the day of `t` (0..1440).
pub fn minute_of_day(t: Instant) -> i64 {
    t.rem_euclid(DAY)
}

/// Weekday of a day number, 0 = Monday … 6 = Sunday.
pub fn weekday(day: i64) -> u8 {
    // 1970-01-01 was a Thursday (3).
    (day + 3).rem_euclid(7) as u8
}

/// The date of `t`; a finish at exactly 00:00 belongs to the previous day.
pub fn finish_date(t: Instant) -> Date {
    date_of_day(day_of(t - 1))
}

/// The date of `t` read as a start.
pub fn start_date(t: Instant) -> Date {
    date_of_day(day_of(t))
}

/// Formats an instant as `YYYY-MM-DDTHH:MM`.
pub fn format_instant(t: Instant) -> String {
    let m = minute_of_day(t);
    format!("{}T{:02}:{:02}", date_of_day(day_of(t)), m / 60, m % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_numbers_and_weekdays() {
        assert_eq!(day_number(EPOCH), 0);
        let d = Date::constant(2027, 1, 4);
        assert_eq!(date_of_day(day_number(d)), d);
        assert_eq!(weekday(day_number(d)), 0, "2027-01-04 is a Monday");
        assert_eq!(weekday(day_number(Date::constant(1969, 12, 31))), 2);
    }

    #[test]
    fn finish_at_midnight_belongs_to_previous_day() {
        let d = Date::constant(2027, 1, 4);
        assert_eq!(finish_date(start_of(d) + DAY), d);
        assert_eq!(start_date(start_of(d) + 9 * 60), d);
        assert_eq!(format_instant(start_of(d) + 9 * 60), "2027-01-04T09:00");
    }
}
