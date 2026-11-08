//! Scheduling engine: working-time arithmetic, CPM, constraints, leveling and smoothing,
//! costs, earned value and diagnostics. Pure functions over `alma-core` (SPEC.md §7).

pub mod calendar;
pub mod cpm;
mod level;
pub mod lint;
pub mod resources;

pub use calendar::WorkCalendar;
pub use cpm::{Delay, Driver, LevelingReport, PlanIndex, Schedule, TaskDates, schedule};
pub use lint::{RULES, Rule, lint};
