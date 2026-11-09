//! Working-time arithmetic on a calendar (SPEC.md §7.1, ADR 0005, ADR 0006).
//!
//! Instants are minutes. A *start position* lies inside a working interval `[a, b)`; a *finish
//! position* lies inside `(a, b]`. Adding working time from a start lands on a finish, and
//! subtracting working time from a finish lands on a start, so a one-day task that starts Monday
//! 09:00 finishes Monday 18:00, not Tuesday 09:00.

use std::collections::HashMap;

use alma_core::model::{Calendar, Interval};
use alma_core::time::{DAY, Instant, day_number, day_of, minute_of_day, weekday};

/// Search limit for the next or previous working minute, in days (about 200 years). Calendars
/// with no working time are rejected by the parser (`E-CAL-01`); the limit only keeps a broken
/// calendar from looping forever.
const SEARCH_DAYS: i64 = 200 * 366;

/// A calendar compiled for fast lookups.
#[derive(Debug, Clone)]
pub struct WorkCalendar {
    workdays: [bool; 7],
    hours: Vec<Interval>,
    /// Day number → index into `exception_hours`; later exceptions override earlier ones.
    exceptions: HashMap<i64, usize>,
    exception_hours: Vec<Vec<Interval>>,
    /// Exception names, parallel to `exception_hours`.
    exception_names: Vec<String>,
}

impl WorkCalendar {
    /// Compiles a calendar.
    pub fn new(cal: &Calendar) -> Self {
        let mut exceptions = HashMap::new();
        let mut exception_hours = Vec::new();
        let mut exception_names = Vec::new();
        for ex in &cal.exceptions {
            let idx = exception_hours.len();
            exception_hours.push(if ex.working {
                ex.hours.clone().unwrap_or_else(|| cal.hours.clone())
            } else {
                Vec::new()
            });
            exception_names.push(ex.name.clone());
            for day in day_number(ex.from)..=day_number(ex.to) {
                exceptions.insert(day, idx);
            }
        }
        Self {
            workdays: cal.workdays,
            hours: cal.hours.clone(),
            exceptions,
            exception_hours,
            exception_names,
        }
    }

    /// The working time both calendars share: a task worked by a resource with its own
    /// calendar (knowledge notes, resources §1.3). Exceptions of either calendar become
    /// exceptions of the result, keeping their names.
    pub fn intersect(&self, other: &WorkCalendar) -> WorkCalendar {
        let mut workdays = [false; 7];
        for (d, w) in workdays.iter_mut().enumerate() {
            *w = self.workdays[d] && other.workdays[d];
        }
        let mut days: Vec<i64> = self
            .exceptions
            .keys()
            .chain(other.exceptions.keys())
            .copied()
            .collect();
        days.sort_unstable();
        days.dedup();
        let mut exceptions = HashMap::new();
        let mut exception_hours = Vec::new();
        let mut exception_names = Vec::new();
        for day in days {
            exceptions.insert(day, exception_hours.len());
            exception_hours.push(overlap(self.intervals(day), other.intervals(day)));
            exception_names.push(
                self.exception_on(day)
                    .or_else(|| other.exception_on(day))
                    .unwrap_or_default()
                    .to_string(),
            );
        }
        WorkCalendar {
            workdays,
            hours: overlap(&self.hours, &other.hours),
            exceptions,
            exception_hours,
            exception_names,
        }
    }

    /// True when the calendar has any working time at all.
    pub fn has_work(&self) -> bool {
        (!self.hours.is_empty() && self.workdays.iter().any(|&w| w))
            || self.exception_hours.iter().any(|h| !h.is_empty())
    }

    /// Working intervals of a day.
    pub fn intervals(&self, day: i64) -> &[Interval] {
        if let Some(&idx) = self.exceptions.get(&day) {
            return &self.exception_hours[idx];
        }
        if self.workdays[usize::from(weekday(day))] {
            &self.hours
        } else {
            &[]
        }
    }

    /// The name of the exception that applies on `day`, if any.
    pub fn exception_on(&self, day: i64) -> Option<&str> {
        self.exceptions
            .get(&day)
            .map(|&i| self.exception_names[i].as_str())
    }

