# 0006 — Duration grammar and working-day length

- **Status:** accepted
- **Date:** 2026-10-29

## Context
CLAUDE.md lists the exact grammar of durations and lags as an open decision. SPEC.md uses
`"15d"`, `"3ed"` and `"4h"` without defining them, nor how long a working day is. Details in
`docs/format.md` §4 (decisions D1, D2).

## Decision
- Grammar: `[-]<number><unit>`; the number has at most two decimals; units `min h d w` (working
  time) and `emin eh ed ew` (elapsed time). No months or years. The minus sign is valid only in lags.
- Every duration is stored as a whole number of minutes; a literal that is not a whole number of
  minutes is an error (`E-FMT-04`).
- `1d` = `settings.schedule.hours_per_day` working hours (default 8) and `1w` =
  `days_per_week` working days (default 5), whatever the calendar says. A calendar whose working
  day has another length triggers `L-CAL-01` (warn). `1ed` = 24 h, `1ew` = 7 × 24 h.

## Consequences
Effort, duration and cost convert without depending on which calendar a task uses, as in MS
Project. Plans whose calendars do not match `hours_per_day` still schedule, with a warning.
