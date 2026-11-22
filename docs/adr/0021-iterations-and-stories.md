# 0021 — Iterations, stories and velocity

- **Status:** accepted
- **Date:** 2026-11-09

## Context
SPEC.md §4 (point 5) and M4 want hybrid agile work as real objects, not fake tasks. M0 already
has `[[cadence]]`: it generates iterations and draws sprint bands, but no work can be planned
in them. The knowledge notes (scheduling §5.3–5.10, Annex A `L-AGL-*`; resources §5.8,
F-COST-29) ask for four things:
- work in an iteration, with points;
- stories that are not scheduled one by one;
- velocity per team, never summed across teams, with a forecast range;
- capacity checks.

## Decision
**Data** (`docs/format.md` §6.7, §6.14):
- `[[cadence]]` keys move to tier A and gain `capacity` (points per iteration).
- Tasks gain `iteration` (a generated iteration id, leaf tasks only, `E-REF-03` when unknown)
  and `points` (≥ 0).
- `kind = "story"`: a backlog item with no duration, effort, estimate, constraint, links or
  remaining work. Nothing may depend on a story (`E-FMT-03`). Epics are summaries.

**Iterations** (`alma_sched::agile`):
- A cadence's iterations run back to back. Each starts at the first working minute after the
  previous one ends and lasts `length` on the cadence calendar.
- A forecast may extend the cadence past `count`.

**Scheduling:**
- A story with an `iteration` is in the logic network, fixed to that window as a must-start-on
  task would be (`Driver::Iteration`):
  - its duration is the window;
  - links on its container reach it, so a release milestone after the container waits for the
    last planned iteration;
  - backward-pass float shows whether that iteration is late for a deadline.
- A story with no iteration is backlog (`Driver::Backlog`):
  - it is out of the network and out of roll-ups;
  - it is never drawn.
- The Gantt chart never draws stories (scheduling §5.10). Their container spans their
  iterations.
- A story's progress is its `actual_finish` (done) or its `actual_start`. Earned value counts a
  finished story as complete.
- An ordinary task with an `iteration` is scheduled by its own logic. It is only checked
  against the window (`L-AGL-04`).

**Velocity and forecast** (`alma iter ls`, MCP `iterations`):
- Per iteration: committed points, finished points (work with `actual_finish`), capacity, a
  state (`done`, `current` or `planned`, against the status date) and SPI = finished /
  committed for ended iterations (F-COST-29).
- Per cadence:
  - the velocity of each ended iteration;
  - points left: unfinished work in its iterations, plus the backlog when the plan has a single
    cadence. With several cadences the backlog belongs to no team and is reported apart.
  - a forecast: iterations needed = ceil(left / mean velocity), counted from the first
    iteration not ended, with the end date. The best and worst ended iterations give the range
    the notes ask for. Velocity is never summed across cadences.

**Lint:**
- `L-AGL-04` (warn): a task planned in an iteration but scheduled outside it.
- `L-AGL-06` (info): an iteration shorter than a week or longer than a month.
- `L-AGL-07` (warn): points committed to an iteration above the cadence `capacity`.

Iterations are generated, so `L-AGL-01..03` (overlaps, unequal lengths, gaps) cannot happen.
`L-AGL-05` waits for `[[release]]` (still reserved). Agile earned value for a container
(accepted points over planned points), `agile_drives` and per-iteration overrides are left for
later.

**Commands:**
- `alma iter add --capacity N`;
- `alma iter set ID KEY=VALUE` (op `cadence.set`);
- `alma iter ls [--cadence ID]`;
- `alma task add … --kind story --field iteration=s03 --field points=5`.

## Consequences
- Sprints hold real work: committed points, velocity and a forecast an agent can act on.
- Predictive logic still connects to agile containers through their planned iterations.
- Moving a story to another sprint is a one-key edit.
- Tests:
  - a hand-worked plan in `crates/alma-ops/tests/agile.rs` (velocity 13 and 8, forecast of 4
    iterations with a range of 3 to 5, story windows, the backlog left out, a release waiting
    for the last iteration, `L-AGL-04/06/07`, story validation);
  - CLI contract and MCP tests.
