//! Scheduling engine: working-time arithmetic, CPM, constraints, leveling and smoothing,
//! costs, earned value and diagnostics. Pure functions over `alma-core` (SPEC.md §7).

pub mod calendar;
pub mod cpm;

pub use calendar::WorkCalendar;
pub use cpm::{Driver, PlanIndex, Schedule, TaskDates, schedule};
pub mod lint;

pub use lint::{RULES, Rule, lint};
