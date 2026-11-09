# 0015 — Risks with EMV, and funding checked against spend

- **Status:** accepted
- **Date:** 2026-11-08

## Context
ADR 0014 made contingency explicit, but its size still had no objective basis. The knowledge
notes (resources §7.1, F-COST-37) size contingency from the expected monetary value of
identified risks. They also ask for funding limits: money arrives in steps, and planned spend
must not run ahead of it (§4.4, F-COST-11, `L-COST-04`). Both were reserved keys.

## Decision
- **`[[risk]]`** fields:
  - `probability` (0–1) and `impact` (money) and/or `impact_time` (duration);
  - `sign = threat | opportunity` and `status = open | closed`;
  - `owner`, `affects` (task ids) and `correlation_group`.

  EMV = probability × impact, negative for opportunities. The time EMV is probability ×
  `impact_time`, rounded up to the minute. Only open risks count.
- **`[[reserve]] risks = [...]`** links a contingency to the risks it covers. Lint:
  - `L-RISK-01` now fires only when a contingency has neither a basis nor linked risks.
  - `L-RISK-02` (info): the amount differs from the EMV of its open risks by more than
    `[rules.params] emv_tolerance` (percent, default 10).
  - `L-RISK-03` (info): two or more of its risks share a correlation group. Summing them as
    independent understates the exposure.
  - `L-RISK-05` (info): all its risks are closed, so it should be released.
- **`[[funding]]`** = `{ id, date, amount, source }`: money available from that day. Cumulative
  funding is the sum of the entries dated on or before a day. Spend is the daily cumulative cost
  baseline of the current schedule (management reserve is not spent until released). The first
  day spend exceeds funding is `L-COST-04`: a warning while `phase = "draft"`, an error after,
  like `L-SCH-05`. The diagnostic gives the day and the shortfall. Its ids are the funding
  entries available by then, so adding later funding does not report it as new.
- **Interface:**
  - `alma risk add|set|rm|ls` and `alma funding add|rm` (batch ops, hence MCP tools), plus
    MCP `risk_list`. `risk rm` is refused while a reserve covers the risk (`E-OPS-03`): close
    it instead.
  - `alma cost` reports the funding total and the first shortfall.
  - `alma scurve` gains a cumulative `funded` column, and the S-curve chart draws it as a
    staircase.

## Consequences
An agent can size contingency from risks, and an evaluator can see where each number comes
from. It can also check that the cash plan is fundable and fix it: delay flexible work with SNET
constraints, or add a tranche. Monte Carlo (later) can reuse the same risks, because
`impact_time` and `affects` are already stored. A funding-driven leveling mode
(`alma level --mode funding`) is a possible extension, not built.