    /// The earliest start position at or after `t`.
    pub fn next_start(&self, t: Instant) -> Instant {
        let first = day_of(t);
        for day in first..first + SEARCH_DAYS {
            let from = if day == first { minute_of_day(t) } else { 0 };
            for &(a, b) in self.intervals(day) {
                if from < i64::from(b) {
                    return day * DAY + from.max(i64::from(a));
                }
            }
        }
        t
    }

    /// The latest finish position at or before `t`.
    pub fn prev_end(&self, t: Instant) -> Instant {
        // A finish at 00:00 belongs to the previous day.
        let last = day_of(t - 1);
        for day in (last - SEARCH_DAYS..=last).rev() {
            let to = if day == last { t - day * DAY } else { DAY };
            for &(a, b) in self.intervals(day).iter().rev() {
                if to > i64::from(a) {
                    return day * DAY + to.min(i64::from(b));
                }
            }
        }
        t
    }

    /// Adds `minutes` of working time to `t` (subtracts when negative). Zero returns `t`.
    pub fn add(&self, t: Instant, minutes: i64) -> Instant {
        if minutes < 0 {
            return self.sub(t, -minutes);
        }
        if minutes == 0 {
            return t;
        }
        let mut rem = minutes;
        let first = day_of(t);
        for day in first..first + SEARCH_DAYS {
            let from = if day == first { minute_of_day(t) } else { 0 };
            for &(a, b) in self.intervals(day) {
                let (a, b) = (i64::from(a), i64::from(b));
                if from >= b {
                    continue;
                }
                let begin = from.max(a);
                let avail = b - begin;
                if rem <= avail {
                    return day * DAY + begin + rem;
                }
                rem -= avail;
            }
        }
        t
    }

    /// Subtracts `minutes` of working time from `t` (adds when negative). Zero returns `t`.
    pub fn sub(&self, t: Instant, minutes: i64) -> Instant {
        if minutes < 0 {
            return self.add(t, -minutes);
        }
        if minutes == 0 {
            return t;
        }
        let mut rem = minutes;
        let last = day_of(t - 1);
        for day in (last - SEARCH_DAYS..=last).rev() {
            let to = if day == last { t - day * DAY } else { DAY };
            for &(a, b) in self.intervals(day).iter().rev() {
                let (a, b) = (i64::from(a), i64::from(b));
                if to <= a {
                    continue;
                }
                let end = to.min(b);
                let avail = end - a;
                if rem <= avail {
                    return day * DAY + end - rem;
                }
                rem -= avail;
            }
        }
        t
    }

    /// Working minutes from `a` to `b`; negative when `b < a`.
    pub fn between(&self, a: Instant, b: Instant) -> i64 {
        if b < a {
            return -self.between(b, a);
        }
        let mut total = 0;
        let (first, last) = (day_of(a), day_of(b));
        for day in first..=last {
            let lo = if day == first { minute_of_day(a) } else { 0 };
            let hi = if day == last { minute_of_day(b) } else { DAY };
            for &(x, y) in self.intervals(day) {
                let (x, y) = (i64::from(x).max(lo), i64::from(y).min(hi));
                if y > x {
                    total += y - x;
                }
            }
        }
        total
    }
}

