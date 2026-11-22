# 0008 — Diagnostic and formula codes

- **Status:** accepted
- **Date:** 2026-10-29

## Context
The two knowledge notes numbered lint rules and formulas independently, so codes collided
(`L-EST-01..05`, `F1` vs `F-01`), and SPEC.md §8a used a third scheme (`E012`).

## Decision
- `E-<AREA>-nn`: hard errors (invalid or unschedulable plan), never configurable.
- `L-<AREA>-nn`: lint rules, severity configurable in `[rules.severity]` or per node with `allow`.
- Formulas: `F-SCH-nn` (scheduling note) and `F-COST-nn` (cost note). Tests cite them as
  `f_sch_07_...`, `l_dep_01_...`.
- Command-layer errors use `E-OPS-nn`, CLI invocation errors `E-CLI-nn` (`docs/format.md` §9.2).
- Codes are never reused. The renumbering table is in `docs/format.md` §9.3.

## Consequences
Every code is unique across the project, so a test name or a diagnostic identifies one rule.
