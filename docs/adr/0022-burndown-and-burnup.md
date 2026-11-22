# 0022 — Burndown and burnup charts

- **Status:** accepted
- **Date:** 2026-11-09

## Context
The knowledge notes (scheduling §5.7) ask for two charts drawn from data ADR 0021 already
records (points, iterations, finish dates):
- a sprint burndown: points left against the time left, with an ideal line;
- a release or cadence burnup: points done against total scope, which shows scope growth.

Both should go through the same pipeline as the S-curve, so they print and render the same
way.

## Decision
- **`alma export --chart burndown --iteration s03`:**
  - one point per working day of the iteration, on the cadence calendar;
  - the committed points (work planned in that iteration) are the origin;
  - the ideal line falls evenly to 0 by the last working day;
  - the actual line is committed minus the points of work whose `actual_finish` is on or before
    that day, drawn only through the status date.
- **`alma export --chart burnup [--cadence ID]`:**
  - one point per iteration of the cadence (the only cadence by default);
  - the cumulative committed points (`planned`);
  - the cumulative finished points for iterations that have started by the status date
    (`done`);
  - total scope as a horizontal line: all points of the cadence, plus the backlog when the plan
    has a single cadence.
- MCP `render` and `export` take the same `chart`, `iteration` and `cadence` arguments.
- The data is computed in `alma_ops::agile::{burndown, burnup}`. `alma_layout::charts::curve`
  gains `Series::origin`, the value at the start of the first period: 0 for cumulative
  curves, the committed points for a burndown.
- Charts with few periods widen the plot until the title and the legend fit side by side.
  This applies to every chart.

## Consequences
- An agent can look at a sprint's progress and a team's trend as images, from the same data as
  `alma iter ls`.
- The daily burndown has no history of re-estimates: points are taken as they are now, and a
  story counts as done on its `actual_finish` day.
- Tests:
  - hand-worked data in `crates/alma-ops/tests/agile.rs` (an ideal of 1.3 points a day, a
    drop of 8 points on day eight, the burnup values and the scope);
  - a layout test that the title and legend fit;
  - CLI and MCP tests.
