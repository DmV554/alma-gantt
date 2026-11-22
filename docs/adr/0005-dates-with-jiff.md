# 0005 — Civil dates with `jiff`; working time as integer minutes

- **Status:** accepted
- **Date:** 2026-10-29

## Context
SPEC.md §7.1 and §10.3 left `chrono` vs `jiff` open. The engine only needs *civil* dates (no time
zones): parse and print ISO dates, weekday of a date, add or subtract days. All scheduling
arithmetic is in working time on calendars, which no date library provides. `chrono` 0.4 is
ubiquitous but its API mixes naive and zoned types; `jiff` (0.2, by the author of `regex`, MIT or
Unlicense) has a clean `civil::Date` type and is actively maintained, but is pre-1.0.

## Decision
- `alma-core` uses `jiff::civil::Date` for dates, with `default-features = false` and only the
  `std` feature, so no time-zone database is bundled and the clock is never read.
- Points in time inside the engine are `Instant`: an `i64` number of minutes since
  1970-01-01T00:00 (civil, no zone). Durations are `i64` minutes. Working-time arithmetic lives
  in `alma-sched` and converts to dates only at the edges.
- The version is pinned exactly in the workspace manifest. A minor bump of `jiff` is a reviewed
  change, because pre-1.0 minors may break.

## Consequences
`jiff` appears only at the boundaries (model dates, parsing, output), so replacing it later is
cheap. Instants as integers make the CPM passes simple, fast and deterministic.
