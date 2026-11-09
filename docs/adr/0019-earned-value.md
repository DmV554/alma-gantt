# 0019 — Earned value

- **Status:** accepted
- **Date:** 2026-11-09

## Context
SPEC.md §4 (point 6) promises earned value on top of the cost-loaded schedule. The knowledge
notes (resources §5.3–5.6, Appendix A and B.3) define the inputs and the formulas:
- inputs: PV from the baseline, EV from measured progress, AC entered;
- formulas: F-COST-13 to F-COST-30;
- lint rules: `L-EVM-01..08`.

ADR 0018 records progress. Two pieces were still missing:
- the baseline snapshot (format 0, ADR 0013) holds only the project's planned-value curve and
  per-task totals, so a task or control account cannot be measured;
- the keys `physical_pct` and `ev_method` were reserved, and there was nowhere to record what
  was spent.

## Decision
**Data** (`docs/format.md` §6.13):
- `physical_pct` (0–100), on leaf tasks, needs `actual_start`; a finished task is 100.
- `ev_method`: either `physical` (the default) or a fixed formula `S/F` with S + F = 100
  (`0/100`, `50/50`, `20/80`).
- `actual_cost`, cumulative through the status date, allowed on any node. Cost is often known
  only per control account, and it rolls up.
- `progress_rule` is dropped from the reserved keys, because it duplicated `ev_method`.
  `control_threshold` is dropped too: thresholds are lint parameters.

**The snapshot gains a format version 1:**
- Each task stores its own `budget` (its assignments and fixed cost, with no descendants and
  no reserves) and a cumulative daily `pv` curve.
- Format 0 files are still read; earned value against them is `L-EVM-04`, with a hint to save
  a new baseline.

**Measures** (`alma_sched::evm`, pure):
- PV (F-COST-13): the snapshot's curve of each task through the status date.
- EV (F-COST-14): the task's budget × its earned fraction, capped at 1.
  - Complete earns 1, not started earns 0.
  - In progress earns `physical_pct`/100 or S/100.
  - Level-of-effort tasks, and money carried by a summary itself, earn as planned (min(PV,
    BAC)). They have no deliverable to measure. This is the conventional meaning (the notes
    say the sources do not define it).
- AC (F-COST-15): entered.
- Rows roll up over descendants.
- Baseline tasks the plan no longer has keep their budget and PV in the project row until a
  revised baseline drops them (scope change goes through change control).
- **Reserves are not earned (our interpretation).** BAC here is the sum of task budgets.
  Contingency stays a reserve until a change moves it into tasks, so it never makes SPI look
  worse.
- Indices (F-COST-16 to F-COST-28, F-COST-30), with `None` instead of a division by zero:
  - SV, CV, SV%, CV%, SPI, CPI;
  - the three statistical EACs side by side (budget rate, CPI, CPI × SPI);
  - ETC, VAC, TCPI against BAC and against the CPI EAC, percent complete and percent planned.
  - The weighted EAC (an interpretation in the notes) and the bottom-up EAC (it needs a
    manual ETC) are left out.

**Lint:**
- `lint_baseline(plan, schedule, snapshot)` holds the rules that need the active baseline.
  `alma_ops::load_file` adds them for the CLI and MCP. They are:
  - `L-BAS-03` (warn): a milestone or the project finish slips more than `finish_slip`
    (default 5d) past the baseline;
  - `L-EVM-01` (warn): CV% or SV% of the project or of a control account below minus
    `cv_threshold` or `sv_threshold` (percent, default 10);
  - `L-EVM-02` (error): progress on a costed task with no baseline budget;
  - `L-EVM-03` (warn): actual cost on unstarted work, or value earned with no actual cost
    recorded anywhere;
  - `L-EVM-05` (warn): TCPI(BAC) above `tcpi_max` (default 1.1).
- `L-EVM-04` (error) is returned by `alma evm` when there is no baseline, no status date or a
  format 0 snapshot.
- `L-EVM-08` (info, no baseline needed): a task measured 0/100 that lasts more than two
  reporting periods.
- `L-EVM-06` and `L-EVM-07` need a bottom-up ETC and a history of status dates; they come later.

**Commands:**
- `alma progress set --pct N --cost X`;
- `alma evm [--depth N | --accounts] [--baseline B]` (MCP `evm`);
- `alma rule param NAME VALUE` (op `rule.param`), which sets `[rules.params]`.

## Consequences
- EVM works from facts an agent can collect (dates, percent done, money spent) and is
  computed, never stored (ADR 0003).
- Baselines saved before this change must be saved again to measure earned value. Their
  variance and S-curve still work.
- EV is cumulative at one status date. Per-period EV and AC need status snapshots over time
  (the `--history` the notes describe); that is future work.
- Tests: hand-worked example (`crates/alma-sched/tests/evm.rs`), named with the formula and
  rule ids, a property test that EV ≤ BAC and PV ≤ BAC per task, a snapshot round trip for
  both formats, and parser, contract and MCP tests.
