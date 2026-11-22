# 0017 — Resource calendars narrow the tasks they work on

- **Status:** accepted
- **Date:** 2026-11-08

## Context
A resource can have its own `calendar` (a part-timer, a crew on a 6-day week, equipment
available on weekends). Until now that calendar only set capacity hours in `alma res usage`:
a task worked by a Monday-to-Wednesday person was still scheduled Monday to Friday. The
knowledge notes (resources §1.3) say a task's working time is the intersection of its calendar
and its resources' calendars, configurable. They also ask for `L-RES-03` when an assignment
falls outside the resource's availability.

## Decision
- **`[settings.schedule] resource_calendars = "intersect" | "ignore"`**, default
  `intersect`. With `intersect`, a task assigned to resources that have an explicit `calendar`
  works only when its own calendar and all of theirs do: workdays, hours and exceptions are
  intersected, and exception names are kept so `explain` can name them. Resources without
  a `calendar` key do not narrow anything. Durations, lags measured on the successor or
  predecessor, floats, demand, leveling and cost all use the combined calendar, because they
  all go through `Calendars::of_task`.
- **No shared working time** (for example a weekend crew on a weekday task): `L-RES-03`
  (error) names the task and the resource. The task keeps its own calendar, so the schedule
  stays computable while the plan is fixed.
- Time-phased availability (`available_from`, `available_to`, `availability`) stays reserved.

## Consequences
Plans with part-time people or special crews schedule realistically without hand-made task
calendars. `ignore` keeps the old behavior for plans that model availability another way.
Combined calendars are cached per (task calendar, resource calendars) set, so plans where many
tasks share the same team pay for one intersection.