/// The intersection of two sorted, non-overlapping interval lists.
fn overlap(a: &[Interval], b: &[Interval]) -> Vec<Interval> {
    let mut out = Vec::new();
    for &(a0, a1) in a {
        for &(b0, b1) in b {
            let (lo, hi) = (a0.max(b0), a1.min(b1));
            if lo < hi {
                out.push((lo, hi));
            }
        }
    }
    out.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alma_core::model::CalendarException;
    use alma_core::time::start_of;
    use jiff::civil::Date;

    fn at(y: i16, m: i8, d: i8, h: i64, min: i64) -> Instant {
        start_of(Date::new(y, m, d).unwrap()) + h * 60 + min
    }

    fn std_cal() -> WorkCalendar {
        WorkCalendar::new(&Calendar::standard())
    }

    #[test]
    fn intersection_keeps_shared_days_hours_and_exceptions() {
        // A part-timer: Monday to Wednesday, mornings, with a day off on Tuesday 5 January.
        let mut part = Calendar::standard();
        part.workdays = [true, true, true, false, false, false, false];
        part.hours = vec![(8 * 60, 12 * 60)];
        part.exceptions.push(CalendarException {
            name: "Day off".into(),
            from: Date::constant(2027, 1, 5),
            to: Date::constant(2027, 1, 5),
            working: false,
            hours: None,
        });
        let both = std_cal().intersect(&WorkCalendar::new(&part));
        assert!(both.has_work());
        let monday = day_number(Date::constant(2027, 1, 4));
        assert_eq!(both.intervals(monday), &[(9 * 60, 12 * 60)]);
        assert!(both.intervals(monday + 1).is_empty(), "day off");
        assert_eq!(both.exception_on(monday + 1), Some("Day off"));
        assert!(both.intervals(monday + 3).is_empty(), "Thursday");
        // 6 h of work: Monday 3 h, (Tuesday off), Wednesday 3 h.
        assert_eq!(both.add(at(2027, 1, 4, 9, 0), 360), at(2027, 1, 6, 12, 0));
        let mut nights = Calendar::standard();
        nights.hours = vec![(20 * 60, 23 * 60)];
        assert!(!std_cal().intersect(&WorkCalendar::new(&nights)).has_work());
    }

    #[test]
    fn one_day_task_finishes_same_day() {
        let c = std_cal();
        // 2027-01-04 is a Monday.
        let start = c.next_start(at(2027, 1, 4, 0, 0));
        assert_eq!(start, at(2027, 1, 4, 9, 0));
        assert_eq!(c.add(start, 480), at(2027, 1, 4, 18, 0));
        assert_eq!(c.sub(at(2027, 1, 4, 18, 0), 480), start);
    }

    #[test]
    fn skips_lunch_and_weekends() {
        let c = std_cal();
        assert_eq!(c.add(at(2027, 1, 4, 12, 0), 120), at(2027, 1, 4, 15, 0));
        // Friday 17:00 + 2h = Monday 10:00.
        assert_eq!(c.add(at(2027, 1, 8, 17, 0), 120), at(2027, 1, 11, 10, 0));
        assert_eq!(c.sub(at(2027, 1, 11, 10, 0), 120), at(2027, 1, 8, 17, 0));
        assert_eq!(c.next_start(at(2027, 1, 8, 18, 0)), at(2027, 1, 11, 9, 0));
        assert_eq!(c.prev_end(at(2027, 1, 11, 9, 0)), at(2027, 1, 8, 18, 0));
        assert_eq!(c.prev_end(at(2027, 1, 11, 0, 0)), at(2027, 1, 8, 18, 0));
    }

    #[test]
    fn between_counts_working_minutes() {
        let c = std_cal();
        assert_eq!(
            c.between(at(2027, 1, 4, 9, 0), at(2027, 1, 11, 9, 0)),
            5 * 480
        );
        assert_eq!(
            c.between(at(2027, 1, 11, 9, 0), at(2027, 1, 4, 9, 0)),
            -5 * 480
        );
        assert_eq!(c.between(at(2027, 1, 8, 18, 0), at(2027, 1, 11, 9, 0)), 0);
    }

    #[test]
    fn exceptions_override_weekdays() {
        let mut cal = Calendar::standard();
        cal.exceptions.push(CalendarException {
            name: "Holiday".into(),
            from: Date::constant(2027, 1, 5),
            to: Date::constant(2027, 1, 5),
            working: false,
            hours: None,
        });
        cal.exceptions.push(CalendarException {
            name: "Saturday shift".into(),
            from: Date::constant(2027, 1, 9),
            to: Date::constant(2027, 1, 9),
            working: true,
            hours: Some(vec![(9 * 60, 13 * 60)]),
        });
        let c = WorkCalendar::new(&cal);
        // Monday 18:00 + 1d skips Tuesday.
        assert_eq!(c.add(at(2027, 1, 4, 18, 0), 480), at(2027, 1, 6, 18, 0));
        // Friday 18:00 + 4h lands on the Saturday shift.
        assert_eq!(c.add(at(2027, 1, 8, 18, 0), 240), at(2027, 1, 9, 13, 0));
        assert_eq!(
            c.exception_on(day_number(Date::constant(2027, 1, 5))),
            Some("Holiday")
        );
    }
}
