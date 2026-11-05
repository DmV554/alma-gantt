# 0007 — Plan file format, version 0

- **Status:** accepted
- **Date:** 2026-10-29

## Context
The format was spread over SPEC.md §6, §7.6 and §8, two knowledge notes and the fixture, and they
contradicted each other (list in `docs/format.md` §11).

## Decision
`docs/format.md` is the normative reference for `*.alma.toml` files (decisions D3–D11, D14–D16):
- Unknown keys are errors with a "did you mean" hint; free-form data goes under `custom`; keys
  reserved for later milestones fail with `E-FMT-05`.
- Every key has a tier: A (used in M0), B (accepted and validated, used later), R (reserved).
- `kind` (`task | milestone | loe | buffer`) and `wbs_role` (`deliverable | control_account |
  work_package | planning_package`) are orthogonal; a summary is any node with children.
- Milestones use `class`, `owner`, `acceptance`. Iterations come from `[[cadence]]`. Time reserves
  are `kind = "buffer"`. Assignments are `assign`. `sched_type` + `effort` for effort-driven tasks.
- IDs are globally unique slugs. `[project] format = 0` versions the file.
- Baseline snapshots will live in `.alma/baselines/`, never inside the plan (ADR 0003).
- The fixture lints cleanly when `alma check` reports no `error` and no `warn`.

## Consequences
A test keeps `docs/format.md` and the parser's key registry identical. Adding an accepted key is
non-breaking; changing the meaning of one requires a new `format` version.
