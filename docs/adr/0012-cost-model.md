# 0012 — First cost model: rates, per-use costs, fixed costs and the forecast S-curve

- **Status:** accepted
- **Date:** 2026-11-08

## Context
SPEC.md §7.5 and the knowledge notes (resources §2–§5, Annex B F-COST-07/08/12) ask for task
costs, their distribution over time and an S-curve. The notes describe a much richer model:
rate tables by date, bill rates, categories on every element, reserves, baselines and EVM.
Agents need the basics now: what does the plan cost, where does the money go, and when is it
spent. The format already has `rate = { amount, per }`, `assign = [{ resource, units }]` and
`fixed_cost`.

## Decision
- **Cost per element (F-COST-07):**
  - assignment, time rate (`per = hour | day | week | month`): units × working hours of the
    task on its calendar × hourly rate;
  - assignment, `per = "use"`: units × amount. This is a per-use fee or a quantity × unit
    price, for any resource kind;
  - `fixed_cost`: its amount.

  A resource with assignments but no rate costs zero and is flagged (`L-RES-05`).
- **Rate conversion:** `day` = `hours_per_day` hours; `week` = `days_per_week` days. A
  **month is 52/12 weeks** of working time (173.33 h at 8 h × 5 d). That is our
  interpretation; the sources define no month length. Plans that bill by month should say so in
  the resource name or use a day rate.
- **Accrual (F-COST-08):**
  - time-rated costs spread evenly over the task's working time (the `flat` profile);
  - per-use costs accrue at the start;
  - fixed costs follow `accrue = start | prorated | end`.

  Lags accrue nothing. Other profiles (front, back) and date-effective rates are later work.
- **Precision:** every amount is a `Decimal` kept to 12 decimal places. Period shares are
  computed as differences of cumulative amounts, so the shares of all periods add up exactly to
  the total. Reports round to cents.
- **Roll-up:** a summary's cost includes its own elements and all descendants. Categories:
  fixed costs use their `category`, which `L-COST-01` asks for. Assignments are grouped by
  their resource's kind.
- **Naming:** the curve from the current schedule is a **forecast** (`basis = "forecast"`).
  Planned value (PV) and BAC need a baseline, as the knowledge notes insist, so `alma cost`
  reports a `total`, not a BAC.
- **Interface:**
  - `alma cost [--by wbs|resource|category] [--depth N]`;
  - `alma scurve [--period day|week|month] [--csv]`;
  - a `cost` field on every task view (`show`, `ls --fields`);
  - `alma res set` (batch op `res.set`) to change rates and capacity;
  - MCP tools `cost`, `scurve` and `res_set`.
- Resource, rate, assignment, fixed-cost and currency keys move from tier B to tier A in
  `docs/format.md`: the engine now uses them.

## Consequences
An agent can price a plan and see its spending profile, and every number can be checked by
hand: the fixture's test works it out line by line. Leveling changes when money is spent but not
how much. Baselines, reserves (contingency inside the baseline, management reserve outside it)
and earned value build on this model without changing it. Bill rates, rate tables and funding
limits are additions.
