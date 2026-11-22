# 0018 — Progress and the status date

- **Status:** accepted
- **Date:** 2026-11-09

## Context
M4 starts with schedule control (SPEC.md §5 M4; knowledge notes, scheduling §4.14–4.16 and
resources §5.7). The progress keys (`status_date`, `actual_start`, `actual_finish`, `remaining`,
`progress_logic`) were reserved. Once actual dates exist, a schedule needs answers to these
questions:
- what the dates of work already done are;
- where unfinished work goes;
- how a task that started before its predecessors finished (out of sequence) is scheduled;
- what float means for work in progress.

Earned value (`physical_pct`, `ev_method`, actual cost) builds on top of this. It is a separate
decision (ADR 0019).

## Decision
**Data.**
- `[project] status_date` is the data date. Progress is reported through the end of that day.
  The data instant is the first working minute after it, on each task's calendar.
- On leaf tasks that are not level of effort:
  - `actual_start` (date): the first working minute of that day;
  - `actual_finish` (date): the last working minute of that day;
  - `remaining` (duration).
- A milestone is reached with either date. It sits at the first working minute of that day,
  as with `MSO`.
- The parser rejects the following with `E-FMT-03`:
  - progress on a summary or on a level-of-effort task;
  - a finish before its start;
  - `actual_finish` or `remaining` without `actual_start`;
  - remaining work on a finished task.

**Scheduling** (`cpm::schedule`, one place used by the forward pass and leveling alike).
- *Complete:* both dates are actual. The task has no late dates of its own (late = early) and
  no float. It is never critical.
- *In progress:*
  - The start is actual.
  - Remaining work defaults to the duration minus the working time from the actual start to
    the end of the status date, floored at 0. An explicit `remaining` replaces that default.
  - The remaining work resumes no earlier than the data instant.
  - `progress_logic = "retained_logic"` (the default): the remaining work also honors every
    link as if it were a task of its own. For FF and SF links, the remaining duration counts,
    not the full one.
  - `progress_logic = "progress_override"`: once started, links no longer hold the task back.
  - Float is measured on the finish only (total float = working time from early to late
    finish). The late start is that of the remaining work.
- *Not started:* the task is scheduled by logic, then pushed to the data instant if logic
  allows an earlier start (`Driver::StatusDate`). `explain` says so.
- *Leveling* never moves started work. It places started work first, so the resources that
  work holds are booked before anything else competes for them.
- *Work spans:*
  - A task in progress that sits idle (working time passes between the work done and the
    resumption) works in two spans: `TaskDates::work_spans`.
  - Resource demand, cost spreading (the `pause` of `Accrual::Spread`) and the Gantt bar use
    the spans. No demand or cost accrues while the task waits.
  - A gap made only of non-working time does not split the task.

**Lint** (knowledge notes, scheduling Annex A):
- `L-BAS-01` (warn): progress without a baseline.
- `L-BAS-02` (error): progress without a status date.
- `L-BAS-04` (error):
  - an actual date after the status date;
  - a started task whose mandatory finish-to-start predecessor (on it or on an ancestor) has
    not started.

  These are derived consistency rules, not taken from the sources.

**Commands:**
- `alma progress date D` (op `progress.date`).
- `alma progress set ID [--start D] [--finish D] [--remaining DUR] [--clear]` (op
  `progress.set`).
- `alma progress ls [--all] [--baseline B]`, read-only:
  - counts by state, and the forecast finish against the baseline;
  - every task started or finished, and every task the baseline expected to start by the
    status date that has not (`late_start`).
- MCP tools: `progress`, plus `progress_set` and `progress_date`, which are generated from the
  schema.
- Task views gain `progress` (summaries roll up) and `remaining`.
- The Gantt chart:
  - draws a vertical line at the status date;
  - draws a dark stripe over the done part of each task bar;
  - draws two spans joined by a thin line for a task waiting out of sequence.

## Consequences
- Agents can report status with facts only (dates and work left). The forecast, float and
  critical path follow from them.
- Actual dates are stored in the plan because they are inputs, not computed values (ADR 0003
  still holds): the forecast dates of unfinished work are never written.
- When the status date is set before any progress is recorded, every task moves after it. That
  is the honest forecast ("nothing has started"), and `progress ls` lists the late starts.
- Tests:
  - hand-worked cases in `crates/alma-sched/tests/progress.rs` for in progress, remaining
    work, the data-date floor, both progress logics, reached milestones, float of work in
    progress, leveling and cost while waiting;
  - a property test of the invariants;
  - parser, contract and MCP tests.
- `progress_rule` stays reserved. ADR 0019 settles it together with `ev_method`.
