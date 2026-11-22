# 0011 — Resource leveling as a scheduling setting, with a serial schedule generation scheme

- **Status:** accepted
- **Date:** 2026-11-08

## Context
SPEC.md §7.4 and the knowledge notes (resources §1.5–§1.6) ask for over-allocation checks
(`L-RES-01`), leveling and smoothing as separate operations, an audit trail per delayed task,
and a report of how the critical path changed. ADR 0003 forbids writing computed values to the
plan file, and SPEC §8a requires that the same file and commands give the same bytes and output.

Desktop tools usually store a per-task "leveling delay" in the file. That is a computed value:
it goes stale as soon as a duration changes, and nobody can review it in a diff.

## Decision
- **Leveling is a setting, not stored output.** `[settings.schedule] leveling = "off" |
  "smoothing" | "leveling"` (default `off`). When it is on, every schedule is leveled: the
  delays are recomputed from the plan each time and never written. `alma level [--mode]` (batch
  op `level`) only changes the setting; `settings.set` changes any `[settings.schedule]` key.
- **Algorithm:** a serial schedule generation scheme over the logic network. Tasks are placed
  one by one when all their predecessors are placed, choosing among the ready tasks by least
  total float, then earliest late finish, then source order (all from the unleveled schedule).
  A task starts at the earliest time logic allows; while one of its resources would go above
  `max_units`, it moves to the end of the booking in the way that ends first. The load only
  drops when a booking ends, so the search never skips a feasible start. It is deterministic.
  The priority rule is fixed for now; SPEC's configurable rule comes later.
- **What it moves:** leaf tasks with `work`/`equipment` assignments and a duration.
  Milestones, level-of-effort tasks and tasks with `MSO`/`MFO` stay put. The first two demand
  nothing during leveling, and fixed tasks still book their resources. `material` and `cost`
  resources have no capacity over time and never over-allocate.
- **Smoothing** (our reading of PMBOK's "within free and total float"): a task may move only
  up to its free float in the unleveled schedule. Then no successor moves and the finish stays.
  A task that does not fit stays at its logic date and is listed as unresolved (`L-RES-02`).
- **Resource-constrained float.** After leveling, each delayed task gets an implicit
  finish-to-start link from the task that held its resource, used in the backward pass only.
  Float and the critical path then reflect resources: the task that blocks a critical task is
  critical too. The forward dates do not change, because the delayed start already satisfies
  the link. This is our interpretation; it is the resource-critical-path view many planners
  use, and the plain logic-only float would hide the blocking task.
- **Explainability:** a delayed task's driver is `Leveling { resource, by }`; `explain` prints
  "waits for `dev`, busy with `yard-sw` until …" and continues the chain through `by`.
- **Over-allocation** is checked on the final schedule, on working time, per resource
  (F-COST-03/F-COST-04). `L-RES-01` is a warning when leveling is off and an error when it is on
  (only unfixable cases are left: one assignment above capacity, or fixed tasks overlapping).
  In smoothing mode leftovers are `L-RES-02` (warning).
- **Reports:** a mutation that changes the mode returns `leveling` (mode, finish before and
  after, each delayed task with the resource it waited for and the task holding it, unresolved
  tasks, over-allocation left). Every mutation now reports `critical_path_changed` when the
  critical set changed. `alma res ls` and `alma res usage <id> --period day|week|month` give
  demand vs capacity per period. The same queries are MCP tools: `list_resources` and
  `resource_usage`.

## Consequences
Plans stay diffable: turning leveling on is a one-line change, and its effect is recomputed
and explained. A plan with leveling on can shift a lot after a small edit. That is the honest
outcome of the heuristic, and the mutation report shows it. Leveling costs O(n · b) per
schedule, where b is the number of bookings per resource. That is fine for M2; the 5,000-task
budget in SPEC §12 applies to logic-only scheduling. Resource calendars are not yet intersected
with task calendars: demand uses the task calendar, and capacity in histograms uses the
resource calendar. Time-phased availability and other priority rules are later work.
Property tests cover these guarantees:
- leveling leaves no over-allocation it could fix;
- links still hold, and no task starts earlier than logic allows;
- smoothing never moves the finish;
- leftovers are always reported;
- the result is deterministic.
