# Resources, cost, earned value, reserves, estimation and cash flow

**Status:** research note, input for `SPEC.md` (§6 domain model, §7.4 leveling, §7.5 costs/S-curve,
§8a agent interface, lint rules). **Date:** 2026-10-29.

This note turns project-management practice into concrete requirements for alma-gantt. Every
concept has three parts:

1. **What it is**: a short explanation in our own words.
2. **Implication for alma-gantt**: a data-model field, engine behavior, CLI command or flag, view,
   export column or lint rule.
3. **Source**: the citation (`cita`) and fragment id from the knowledge base we consulted.

**Copyright.** PMBOK 6, the Agile Practice Guide, the WBS Practice Standard, the course slides
(FEP0x), the readings (LECT1-3) and the evaluation book chapters (EVAL7/8) are copyrighted. This
note paraphrases them. A few short quotes (under 15 words) appear in quotation marks with their
citation. Formulas and published method parameters (for example the UCP weights) are given as
facts. No tables or figures are copied.

**Source key.**

| Key | Document |
|---|---|
| PMBOK 6 | *A Guide to the Project Management Body of Knowledge*, 6th ed. (PMI) |
| APG | *Agile Practice Guide* (PMI) |
| WBS Practice Standard | *Practice Standard for Work Breakdown Structures*, 2nd ed. (PMI) |
| FEP02 | Course slides "Requisitos, Alcance, Planificación y EDT" |
| FEP03 | Course slides "Estimación de Software" |
| FEP04 | Course slides "Gestión de los Riesgos en Proyectos TIC" |
| FEP06 | Course slides "Contratos, Adquisiciones y Calidad" |
| Lectura 1/2/3 | Readings on software estimation methods / use case points / UCP worked example |
| EVAL7 / EVAL8 | Project evaluation textbook, ch. 7 (building cash flows) and ch. 8 (NPV, IRR); cited as "Libro sin identificar, cap. N" because the book is not identified |

---

## 1. Resources

### 1.1 Resource types: team vs physical

**What it is.** Project resources come in two families. *Team* resources are people with a
role. *Physical* resources are equipment, materials, facilities and infrastructure. The two
families behave differently. People and equipment provide capacity over time and are reused
once a task ends. Materials are consumed in quantities. Managing physical resources well is
mostly about having them on time and in the right amounts: equipment that arrives late delays
the work, and too much or too little inventory costs money.

**Implication for alma-gantt.**
- Make `Resource.kind` richer than the current `work | material | cost`:
  - `work`: people or roles. Capacity is measured in units per time, for example 1.0 = one
    full-time person.
  - `equipment`: capacity over time like `work`, but it usually has a usage rate and may need
    a *per-use* cost (mobilisation).
  - `material`: consumable. It has a quantity unit (m, t, licence, GB-month), a price per unit,
    and an assignment that sets a quantity, either fixed or per working day.
  - `cost`: a pure money line (travel, fees) with no capacity.
- Leveling (§7.4) applies only to `work` and `equipment`. A `material` can still have an
  availability limit (stock or delivery schedule), which a lint rule checks; the leveler does not
  act on it.
- Source: PMBOK 6, §X4.6 (PMBOK6-1513); PMBOK 6, p. 312 (PMBOK6-0656); PMBOK 6, §9.6
  (PMBOK6-0757); PMBOK 6, §9.3.3.1 (PMBOK6-0709).

### 1.2 Resource breakdown structure (RBS)

**What it is.** The RBS is a hierarchy of resources, grouped first by category (labor, material,
equipment, supplies) and then by type. A type can carry attributes such as skill level, grade or
required certification. Each lower level adds detail until a resource can be matched to WBS
work. The RBS is used both to plan and to monitor resources. It sits beside two related
hierarchies: the OBS (organizational units) and the WBS (deliverables).

**Implication for alma-gantt.**
- Add `Resource.parent` (an optional tree, the same pattern as tasks) and
  `Resource.category`/`Resource.skills[]`. The RBS code is derived from the tree, like the
  WBS code.
- Add `Resource.org_unit` (OBS) so that reports can group by department.
- Reports: cost and work rolled up by RBS node, for example "all Developers" or "all
  Equipment"; the histogram filtered by RBS node.
- Add an `alma res tree` CLI query, plus `--group-by rbs|obs|wbs` on cost and histogram reports.
- Assignments may point at a *role* (generic RBS node) before named people exist. Lint
  `L-RES-06` (info) lists role assignments that are still unresolved at baseline time.
- Source: PMBOK 6, §9.2.3.3 (PMBOK6-0696); PMBOK 6, §9.1.2.2 (PMBOK6-0670); PMBOK 6,
  §9.2.2.6 (PMBOK6-0692).

### 1.3 Resource calendars and availability

**What it is.** A resource calendar records when a specific resource can work: working days,
shifts, normal hours, weekends and holidays. It also records the window during which the
resource is available to the project at all. It can reflect skill level and location. Planned
leave and collective-bargaining limits are resource constraints that belong in the assumptions.

**Implication for alma-gantt.**
- `Resource.calendar` already exists in SPEC. Add:
  - `Resource.available_from` / `available_to`: the project-level availability window.
  - `Resource.availability[]`: a time-phased maximum-units table, for example
    `{from, to, max_units}`. One `max_units` value for the whole project is not enough: people
    join, leave and go part-time.
- Engine: when a task has assignments, its working time is the *intersection* of the task
  calendar and each assigned resource's calendar (configurable: intersection, or the task
  calendar only). The `explain` driver must name the resource calendar when it moved a date.
- Lint `L-RES-03` (error): an assignment falls outside the resource's availability window.
- Source: PMBOK 6, §9.3.3.3 (PMBOK6-0711); PMBOK 6, p. 704 glossary "Resource Calendar"
  (PMBOK6-1580); PMBOK 6, §9.2.3.4 (PMBOK6-0697).

### 1.4 Resource requirements and assignments (units)

**What it is.** Resource requirements state the types and quantities of resources each activity
needs. They roll up to the work package, the WBS branch and the whole project. They should come
with their basis (method, assumptions, constraints, range, confidence).

**Implication for alma-gantt.**
- `assignments[]: { resource, units, work?, quantity?, profile?, basis? }`:
  - `units` for `work`/`equipment`, as a fraction of capacity (0.5 = half time).
  - `work` (effort, person-hours) can be stored *instead of* units. With
    `task.type = fixed_work` the engine derives units or duration (see §2.1).
  - `quantity` for `material` (total, or per day with `per = "day"`).
  - `profile`: how the work is spread over the task (`flat` default, `front`, `back`, `bell`,
    or explicit per-period values). This also drives cost distribution (§5.1).
- Roll-up: `alma res usage --by wbs|rbs --period week` gives work per resource per period.
- Source: PMBOK 6, §9.2.3.1 (PMBOK6-0694); PMBOK 6, §9.2.3.2 (PMBOK6-0695).

### 1.5 Resource histogram and over-allocation

**What it is.** A resource histogram is a bar chart of how much work a resource is scheduled for
in each period. A resource is *over-allocated* when the scheduled demand in a period is higher
than its availability, for example when one person is assigned to two activities at the same
time.

**Implication for alma-gantt.**
- View/export: a histogram per resource or RBS node. Bars show demand per period, a line shows
  availability, and the excess is highlighted. Use the same `alma-layout` display list for screen
  and SVG/PDF.
- `alma res histogram <res> --period week --json` returns
  `[{period, demand, capacity, over}]` so that an agent can verify its fix.
- Lint `L-RES-01` (error after `alma level`, warning before): a resource is over-allocated in at
  least one period. The diagnostic lists the period, the excess and the competing task IDs, and
  suggests `alma level` as the fix.
- Source: PMBOK 6, p. 704 glossary "Resource Histogram" (PMBOK6-1580); PMBOK 6, §6.5.2.3
  (PMBOK6-0453).

### 1.6 Resource leveling vs resource smoothing

**What it is.** Both techniques move activity start and finish dates so that resource demand
stays at or below supply. They differ in what they may change:
- **Leveling** may delay activities beyond their float. It often extends the project and
  usually changes the critical path.
- **Smoothing** only moves activities inside their *free and total* float. The critical path and
  the end date stay fixed, so smoothing may fail to resolve every conflict.

The FEP02 slides give the same rule in short form: leveling tends to lengthen the project and
change the critical path, while smoothing does neither.

**Implication for alma-gantt.**
- Name the two modes as the sources do: `alma level --mode leveling|smoothing`. The current SPEC
  wording ("within float only" / "allow end-date slip") maps to `smoothing` / `leveling`.
  `--within-float` can stay as an alias.
- In `smoothing` mode a task may only move while its successors are not pushed (free float) and
  the project finish does not move (total float). When a conflict is left unresolved, the engine
  emits a diagnostic (`L-RES-01` stays raised) and does not slip the end date.
- After `leveling`, the engine must recompute the critical path and report it as a change in the
  mutation result: `{"critical_path_changed": true, "added": [...], "removed": [...]}`. The
  audit trail already planned in SPEC §7.4 must include it.
- Keep both schedules (unleveled and leveled). EVM and the S-curve must state which one they use.
- Source: PMBOK 6, §6.5.2.3 (PMBOK6-0453, PMBOK6-0454); PMBOK 6, p. 704 glossary
  (PMBOK6-1580); FEP02, p. 82 (FEP02-0082).

### 1.7 Lag consumes calendar time, not resources

**What it is.** A lag is waiting time. It consumes no hours or resources, but it takes days from
the committed deadline. A lead (negative lag) lets the successor overlap its predecessor.

**Implication for alma-gantt.** A lag must never generate resource demand or cost in the
histogram or the S-curve. Golden test: two tasks with an FS+5d lag produce zero demand during the
gap.
Source: FEP02, p. 69 (FEP02-0069).

---

## 2. Effort, duration and rates

### 2.1 Effort is not duration

**What it is.** Effort is the amount of work, in person-hours or person-days. It is what you
estimate per work package and the basis of cost. Duration is the calendar time the work takes,
which is what the schedule shows and the contract commits to. Effort does not change when people
are added; duration depends on how many people work and at what dedication. Converting one into
the other needs the working calendar: holidays and vacations lengthen the duration without
changing the effort. For example, 160 person-hours with two people at 100 % and 8 h/day gives
10 working days.

The relationship is not linear. Doubling the people rarely halves the duration, because of
coordination, learning curves and the law of diminishing returns. Some work has a fixed duration
whatever the staffing, such as a 24-hour stress test.

**Implication for alma-gantt.**
- Add a task scheduling type, as in MS Project and TaskJuggler:
  `task.type = fixed_duration | fixed_work | fixed_units`, and a flag `effort_driven`.
  - `fixed_work`: `duration = work / (Σ units_i × hours_per_day_i)`, evaluated on the resource
    calendars.
  - `fixed_duration`: work is derived from duration × units.
- Do **not** silently shorten a task when the agent adds a resource. With `effort_driven = true`
  the engine recomputes the duration and the mutation result reports the change. Lint `L-EST-08`
  (info) warns that the reduction assumes linear scaling.
- Source: FEP02, p. 71 (FEP02-0071); PMBOK 6, §6.4 (PMBOK6-0418, PMBOK6-0419); FEP03, p. 5
  (FEP03-0005).

### 2.2 Cost rate vs sell rate (the rate chain)

**What it is.** The hourly rate that multiplies effort in a commercial offer is not the
person's salary divided by the monthly hours. The chain goes like this:

1. Gross salary.
2. Cost to the company, which adds social charges and provisions.
3. Cost per *productive* hour. Only about 75-85 % of nominal hours are productive; the rest is
   holidays, leave and internal work.
4. Plus overhead.
5. Plus margin.
6. The result is the sell rate.

Using nominal hours underestimates the real cost by more than 20 %. Pricing at cost instead of
at the sell rate ("vender al costo") is a classic way to lose money on a contract. An offer also
adds non-labor inputs (licences, infrastructure, guarantees) and a contingency reserve; each
needs its own justification.

**Implication for alma-gantt.**
- `Resource.rates`: support at least `cost_rate` and an optional `bill_rate`, and allow rate
  tables effective by date (`[{from, cost, bill}]`), because rates change over a long project.
- Optional project settings `productive_ratio` (default 1.0), `overhead_pct` and `margin_pct`.
  With these, `alma cost --basis bill` shows the derivation and the agent can justify a price.
  Reports must say which basis they use (`cost` or `bill`).
- Lint `L-COST-05` (warning): `bill_rate < cost_rate`, or `bill` basis selected while
  `margin_pct = 0` and `overhead_pct = 0` (likely pricing at cost).
- Source: FEP03, p. 54 (FEP03-0054); FEP02, p. 90 (FEP02-0090); FEP03, p. 55 (FEP03-0055).

---

## 3. Cost estimating and aggregation

### 3.1 What a cost estimate covers

**What it is.** A cost estimate approximates the money needed for the work. It covers every
resource charged to the project: labor, materials, equipment, services, facilities and IT. It
can also include special categories: inflation allowance, financing cost (interest), exchange
rates and contingency. Indirect costs can be added at activity level or higher.

Estimates are refined as the project advances. A rough order of magnitude (ROM) estimate at
initiation might be within -25 % to +75 %. A definitive estimate later might narrow to -5 % to
+10 %. An estimate should state its range and confidence, for example "10,000 ±10 %".

**Implication for alma-gantt.**
- Every cost element (assignment cost, fixed cost, material) can carry a `category` from a
  configurable list (`labor`, `material`, `equipment`, `services`, `facilities`, `it`,
  `financing`, `inflation`, `fx`, `indirect`). Reports and XLSX/CSV exports group by category.
  The cash-flow export (§10) uses the same categories.
- Add `project.estimate_class` (`rom | budget | definitive`, or a free label) with
  `accuracy = {low_pct, high_pct}`. Reports print the range next to totals: "BAC 12,400 UF
  (-5 % / +10 %)".
- Add `project.currency` plus optional `fx[]` tables, used when cost lines are in another
  currency. Money stays `rust_decimal`.
- Source: PMBOK 6, §7.2 (PMBOK6-0513); PMBOK 6, §7.2.3.1 (PMBOK6-0528); PMBOK 6, §7.2.3.2
  (PMBOK6-0529).

### 3.2 Cost management plan parameters

**What it is.** The cost management plan fixes the rules of the game:
- units of measure per resource (hours, days, metres, lump sum);
- level of precision (rounding);
- level of accuracy (the acceptable ± range);
- the link to accounting through *control accounts*, each with its own code;
- control thresholds (the allowed variance, usually a percentage, before action is required);
- the rules of performance measurement (where EV is measured, which EV technique, which EAC
  formulas);
- report formats and frequency;
- the procedure for currency fluctuations.

The schedule management plan has the same items for time.

**Implication for alma-gantt.**
- A `[cost]` settings table in the plan file:
  `currency`, `rounding` (for example `"1"` or `"0.01"`), `accuracy`,
  `thresholds = { cv_pct, sv_pct, cpi_min, spi_min }`, `ev_method_default`
  (see §5.6), `eac_method_default`, `reporting_period = "month"`.
- `alma check` uses the thresholds. Lint `L-EVM-01` (warning) fires when an EV metric crosses its
  threshold at the status date.
- Rounding happens **only at presentation**. Engine sums use full precision, so that totals stay
  consistent.
- Source: PMBOK 6, §7.1.3.1 (PMBOK6-0508, PMBOK6-0509, PMBOK6-0510); PMBOK 6, §6.1.3.1
  (PMBOK6-0388); FEP02, p. 64 (FEP02-0064).

### 3.3 Estimating methods for cost

**What it is.** The four classic methods apply to cost as well as duration (see §8):
- **analogous**: scale from a similar past project;
- **parametric**: a statistical relationship, cost = quantity × unit rate, or a more complex
  model;
- **bottom-up**: estimate the lowest-level work, then roll it up;
- **three-point**: optimistic, most likely and pessimistic values, combined with a triangular
  or beta formula.

Supporting tools include alternatives analysis (make or buy) and cost of quality. PMBOK notes
that the PMIS (spreadsheets, simulation, statistics) makes these techniques easier to apply.

**Implication for alma-gantt.** Every estimate field (duration, work, cost) accepts an
`estimate` object: `{method, value | (o, m, p) | (qty, rate), basis, source}`. The engine
computes the expected value from it (§8.3), and `basis` keeps the trail an evaluator asks for.
Source: PMBOK 6, §7.2.2.2-§7.2.2.6 (PMBOK6-0520, PMBOK6-0521, PMBOK6-0522, PMBOK6-0523,
PMBOK6-0524); PMBOK 6, §7.2.2.7 (PMBOK6-0526).

### 3.4 Cost aggregation: activity → work package → control account → project

**What it is.** Activity estimates, plus any activity-level contingency, add up to work packages.
Work packages, plus any WP-level contingency, add up to *control accounts*. The control
accounts add up to the cost baseline. Adding the management reserve gives the project budget.

A control account is the management point where scope, budget, actual cost and schedule meet
and are compared against earned value. Each work package belongs to exactly one control account,
and each control account maps to one organizational unit. A *planning package* sits below a
control account: its content is known, but its activities are not yet detailed.

**Implication for alma-gantt.**
- Add `task.role` (orthogonal to `kind`): `activity | work_package | planning_package |
  control_account`. Defaults: a leaf is an activity; a summary whose children are all activities
  is a work package. Control accounts are explicit.
- Add `task.account_code` on control accounts, for the accounting link. It is exported as a
  column.
- Engine: cost and EV roll up along the WBS; reports can stop at the `control_account` level.
- Lint `L-COST-02` (error): a work package sits under more than one control account (for
  example because the WBS was reshaped), or a control account has no work packages.
- Lint `L-COST-03` (warning): a planning package still has no activities after its start date
  minus the rolling-wave horizon (configurable).
- Source: PMBOK 6, §7.3.2.2 (PMBOK6-0539); PMBOK 6, §7.3.3.1 (PMBOK6-0544); PMBOK 6, §5.4.3.1
  (PMBOK6-0342); WBS Practice Standard, §2.2.1 (WBS2-0024); FEP04, p. 49 (FEP04-0049).

---

## 4. Reserves, baselines, budget and funding

### 4.1 Contingency reserve (known unknowns)

**What it is.** A contingency reserve is budget or duration set aside for *identified* risks
that the project has accepted: the known unknowns. It can be defined at any level, from a single
activity to the whole project, and sized as a percentage, a fixed amount or the result of
quantitative analysis. It is **inside** the cost baseline (and the schedule baseline when it is
time) and must be shown explicitly. The project manager uses it when a foreseen risk occurs.
Using it is normal and does **not** change the baseline. It can be reduced or released once the
risk is closed.

FEP04 adds a practical rule: size the cost contingency as the sum of the expected monetary
values of the quantified risks, then state it as its own line in the offer and never hide it
inside the hours. FEP02 makes the same point for time: a reserve that is not declared does not
exist, and a delay without one reads as non-compliance.

**Implication for alma-gantt.**
- A first-class `[[reserve]]` object:
  `{ id, kind = "contingency", dimension = "cost" | "time", amount | duration, scope = <task id
  or "project">, basis, risks = [risk ids] }`.
  - Cost contingency is **included** in BAC/PMB (performance measurement baseline) and in the
    S-curve. Its default time distribution is configurable: spread like its scope, or held as a
    lump at the scope's finish.
  - Time contingency is modeled as a **buffer task** (`kind = "buffer"`, no resources, no cost)
    placed in the network, usually before a contract milestone. It is part of the schedule
    baseline. The Gantt shows it differently from normal work.
- Command `alma reserve draw <id> --amount 3 --reason R-07`. It records the use and does not
  rebaseline.
- Lint `L-RISK-01` (warning): a contingency reserve has no basis and no linked risks (a "cushion"
  that an evaluator will discount).
- Lint `L-RISK-02` (info): the cost contingency differs from the Σ EMV of the linked risks by
  more than a threshold.
- Source: PMBOK 6, §7.2.2.6 (PMBOK6-0524, PMBOK6-0525); PMBOK 6, §6.4.2.6 (PMBOK6-0433);
  FEP02, p. 77 (FEP02-0077); FEP04, p. 48 (FEP04-0048); FEP04, p. 49 (FEP04-0049).

### 4.2 Management reserve (unknown unknowns)

**What it is.** A management reserve is held back for unforeseen work that is still within
scope: the unknown unknowns. It sits **outside** the cost and schedule baselines but **inside**
the project budget and the total funding requirement. Management (the sponsor), not the project
manager, releases it. Using it moves the amount into the baseline through an approved change, so
the baseline changes. In a fixed-price bid it is often not declared to the client; the margin
absorbs it.

**Implication for alma-gantt.**
- `[[reserve]] kind = "management"`. It is **excluded** from BAC/PV/EV and **included** in
  "project budget" and "total funding". The S-curve report can draw the budget ceiling
  (PMB + MR) as a horizontal reference.
- `alma reserve release <id> --amount X --change CR-12` works only together with a baseline
  revision (§6). It moves money from MR into a control account or a new work package and records
  the change reference.
- Lint `L-BAS-08` (error): the management reserve decreased while the current baseline did not
  increase by the same amount, or the reverse.
- Source: PMBOK 6, §7.3.2.3 (PMBOK6-0540); PMBOK 6, §7.3.3.1 (PMBOK6-0545); PMBOK 6, §6.4.2.6
  (PMBOK6-0434); FEP04, p. 49 (FEP04-0049); FEP02, p. 77 (FEP02-0077).

### 4.3 Cost baseline vs project budget (the stack)

**What it is.** The amounts nest like this:
- activity estimates + activity contingency → work package cost;
- work package cost + WP contingency → control account;
- Σ control accounts → **cost baseline** (= PMB when EVM is used);
- cost baseline + management reserve → **project budget**.

The cost baseline is the *approved, time-phased* budget without management reserve. It changes
only through formal change control and is the reference that actuals are compared against.
Control is measured against the baseline, not against the total budget.

**Implication for alma-gantt.**
- `alma cost summary --json` returns exactly these levels, each with its amount:
  `activities`, `contingency_activity`, `work_packages`, `contingency_wp`, `control_accounts`,
  `cost_baseline` (= PMB = BAC), `management_reserve`, `project_budget`. An agent or a report can
  then reproduce the stacked picture without guessing.
- Golden test: the stack adds up exactly (Decimal) for the sample plan.
- Source: PMBOK 6, §7.3.3.1 (PMBOK6-0544, PMBOK6-0545); FEP04, p. 49 (FEP04-0049).

### 4.4 Funding requirements and funding-limit reconciliation

**What it is.** Funding requirements come from the cost baseline, as a total and per period
(quarter, year). The total funding needed is the cost baseline **plus** the management reserve.
Funding often arrives in uneven steps, so the funding curve is a staircase over the S-curve.
Funding requirements may name the source of the funds.

If planned spending in some period exceeds a funding limit, work must be rescheduled to flatten
the spending rate. This is done with imposed date constraints. Externally financed projects
(common in infrastructure) may have extra conditions from the funder.

**Implication for alma-gantt.**
- `[[funding]]` entries: `{ period | from/to, amount, source }`. A cumulative funding staircase
  is drawn on the S-curve chart.
- Engine check: cumulative planned spend (PMB, plus MR if `--include-mr`) versus cumulative
  funding at each period end.
- Lint `L-COST-04` (error): cumulative planned spend exceeds cumulative funding in some period.
  The diagnostic gives the period and the gap, and suggests either a `SNET` constraint on the
  most flexible tasks with spend in that period (fix command: `alma task set <id> --constraint
  SNET:<date>`) or more funding.
- A future `alma level --mode funding`, an optional heuristic that delays tasks with float until
  the spending fits, is the money analogue of resource leveling. Mark it "later".
- Source: PMBOK 6, §7.3.3.2 (PMBOK6-0546); PMBOK 6, §7.4.1.3 (PMBOK6-0552); PMBOK 6, §7.3.2.5
  (PMBOK6-0542); PMBOK 6, §7.3.2.6 (PMBOK6-0543).

---

## 5. Cost over time and the S-curve

### 5.1 Time-phasing costs

**What it is.** The cost baseline is tied to schedule activities, so it can be shown as money per
period. Costs accrue when work is done, according to how each resource's work is distributed
over the activity.

**Implication for alma-gantt.**
- Cost distribution must follow the assignment `profile` (§1.4) and the working calendar. SPEC
  §7.5's "linear across working days" is the `flat` default.
- Fixed costs and materials need an `accrual` setting: `start | end | prorated | milestone`.
  Example: a licence is paid at start, an acceptance payment at a milestone.
- Periods: `day | week | month | quarter | year`, using a configurable calendar (ISO weeks,
  calendar months). The period boundaries and the number of the first period ("period 0"
  convention, §10) must appear in the export header.
- Source: PMBOK 6, §7.3.3.1 (PMBOK6-0544).

### 5.2 The S-curve: what it is and how it is built

**What it is.** The S-curve is the *cumulative* time-phased budget. Plotted against time it
usually looks like an S: spending is slow at the start, fastest in the middle and slow again
near the end. The baseline S-curve is the cost baseline (PMB), so it includes contingency and
excludes management reserve. In EVM reports three cumulative curves share one chart: PV, EV and
AC, each reported per period and cumulatively. Funding appears as a staircase on top.
Separately, quantitative risk analysis produces a *probability* S-curve: the cumulative
probability of finishing by a date, or at or under a cost. It is a different chart with the same
name.

**Implication for alma-gantt.**
- Building algorithm (deterministic):
  1. Take the chosen schedule (baseline *n*, or current; leveled or unleveled, stated in the
     output).
  2. For every cost element, distribute its amount over working time according to its profile
     and accrual.
  3. Add contingency reserves according to their distribution rule.
  4. Bucket the amounts into periods, then compute running totals.
  5. Output `[{period_start, period_end, pv, pv_cum, ev, ev_cum, ac, ac_cum, funding_cum,
     budget_ceiling}]`.
- Naming rule: **PV always comes from a baseline.** A curve from the *current* plan is a forecast
  curve (`forecast_cum`), not PV. SPEC §7.5 currently calls both "planned value"; see the gaps
  section.
- `alma scurve --baseline "Informe 3" --period month --json|--csv|--svg`; plus
  `--probabilistic` once Monte Carlo exists (§7.3), which produces the other S-curve.
- Source: PMBOK 6, §7.3.3.1 (PMBOK6-0544); PMBOK 6, §7.3.3.2 (PMBOK6-0546); PMBOK 6, §7.4.2.2
  "Charts" (PMBOK6-0563); PMBOK 6, §11.4.2.5 (PMBOK6-0929).

---

## 5 (cont.). Earned value management (EVM)

EVM combines the scope, schedule and cost baselines into the **performance measurement baseline
(PMB)**. It tracks three quantities for every work package and control account. All the other
metrics derive from these three.

### 5.3 PV, EV, AC, BAC

**What it is.**
- **PV (planned value)**: the authorized budget for the work *scheduled* to be done by a date,
  excluding management reserve. Its total over the project is the **BAC (budget at
  completion)**, which is also the PMB total.
- **EV (earned value)**: the budget of the work actually *completed*, measured against the PMB.
  It is capped: a component's EV can never exceed its PV budget. It is tracked both per period
  (current status) and cumulatively (trend). Each WBS component needs progress-measurement
  criteria.
- **AC (actual cost)**: the cost actually incurred for the work measured by EV. It has no cap. It
  must be defined the same way as PV and EV: if PV counts only direct hours, AC must too.

**Implication for alma-gantt.**
- Data: `progress` (SPEC M4) needs, per task and status date: `percent_complete` (or milestone
  flags, see §5.6), `actual_start`, `actual_finish`, `remaining_duration`, and
  **`actuals[]: {period, cost, work?}`**. AC is entered data and cannot be computed from rates,
  because whatever was really spent counts.
- `project.status_date` (data date) is required for any EVM output.
- Engine: `EV_task = BAC_task × percent_complete_earned`, where the earned percent follows the
  task's EV method; roll up by summing EV; `PV(t)` = baseline S-curve at t.
- Lint `L-EVM-02` (error): a task's EV exceeds its BAC (percent > 100, or progress recorded
  without a baseline budget).
- Lint `L-EVM-03` (warning): AC is recorded for a task with EV = 0 and no actual start, or EV > 0
  with no AC at all (inconsistent definitions).
- Lint `L-EVM-04` (error): EVM requested without a status date or without a baseline.
- Source: PMBOK 6, §7.4.2.2 (PMBOK6-0556, PMBOK6-0557); PMBOK 6, §6.6.1.3 (PMBOK6-0478).

### 5.4 Variances and indices: SV, CV, SPI, CPI, VAC

**What it is.**
- **SV = EV − PV**: ahead (positive) or behind (negative), expressed in money. At completion it
  always ends at 0, because everything planned has been earned. So it should be read together
  with CPM: a delay on the critical path matters more than SV shows.
- **CV = EV − AC**: budget surplus (positive) or deficit (negative). At the end it equals BAC
  minus the actual spend. A negative CV is hard to recover.
- **SPI = EV / PV**: below 1 means less work done than planned. Because SPI mixes all work,
  critical-path performance must be checked separately.
- **CPI = EV / AC**: below 1 means a cost overrun on the completed work. PMBOK calls it "the most
  critical EVA metric" (PMBOK 6, §7.4.2.2).
- **VAC = BAC − EAC**: the expected surplus or deficit at completion.
- The acceptable variance range tends to *narrow* as more work is completed.

**Implication for alma-gantt.**
- `alma evm --status-date 2027-06-30 --level control_account --json` returns, per row:
  `pv, ev, ac, bac, sv, cv, spi, cpi, sv_pct, cv_pct, eac, etc, vac, tcpi_bac, tcpi_eac`.
  Values are computed both for the period and cumulatively (`--period month` adds the
  per-period series).
- Division by zero (PV = 0 or AC = 0) returns `null` with a reason, never `inf` or a panic.
- Because SV ends at 0 by construction, `alma evm` also prints the *schedule* view: finish
  variance of the critical path against the baseline (from `alma diff`), as PMBOK advises.
- Thresholds: optionally tighten them as the percentage complete grows (for example
  `thresholds = [{until_pct = 20, cv_pct = 15}, {until_pct = 100, cv_pct = 5}]`).
- Source: PMBOK 6, §7.4.2.2 (PMBOK6-0558, PMBOK6-0559, PMBOK6-0560, PMBOK6-0561, PMBOK6-0562).

### 5.5 Forecasting: EAC, ETC, TCPI

**What it is.** As work progresses, the team forecasts the **EAC (estimate at completion)**,
which may differ from the BAC. The most common approach is manual and bottom-up:
**EAC = AC + bottom-up ETC**, where ETC (estimate to complete) is a fresh estimate of the
remaining work. This manual EAC is then compared with statistical EACs that use cumulative
indices. PMBOK gives three:

1. **Remaining work at the budgeted rate:** EAC = AC + (BAC − EV). Assume this only when risk
   analysis supports a future improvement.
2. **Remaining work at the current CPI:** EAC = BAC / CPI. Past cost efficiency continues.
3. **Remaining work at CPI × SPI:** EAC = AC + (BAC − EV) / (CPI × SPI). Useful when schedule
   pressure affects cost. Variants weight CPI and SPI (80/20, 50/50, ...).

**TCPI (to-complete performance index)** is the cost efficiency needed on the remaining work to
meet a goal:
- against the budget: TCPI = (BAC − EV) / (BAC − AC);
- against an approved EAC: TCPI = (BAC − EV) / (EAC − AC).

When the BAC is no longer viable, the approved EAC replaces it in TCPI. Trend analysis looks at
these values over successive periods to see whether performance improves or deteriorates.

**Implication for alma-gantt.**
- `eac_method` is per project, overridable per control account:
  `bottom_up | budget_rate | cpi | cpi_spi | weighted{w_cpi, w_spi}`. The default is `cpi`, but
  the report always shows **all** statistical EACs side by side, plus the bottom-up value when
  `etc` has been entered. PMBOK presents them as a range of scenarios for the same purpose.
- Data: `task.etc` (optional manual estimate to complete; cost, or work converted via rates) for
  the bottom-up EAC.
- For the weighted variant, implement the common form
  `EAC = AC + (BAC − EV) / (w_cpi·CPI + w_spi·SPI)` with `w_cpi + w_spi = 1`, and document it as
  an interpretation: PMBOK names the weighting but does not print the formula.
- History: `alma evm --history` reads `.alma/status/*.toml` snapshots (one per status date) and
  outputs the CPI/SPI/EAC trend per period. The snapshots must stay out of the source file
  (SPEC §6 rule) but be versionable.
- Lint `L-EVM-05` (warning): TCPI(BAC) > 1.1 (configurable) while EAC still equals BAC, which
  means the plan assumes a recovery that is unlikely.
- Lint `L-EVM-06` (info): the bottom-up EAC lies outside the [min, max] of the statistical EACs.
- Source: PMBOK 6, §7.4.2.2 (PMBOK6-0563, PMBOK6-0564, PMBOK6-0565, PMBOK6-0566); PMBOK 6,
  §7.4.2.3 (PMBOK6-0568, PMBOK6-0569); PMBOK 6, §7.4.3.1 (PMBOK6-0571).

### 5.6 Measuring progress: percent-complete rules and EV techniques

**What it is.** The cost and schedule management plans must define *how* the percent complete
of each work package is declared. PMBOK lists techniques such as weighted milestones,
fixed-formula and percent complete. The FEP02 slides name three practical rules: completed
milestones, a fixed percentage at start and at finish (the fixed formula, for example 0/100 or
50/50), and units completed. PMBOK also stresses *physical* percent complete, measured on the
work itself, as opposed to percent of duration elapsed. The WBS Practice Standard separates
**discrete effort**, which produces measurable deliverables, from **apportioned effort**, which
is tied in proportion to discrete work. **Level of effort (LOE)** is support work with no
definitive end product, such as project management or liaison.

*Not found in the sources:* the specific percentages of the 0/100, 50/50 or 20/80 rules, and how
EV is computed for LOE. The definitions below are the conventional industry meaning. Mark them
as such in the alma docs.

**Implication for alma-gantt.**
- `task.ev_method`:
  - `percent` (physical % entered by the user);
  - `fixed{start_pct, finish_pct}` (0/100 → `{0,100}`, 50/50 → `{50,50}`): EV is credited
    at actual start and at actual finish;
  - `milestones[{id, weight}]`: EV is credited when the child milestones finish;
  - `units{done, total}`;
  - `apportioned{base = <task id>, factor}`;
  - `loe`: EV = PV for elapsed time; LOE never shows a schedule variance.
- The default comes from `[cost] ev_method_default`. Do **not** default to "% of duration
  elapsed". It is not a physical measure and hides delays.
- Lint `L-EVM-07` (warning): a task uses `percent` with progress values that exactly track
  elapsed duration across ≥ 3 status dates (suspicious: likely not physical).
- Lint `L-EVM-08` (info): a long task (> 2 reporting periods) uses `fixed{0,100}`. EV will be
  lumpy; suggest milestones or 50/50.
- Source: PMBOK 6, §7.1.3.1 (PMBOK6-0510); PMBOK 6, §6.1.3.1 (PMBOK6-0388); FEP02, p. 64
  (FEP02-0064); PMBOK 6, §6.6.1.3 (PMBOK6-0478); PMBOK 6, p. 696 glossary "Percent Complete"
  (PMBOK6-1566); WBS Practice Standard, §2.2.1 (WBS2-0024); WBS Practice Standard, p. 119
  (WBS2-0267).

### 5.7 Reporting periods and dates

**What it is.** PV, EV and AC are reported per period (typically weekly or monthly) and
cumulatively. Schedule control compares actual start and finish dates, percent complete and the
remaining duration of work in progress against the baseline dates. Variance analysis covers
dates, durations *and float*. A large delay off the critical path may not matter, while a small
delay on a critical or near-critical activity may.

**Implication for alma-gantt.**
- Three date sets per task: **baseline** (from a named baseline), **current/forecast**
  (rescheduled from the status date with remaining durations) and **actual** (entered). The Gantt
  shows the baseline ghost bar plus the current bar with its actual part.
- `alma diff --baseline X` must include *float variance* and flag near-critical tasks (total
  float ≤ a configurable threshold) whose finish moved.
- Rescheduling after progress: work not done before the status date moves to after it
  (`reschedule_incomplete_after_status_date = true`).
- Source: PMBOK 6, §7.4.2.2 "Charts" (PMBOK6-0563); PMBOK 6, §6.6.2.1 (PMBOK6-0481); PMBOK 6,
  §6.5.3.1 (PMBOK6-0462).

### 5.8 EVM in an agile context

**What it is.** SPI and CPI translate directly to iterations. SPI = story points completed /
story points planned, for example 25/30 = 0.83. CPI = value of completed features / actual cost
to date. Burnup charts show completed work against total scope and expose scope growth.

**Implication for alma-gantt.** Iterations (SPEC §6) can carry `planned_points` and
`completed_points`. `alma evm --agile` computes an iteration SPI and a release burnup from them,
next to money-based EVM for the predictive branches. In a hybrid plan the two must not be summed
into one SPI. Report them separately.
Source: APG, §5.4.1 (APG-0137).

---

## 6. Change control and baselines

### 6.1 When a baseline may change

**What it is.** Before baselining, the plan can be edited freely. Once it is baselined (scope,
schedule and cost at least), it changes **only** through integrated change control: a change
request is raised, decided and, if approved, applied. Baselines are changed **from the last
baseline forward**; past performance is never rewritten, which protects the historical record.
The PMB is updated for approved changes in scope, schedule or cost estimates. When variances are
so severe that the baseline no longer gives a realistic basis, a change request can revise the
PMB. Likewise, when a schedule delay is severe, a new target schedule with forecast dates may be
needed. Releasing management reserve (§4.2) is one such approved change.

**Implication for alma-gantt.**
- **Baselines are immutable snapshots.** `alma baseline save <name>` creates one;
  `alma baseline save <name> --replace` is refused once actuals exist before the status date.
  Use instead:
- `alma baseline revise <name> --from <base> --change <CR-id> --reason "..."`. It creates a
  **new version** (`Informe 3 r2`) that copies the old baseline's time-phased values *up to the
  status date* and takes the current plan from there on. Past PV stays frozen.
- Data: `baselines[]: {name, version, created, change_ref, parent, status_date_at_creation}`. The
  `explain` output for a PV jump points at the revision and its change reference.
- One baseline is marked `active` (the PMB used for EVM). Others are kept for comparison
  (original vs current approved).
- Lint `L-BAS-06` (error): the active baseline's PV before its revision date differs from its
  parent's PV for the same periods (history was rewritten).
- Lint `L-BAS-07` (warning): a plan with recorded actuals has no baseline, or EVM points at a
  non-active baseline without `--baseline` explicitly given.
- Op-log (SPEC §8a): every baseline action is logged with who, when and the change reference.
- Source: PMBOK 6, §4.2 (PMBOK6-0164); PMBOK 6, §4.6.3.2 (PMBOK6-0250); PMBOK 6, §5.6.3.3
  (PMBOK6-0368); PMBOK 6, §6.5.3.1 (PMBOK6-0462); PMBOK 6, §6.6.3.5 (PMBOK6-0493); PMBOK 6,
  §7.3.2.3 (PMBOK6-0540); FEP06, p. 53 (FEP06-0053).

### 6.2 Change thresholds and the change board

**What it is.** A program or portfolio may require that changes above a set cost go to a change
control board. That threshold must be written into the project plan.

**Implication for alma-gantt.** `[cost] change_threshold = "500"`. `alma baseline revise`
records whether a revision exceeds it, and lint `L-BAS-09` (warning) flags revisions above the
threshold that have no `approved_by`.
Source: PMBOK 6, §4.2 (PMBOK6-0164).

---

## 7. Risk ↔ schedule and cost

### 7.1 Expected monetary value and sizing the reserve

**What it is.** The expected monetary value of a risk is probability × impact. For a single
project it is never the actual outcome: the risk either costs its full impact or nothing. But
the EMVs of several independent risks add up to a reasonable estimate of the total cost of
uncertainty, and that sum is the objective basis for the contingency reserve. Opportunities
enter with the opposite sign and reduce the reserve. Two common mistakes are adding raw impacts
instead of expected values, and ignoring correlated risks: risks with a common cause occur
together, so a simple sum understates them.

**Implication for alma-gantt.**
- An optional `[[risk]]` list: `{id, title, probability, impact_cost?, impact_time?, sign =
  threat|opportunity, owner, affects = [task ids], correlation_group?}`. Computed fields:
  `emv_cost` and `emv_time`.
- `alma risk summary` prints Σ EMV and compares it with the declared contingency (feeds
  `L-RISK-02`).
- Lint `L-RISK-03` (info): several risks share a `correlation_group` but are summed
  independently in the reserve basis.
- Source: FEP04, p. 46 (FEP04-0046); FEP04, p. 48 (FEP04-0048); FEP04, p. 55 (FEP04-0055).

### 7.2 Decision trees and the indifference point (optional report)

**What it is.** To compare two options, add each option's certain cost to the EMV of its risks.
Then solve for the probability at which the two options cost the same (the indifference point).
This shows how far off the probability estimate could be before the decision flips.

**Implication for alma-gantt.** This is out of scope for the engine. At most, an `alma risk
compare` helper over two alternative cost entries. Note it as a possible add-on, not a
requirement.
Source: FEP04, p. 50 (FEP04-0050); FEP04, p. 51 (FEP04-0051); PMBOK 6, §11.4.2.5
(PMBOK6-0930).

### 7.3 Quantitative schedule/cost risk analysis (Monte Carlo)

**What it is.** A simulation model combines the uncertainty of activities and risks. Uncertain
durations, costs or resource needs are given probability distributions (triangular, normal,
lognormal, beta, uniform, discrete). Risks that may or may not happen are modeled as
*probabilistic branches*: optional activities that occur with the risk's probability. Related
risks are linked by *correlation*. The model is run thousands of times with random inputs. The
outputs are:
- a histogram of outcomes and a cumulative probability S-curve, for example "10 % chance of
  finishing by May 13, 90 % by May 28";
- a **criticality index** per activity: the share of iterations in which it was on the critical
  path;
- a **sensitivity / tornado** chart ranking the inputs by correlation with the outcome.

FEP04 turns this into a bidding rule: commit to a high percentile (for example P80) and plan
internally at P50. The gap between the two is the schedule reserve, managed like the cost
reserve. Committing to the deterministic CPM date means accepting about a 50 % chance of being
late. The slides also note that a few hundred iterations in a spreadsheet already give a usable
curve.

**Implication for alma-gantt** (optional engine feature, SPEC "Later").
- Data needed **now**, so that plans are ready later: `duration_estimate = {o, m, p,
  dist = "pert"|"triangular"|"uniform"|...}` on tasks, cost three-points on cost lines, and
  risks with `affects` and `impact_time` (to generate probabilistic branches).
- `alma simulate --iterations 2000 --seed 42 --json` (a fixed seed keeps it deterministic, as
  SPEC §2 requires) outputs:
  - `finish_percentiles {p10, p50, p80, p90, p95}`, and the same for cost;
  - `criticality_index[task] ∈ [0,1]`;
  - `sensitivity[task|risk] = Spearman ρ` (for the tornado chart);
  - the probabilistic S-curve as CSV/SVG.
- `alma simulate --suggest-buffer --confidence 0.8` proposes a time-contingency buffer =
  P80 − deterministic finish, placed before the chosen milestone.
- Lint `L-RISK-04` (warning): a contract milestone's committed date is earlier than the P80
  (or a configured percentile) of the latest simulation.
- Performance: run the simulation on the WASM/native engine; 5k tasks × 1,000 iterations must
  stay interactive (well within budget if one CPM pass takes < 50 ms; parallelize with rayon on
  native).
- Source: PMBOK 6, §11.4.2.4 (PMBOK6-0927); PMBOK 6, §11.4.2.5 (PMBOK6-0928, PMBOK6-0929,
  PMBOK6-0930); PMBOK 6, §6.5.2.4 (PMBOK6-0456); FEP04, p. 53 (FEP04-0053); FEP04, p. 54
  (FEP04-0054); FEP04, p. 56 (FEP04-0056).

### 7.4 Reserve analysis during control

**What it is.** During execution, the project tracks how much contingency and management
reserve is left, whether it is still needed, and whether more is needed. Unused contingency for
risks that did not occur can be released. New risk analysis may justify asking for more.
Opportunities that save money can be added to contingency or returned as margin.

**Implication for alma-gantt.** `alma reserve status --json` shows per reserve: `initial`,
`drawn`, `released`, `remaining`, and linked open or closed risks. Lint `L-RISK-05` (info)
flags contingency still held for risks whose status is `closed`.
Source: PMBOK 6, §7.4.2.2 "Reserve analysis" (PMBOK6-0566, PMBOK6-0567); FEP04, p. 49
(FEP04-0049).

### 7.5 Schedule compression has a cost

**What it is.** There are two ways to compress a schedule:
- **Crashing** adds resources (overtime, extra people, expediting) to critical activities, at
  the lowest added cost. It only works on the critical path and raises cost.
- **Fast tracking** overlaps activities that were sequential. It raises risk, rework and
  coordination effort.

Negative total float, caused by an imposed date, signals that compression is needed.

**Implication for alma-gantt.**
- Lint (existing SPEC idea "negative float") `L-SCH-05` should suggest these two options in
  its `fix`: shortening critical tasks (crash) or converting an FS link to SS with a lag (fast
  track).
- An optional `task.crash = {min_duration, cost_per_day_saved}` lets a future
  `alma compress --target <date>` pick the cheapest critical tasks first. Mark it "later".
- Source: PMBOK 6, §6.5.2.6 (PMBOK6-0458, PMBOK6-0459); FEP02, p. 82 (FEP02-0082).

---

## 8. Estimation

### 8.1 The estimation chain: size → effort → duration → cost

**What it is.** These four quantities form a chain, and each step adds its own assumptions:
- **Size** is functional size (function points or use case points). It depends only on the
  requirements.
- **Effort** is person-hours. It depends on size and productivity.
- **Duration** is calendar time. It depends on effort and staffing.
- **Cost** is money. It depends on effort, rates and other inputs.

Size does not change when the team changes, but effort does. Skipping a step, for example going
straight from requirements to a price, makes the estimate impossible to defend.

**Implication for alma-gantt.** Model the chain explicitly so that each number can explain where
it came from (SPEC pillar 2 applied to money):
- An estimator (§8.5) produces **effort** on work packages.
- The engine turns effort into **duration** through assignments and calendars (§2.1).
- Rates turn effort into **cost** (§2.2).

The `explain` command should work for cost: `alma explain gate-build --cost` shows effort ×
rate + materials + fixed costs + contingency share.
Source: FEP03, p. 5 (FEP03-0005).

### 8.2 Analogous, parametric and bottom-up

**What it is.**
- **Analogous**: use the actual duration or cost of a similar past project, scaled for known
  differences. It is cheap and quick, less accurate, and works when information is scarce. It is
  only reliable if the projects are similar in fact, not just in appearance.
- **Parametric**: an algorithm on historical data. The simplest form is quantity × productivity:
  for example, 1,000 m of cable at 25 m/h = 40 h. It can be very accurate when the data and model
  are good.
- **Bottom-up**: decompose until each piece can be estimated with confidence, then aggregate.

Analogous and parametric models are most reliable when the historical data is accurate, the
parameters are quantifiable and the model scales. Lectura 1 classifies software estimation
methods (Boehm's seven families; a newer grouping into parametric models, expert-based methods
and others). Parametric models such as COCOMO 81 compute effort from size with coefficients per
project mode (organic, semi-detached, embedded), and the intermediate level multiplies by up to
15 cost drivers.

**Implication for alma-gantt.**
- `estimate.method = analogous{ref, scale}` stores the reference project and the scaling factor
  as text plus a number.
- `estimate.method = parametric{qty, unit, rate_per_unit}` computes work or duration =
  qty / productivity, or qty × hours per unit. Units come from the cost plan (§3.2).
- Bottom-up is the WBS roll-up itself. Lint `L-EST-06` (warning) flags a summary task whose
  *own* estimate (if entered as a top-down check) differs from the roll-up of its children by
  more than X %. This is the top-down vs bottom-up cross-check.
- COCOMO: not needed in core. Note it as a possible plugin estimator, like UCP below. Its
  coefficients come from published tables.
- Source: PMBOK 6, §6.4.2.2 (PMBOK6-0428); PMBOK 6, §6.4.2.3 (PMBOK6-0429); PMBOK 6, §6.4.2.5
  (PMBOK6-0431); PMBOK 6, §7.3.2.4 (PMBOK6-0541); Lectura 1, §1 (LECT1-0015, LECT1-0016,
  LECT1-0017); Lectura 1, §1.1.4 and §1-2 (LECT1-0036, LECT1-0037, LECT1-0038).

### 8.3 Three-point estimating: triangular and PERT (beta)

**What it is.** Instead of one number, estimate three: optimistic (O), most likely (M) and
pessimistic (P). The expected value depends on the distribution you assume:
- **Triangular:** E = (O + M + P) / 3. It weighs the three equally and is more conservative.
  PMBOK recommends it when historical data is lacking or the inputs are judgment.
- **Beta / PERT:** E = (O + 4M + P) / 6, with σ = (P − O) / 6.

Under a normal approximation, E ± 1σ covers about 68 %, ± 2σ about 95 % and ± 3σ about 99.7 %.
Committing to E alone means a 50 % chance of overrun. For bids with late-delivery penalties, the
slides suggest committing at about E + 2σ and covering the gap with reserve. FEP04 also says the
difference between the original single-point estimate and E belongs in the **contingency
reserve**, not in the package estimate, and that a project should choose one formula and use it
for all packages.

**Implication for alma-gantt.**
- `duration`, `work` and cost fields accept `{o, m, p}` with `project.three_point = "pert" |
  "triangular"` (one project-wide choice, as FEP04 recommends). The scheduled value is E, or a
  chosen percentile, configurable.
- `alma est show <task>` prints E, σ and the 1σ/2σ ranges.
- Option `--commit-percentile 0.95`: CPM on E for the internal plan, and a suggested time buffer
  = (z·σ_path) on the critical path. Summing σ along the path assumes independence; document
  that. Monte Carlo (§7.3) is the rigorous version.
- Lint `L-EST-02` (error): `o ≤ m ≤ p` is violated.
- Lint `L-EST-07` (info): the project mixes triangular and PERT on different tasks.
- Source: PMBOK 6, §6.4.2.4 (PMBOK6-0430); PMBOK 6, §7.2.2.5 (PMBOK6-0523); FEP02, p. 75
  (FEP02-0075); FEP02, p. 76 (FEP02-0076); FEP04, p. 52 (FEP04-0052).

### 8.4 Basis of estimates

**What it is.** Resource and cost estimates should document how they were produced: method,
sources, assumptions, constraints, range, confidence level and the risks considered.

**Implication for alma-gantt.** `estimate.basis` (free text) plus `estimate.confidence` (0-1 or a
label) and `estimate.range`. Lint `L-EST-01` (info, configurable to warning for bids): a work
package with cost > X % of BAC has no `basis`. Export a "basis of estimates" table (CSV/XLSX)
from these fields.
Source: PMBOK 6, §7.2.3.2 (PMBOK6-0529); PMBOK 6, §9.2.3.2 (PMBOK6-0695).

### 8.5 Use case points (UCP): full method

**What it is.** UCP (Karner, Objectory, 1993, with later refinements) estimates software
effort from a use-case model in five steps.

1. **Unadjusted actor weight (UAW).** Classify each *actor (a role, not a user)* by how it
   interacts:
   - simple = another system through an API → weight **1**;
   - average = another system through a protocol, or a person through a text interface →
     **2**;
   - complex = a person through a GUI → **3**.

   UAW = Σ count × weight. A timer or scheduler actor is a judgment call (often counted as
   simple). Pick one convention and state it.
2. **Unadjusted use case weight (UUCW).** Classify each use case by its number of
   *transactions*. A transaction is one atomic round trip from the actor to the system and back;
   alternative flows count only if they add new round trips.
   - simple = 1-3 transactions → **5**;
   - average = 4-7 → **10**;
   - complex = 8 or more → **15**.

   Some authors add a fourth level (20) or count analysis classes instead; never mix variants in
   one count. UUCP = UAW + UUCW.
3. **Technical complexity factor (TCF).** Rate 13 factors T1-T13 from 0 to 5. Their fixed
   weights are T1 distributed 2, T2 performance 1, T3 end-user efficiency 1, T4 complex
   processing 1, T5 reusable code 1, T6 easy install 0.5, T7 ease of use 0.5, T8 portability 2,
   T9 easy to change 1, T10 concurrency 1, T11 security 1, T12 third-party access 1, T13 training
   1 (Σ weights = 14).

   TFactor = Σ wᵢ·vᵢ ∈ [0, 70]. **TCF = 0.6 + 0.01 × TFactor**, which lies in [0.60, 1.30].
4. **Environmental factor (EF).** Rate 8 factors E1-E8 from 0 to 5. Their weights are E1
   familiarity with the process model 1.5, E2 application experience 0.5, E3 OO experience 1, E4
   lead analyst capability 0.5, E5 motivation 1, E6 requirements stability 2, E7 part-time staff
   −1, E8 difficult language −1. A high value is good for E1-E6 and bad for E7-E8, hence the
   negative weights.

   EFactor = Σ wᵢ·vᵢ. **EF = 1.4 − 0.03 × EFactor**, which lies in [0.42, 1.70].
5. **Adjusted points and effort.** **UCP = UUCP × TCF × EF**. **E = UCP × CF**, where CF is
   person-hours per UCP. Karner's original value is 20. The refinement counts the
   *unfavourable* environmental factors: E1-E6 rated below 3, plus E7-E8 rated above 3.
   - 0-2 unfavourable → CF = **20**;
   - 3-4 → CF = **28**;
   - 5 or more → the method declines to estimate; restructure the project, because the risk
     of failure is too high.

   CF is the most sensitive parameter: going from 20 to 28 adds 40 % effort. Industry values
   range widely (about 15-30), so the slides recommend computing with Karner's value *and* with
   the organization's own history.

**Distribution and interpretation.** The method's literature splits effort as analysis 10 %,
design 20 %, programming 40 %, testing 15 % and overhead 15 %. The values vary by organization.
There are two readings, and they differ by a factor of 2.5:
- **A:** E is the total effort and the percentages only split it;
- **B:** E is the *programming* effort, so total = E / 0.40.

FEP03 adopts B and says the classic texts follow it; Lectura 3's worked example also computes
the total as programming / 0.40. Either is defensible if declared. Lectura 2 also warns that UCP
covers only the functionality in the use cases: migration, training and similar work must be
added separately.

The combined TCF × EF range is about 0.25 to 2.21, much wider than function points' ±35 %, so
every factor value needs a one-line justification.

**Implication for alma-gantt** (an estimator a tool can offer).
- Command `alma estimate ucp --input ucp.toml [--apply-to <wbs-node>] --json`. Input:
  - `actors = [{name, class = simple|average|complex}]`;
  - `use_cases = [{name, transactions}]`. The class is derived from the transaction count, and
    a variant flag allows the 4-level or class-count scheme;
  - `technical = {T1 = {value, why}, …}`;
  - `environmental = {E1 = {value, why}, …}`;
  - `cf = "karner-rule" | <number>`;
  - `reading = "A" | "B"` (required, no default: force the user to declare it);
  - `distribution = {analysis = 0.10, design = 0.20, programming = 0.40, testing = 0.15,
    overhead = 0.15}` (overridable).
- Output: UAW, UUCW, UUCP, TFactor, TCF, EFactor, EF, UCP, the count of unfavourable factors,
  CF, E, the total, effort per phase, and, with `--rate`, cost. Also an **alternative scenario**
  at CF = 28 (as FEP03 recommends declaring).
- `--apply-to` creates or updates five child work packages (or sets `work` on existing ones)
  with `estimate.method = ucp` and `basis` pointing at the input file. The estimate stays
  traceable and re-runs when the use-case list changes.
- Diagnostics:
  - `L-UCP-01` (error): unfavourable-factor count ≥ 5 while an effort was applied (the method
    declines to estimate).
  - `L-UCP-02` (warning): a factor value has no `why`.
  - `L-UCP-03` (warning): all 13 technical factors equal 3 with no stated justification (TCF ≈
    1.02, looks unassessed).
  - `L-UCP-04` (info): a use case with far more than 8 transactions (probably two goals; split
    it).
  - `L-UCP-05` (error): `reading` not declared.
- Source: FEP03, p. 23-30 (FEP03-0023, FEP03-0024, FEP03-0026, FEP03-0027, FEP03-0028,
  FEP03-0029, FEP03-0030); FEP03, p. 37-43 (FEP03-0037, FEP03-0038, FEP03-0039, FEP03-0040,
  FEP03-0041, FEP03-0042, FEP03-0043); FEP03, p. 48-52 (FEP03-0048, FEP03-0049, FEP03-0050,
  FEP03-0051, FEP03-0052); FEP03, p. 69 (FEP03-0069); Lectura 2, §3.3 (LECT2-0079 to
  LECT2-0083); Lectura 3, p. 4-5 (LECT3-0015 to LECT3-0024); Lectura 3, p. 8 (LECT3-0040 to
  LECT3-0043).

---

## 9. Procurement and contracts (what a schedule tool should model)

### 9.1 Contract types and who carries cost risk

**What it is.** Contracts fall into three families:
- **fixed-price** (FFP, FPIF, FPEPA): the seller carries the cost risk;
- **cost-reimbursable** (CPFF, CPIF, CPAF, CPPC): the buyer carries it;
- **hybrids** (T&M, unit price, capacity-based agile contracts): the risk is shared or
  undefined.

Fixed price suits well-defined requirements. T&M suits staff augmentation and work whose
statement of work cannot be fixed quickly. With the same scope, the seller's margin behaves very
differently: under T&M the percentage margin stays the same when costs grow, while under FFP it
can collapse.

**Implication for alma-gantt.**
- An optional `[[contract]]` object: `{id, vendor, type = "FFP"|"FPIF"|"T&M"|"CPFF"|...,
  ceiling?, target_cost?, share_ratio?, fee?}`. Tasks or cost lines can be linked to it
  (`task.contract`).
- Engine effect: for `FFP`, the buyer-side cost of linked work is the contract price, distributed
  by payment milestones (§9.2), not resources × rates. For `T&M`, cost = actual hours × rate,
  with an optional ceiling. This keeps the buyer's S-curve honest.
- Lint `L-PROC-03` (warning): a T&M contract with no ceiling linked to work larger than X % of
  BAC.
- Source: FEP06, p. 23 (FEP06-0023); FEP06, p. 38 (FEP06-0038); PMBOK 6, §12.1.1.6
  (PMBOK6-1015, PMBOK6-1018).

### 9.2 Payment milestones and contract administration

**What it is.** After signature, contract administration covers kickoff, formal acceptance of
each milestone against its criteria, change control (no extra work before it is priced and
approved in writing), payments, vendor evaluation and closure. Payments are made against
**accepted milestones**, not elapsed time or reported effort. Guarantees and retentions back
compliance.

**Implication for alma-gantt.**
- Milestones get `payment = {amount | pct_of_contract, contract, retention_pct?}` and
  `acceptance = {criteria, accepted_on?}`.
- Cash-flow and S-curve reports place the payment at the milestone's (forecast or actual
  acceptance) date, net of retention. Retention is released at a closing milestone.
- Lint `L-PROC-02` (error): the payment milestones of a fixed-price contract do not add up to
  100 % of its price.
- Source: FEP06, p. 53 (FEP06-0053); FEP06, p. 97 (FEP06-0097); FEP06, p. 21 (FEP06-0021).

### 9.3 Procurement calendar: schedule backward from the need date

**What it is.** The procurement calendar is built **backward**: take the date the item is
needed, subtract the delivery lead time, then subtract the purchasing process time. It is never
built forward from today. Contract administration effort should also appear in the WBS with a
named owner.

**Implication for alma-gantt.**
- A procurement pattern: `alma proc add <id> --need <task> --lead 12w --process 4w`. It creates
  the chain *process (ALAP) → order milestone → delivery lead (elapsed time, no resources) →
  delivered milestone → FS to the needing task*. The process tasks get an ALAP constraint, so
  they sit as late as possible, and the float shows the real latest order date.
- Lint `L-PROC-01` (error): the latest order date computed from the chain is before the project
  status date or start (the item is already late).
- Lint `L-PROC-04` (info): a plan with contracts has no task for contract administration.
- Source: FEP06, p. 43 (FEP06-0043); FEP06, p. 54 (FEP06-0054).

---

## 10. Cash flow, NPV and IRR (export and report scope only)

### 10.1 Structure of a project cash flow

**What it is.** A cash flow is a table with one column per period (usually years in investment
appraisal, months for a project budget). There is a **period 0** column before operation starts,
which holds the *investment calendar*: all spending needed before start-up, whether it is
classed as investment or expense (insurance, salaries, rent during construction). Each column
holds the inflows and outflows of that period, plus any investment that must already be made for
the next period to happen.

The textbook orders the items in five steps:
1. taxable income and expenses (revenues, savings, salaries, inputs, rent);
2. non-cash expenses (depreciation, amortization, book value of assets sold), deducted only to
   compute tax;
3. tax on profit, giving net profit;
4. adding back the non-cash expenses;
5. non-taxable items (investments as outflows; salvage value as an inflow in the last period).

The investment calendar is often monthly, so the cost of capital tied up before start-up can be
added. The evaluation horizon should match the useful life; by unwritten convention it is often
10 years. The slides add a consistency rule for IT solutions: every architecture component needs
an investment line (year 0) and an operating-cost line (yearly), and every budget line must map
to a component.

**Implication for alma-gantt.** alma-gantt is not an appraisal tool, but its cost-loaded schedule
*is* the investment calendar and the project's spending profile. So:
- `alma export cashflow --period month|year --xlsx|--csv` with rows grouped by the cost
  categories of §3.1: investment (capex), operating costs (opex), administration, human
  resources (labor), materials and services, contingency. Optional rows: revenues/benefits and
  financing entered as plain `[[cashflow_item]] {period, amount, category, taxable?}` lines.
- Each column holds the period's net flow plus a **cumulative** row. Optionally a period-0
  column that sums everything before a chosen `operation_start` milestone.
- Tax/depreciation are **not** modeled in core. The export leaves rows for a spreadsheet to
  complete (template mode), rather than pretending to do accounting.
- Lint `L-CF-02` (info): a cost line has no category (it will be exported as "uncategorized").
- Source: Libro sin identificar, cap. 7, p. 1-5 (EVAL7-0001 to EVAL7-0008); FEP01, p. 283
  (FEP01-0283).

### 10.2 NPV and IRR

**What it is.**
- **NPV (VAN)** discounts every future cash flow at the required rate i and subtracts the
  investment at period 0. NPV > 0 means the project returns more than i after recovering the
  investment; NPV = 0 means exactly i; NPV < 0 means it falls short. It is the most widely
  accepted criterion.
- **IRR (TIR)** is the rate that makes NPV = 0. It gives the same accept/reject decision as NPV,
  but it does not rank projects of different size. When the cash flow changes sign several
  times, there can be as many IRRs as sign changes.
- **Discounted payback period** is the time until the investment is recovered *including* the
  cost of capital.
- **Benefit-cost ratio** = PV(benefits) / PV(costs). It adds nothing beyond NPV.

**Implication for alma-gantt.**
- `alma report finance --rate 0.10 [--per year|month]` computes NPV, IRR (with a root-finding
  method that detects multiple sign changes and then returns all roots found in a bracket, or
  `null` with a diagnostic) and discounted payback, from the exported cash flow. Use Decimal for
  money and f64 only inside the IRR solver, converting at the boundary.
- Monthly flows need a monthly rate: i_m = (1 + i_a)^(1/12) − 1.
- Lint `L-CF-01` (warning): IRR requested on a flow with more than one sign change.
- Source: Libro sin identificar, cap. 8, p. 3 (EVAL8-0003); cap. 8, p. 9-12 (EVAL8-0010,
  EVAL8-0011, EVAL8-0012, EVAL8-0013, EVAL8-0014); cap. 7, p. 4-5 (EVAL7-0006, EVAL7-0008).

---

## Appendix A. Consolidated lint rules

Severity: **E** error (fails `alma check`, exit 1), **W** warning, **I** info. All rules can be
configured in `[rules]` (SPEC §6).

| Code | Sev. | Checks | Source |
|---|---|---|---|
| L-RES-01 | E after level / W before | Resource demand > availability in some period; lists period, excess, competing tasks; fix `alma level` | PMBOK 6 §6.5.2.3; p. 704 |
| L-RES-02 | W | `level --mode smoothing` left conflicts unresolved (cannot resolve within free/total float) | PMBOK 6 §6.5.2.3 |
| L-RES-03 | E | Assignment outside the resource's availability window or calendar | PMBOK 6 §9.3.3.3 |
| L-RES-04 | E | Material assignment without quantity or unit; work assignment without units/work | PMBOK 6 §9.2.3.1 |
| L-RES-05 | W | Resource with assignments but no cost rate (cost silently 0) | PMBOK 6 §7.2.3.1 |
| L-RES-06 | I | Assignment to a generic role (RBS node) still unresolved at baseline | PMBOK 6 §9.2.3.3 |
| L-COST-01 | W | Cost element without category (breaks category reports and cash flow) | PMBOK 6 §7.2.3.1 |
| L-COST-02 | E | Work package under ≠ 1 control account, or control account with no work packages | PMBOK 6 §5.4.3.1; WBS PS §2.2.1 |
| L-COST-03 | W | Planning package still without activities inside the rolling-wave horizon | PMBOK 6 §5.4.3.1 |
| L-COST-04 | E | Cumulative planned spend > cumulative funding in some period | PMBOK 6 §7.3.2.5 |
| L-COST-05 | W | `bill_rate < cost_rate`, or bill basis with zero overhead and margin (pricing at cost) | FEP03 p. 54-55; FEP02 p. 90 |
| L-RISK-01 | W | Contingency reserve without basis or linked risks | FEP04 p. 48; PMBOK 6 §7.2.2.6 |
| L-RISK-02 | I | Cost contingency differs from Σ EMV of linked risks by > threshold | FEP04 p. 46-48 |
| L-RISK-03 | I | Correlated risks (same group) summed as independent | FEP04 p. 55; PMBOK 6 §11.4.2.4 |
| L-RISK-04 | W | Committed milestone date earlier than the configured percentile (default P80) of the last simulation | FEP04 p. 53; PMBOK 6 §6.5.2.4 |
| L-RISK-05 | I | Contingency still held for closed risks | PMBOK 6 §7.4.2.2 |
| L-BAS-06 | E | Revised baseline rewrites PV before its revision date (past performance changed) | PMBOK 6 §4.6.3.2 |
| L-BAS-07 | W | Actuals recorded with no baseline; or EVM against a non-active baseline implicitly | PMBOK 6 §4.2; §6.5.3.1 |
| L-BAS-08 | E | Management reserve and baseline changes do not balance | PMBOK 6 §7.3.2.3; §7.3.3.1 |
| L-BAS-09 | W | Baseline revision above the change threshold without `approved_by` | PMBOK 6 §4.2 |
| L-EVM-01 | W | CV%, SV%, CPI or SPI beyond the configured control thresholds at status date | PMBOK 6 §7.1.3.1; §6.1.3.1 |
| L-EVM-02 | E | EV > BAC for a component (percent > 100 or no budget) | PMBOK 6 §7.4.2.2 |
| L-EVM-03 | W | AC without EV/actual start, or EV without any AC | PMBOK 6 §7.4.2.2 |
| L-EVM-04 | E | EVM requested without status date or baseline | PMBOK 6 §7.4.2.2 |
| L-EVM-05 | W | TCPI(BAC) > threshold (default 1.1) while EAC still equals BAC | PMBOK 6 §7.4.2.3 |
| L-EVM-06 | I | Bottom-up EAC outside the range of statistical EACs | PMBOK 6 §7.4.2.2 |
| L-EVM-07 | W | Percent complete tracks elapsed duration exactly over ≥ 3 status dates (not physical) | PMBOK 6 §6.6.1.3 |
| L-EVM-08 | I | Long task (> 2 reporting periods) measured with 0/100 | PMBOK 6 §7.1.3.1; FEP02 p. 64 |
| L-EST-06 | W | Top-down estimate of a summary differs from the bottom-up roll-up by > X % | PMBOK 6 §6.4.2.5 |
| L-EST-02 | E | Three-point estimate violates o ≤ m ≤ p | PMBOK 6 §6.4.2.4 |
| L-EST-07 | I | Mixed PERT and triangular formulas in one project | FEP04 p. 52 |
| L-EST-08 | I | Effort-driven duration shortened by adding resources (assumes linear scaling) | PMBOK 6 §6.4 |
| L-EST-01 | I/W | Large work package (cost > X % of BAC) with no basis of estimate | PMBOK 6 §7.2.3.2 |
| L-UCP-01 | E | UCP: ≥ 5 unfavourable environmental factors but effort applied | FEP03 p. 49; Lectura 2 §3.3 |
| L-UCP-02 | W | UCP: factor value without justification | FEP03 p. 40, 37 |
| L-UCP-03 | W | UCP: all technical factors = 3 without justification | FEP03 p. 41 |
| L-UCP-04 | I | UCP: use case far above 8 transactions (split it) | FEP03 p. 29 |
| L-UCP-05 | E | UCP: reading A/B (total vs programming effort) not declared | FEP03 p. 52 |
| L-PROC-01 | E | Latest order date of a procurement chain already past | FEP06 p. 43 |
| L-PROC-02 | E | Payment milestones of a fixed-price contract ≠ 100 % of price | FEP06 p. 53 |
| L-PROC-03 | W | T&M contract without ceiling on large work | FEP06 p. 23, 38 |
| L-PROC-04 | I | Contracts exist but no contract-administration task in the WBS | FEP06 p. 54 |
| L-CF-01 | W | IRR requested on a cash flow with multiple sign changes | EVAL8 p. 11 |
| L-CF-02 | I | Cost line exported as uncategorized in the cash flow | EVAL7 p. 2-4 |
| (golden test) | — | Lag generates no resource demand or cost (not a lint rule) | FEP02 p. 69 |

---

## Appendix B. Formulas the engine and reports must implement

Notation: money is a `Decimal` in project currency (CUR). Time is measured in working hours (h)
unless stated. "cum" means cumulative up to the status date t_s. All EVM quantities are taken at
t_s, at any WBS level, and summed bottom-up. Indices are dimensionless.

### B.1 Resources and effort

| # | Formula | Variables and units |
|---|---|---|
| F-COST-01 | `duration_wd = work_h / Σᵢ (units_i × hours_per_day_i)` | work_h: person-hours; units_i: fraction of capacity; hours_per_day_i: h/day from resource i's calendar; result in working days, then placed on the calendar (intersection of task and resource calendars) |
| F-COST-02 | `work_h = duration_h × Σᵢ units_i` | fixed-duration tasks |
| F-COST-03 | `demand_r(p) = Σ_tasks units_{t,r} × working_h_{t,r}(p)` | demand of resource r in period p (h) |
| F-COST-04 | `over_r(p) = max(0, demand_r(p) − max_units_r(p) × available_h_r(p))` | over-allocation (h) |
| F-COST-05 | `rate_productive = cost_month / (nominal_h_month × productive_ratio)` | CUR/h; productive_ratio typically 0.75-0.85 |
| F-COST-06 | `bill_rate = rate_productive × (1 + overhead_pct) × (1 + margin_pct)` | CUR/h (sequential mark-ups as in the FEP02 example) |

### B.2 Cost

| # | Formula | Variables and units |
|---|---|---|
| F-COST-07 | `cost_task = Σ_work,equip (work_h × rate_h) + Σ_equip per_use + Σ_material (qty × unit_price) + fixed_cost` | CUR; rate per date from the rate table |
| F-COST-08 | `cost_period(p) = Σ_elements amount × share_p(profile, accrual)` | Σ_p share_p = 1 for each element |
| F-COST-09 | `WP = Σ activities + contingency_WP`; `CA = Σ WP`; `PMB = BAC = Σ CA (+ project-level contingency)` | CUR |
| F-COST-10 | `project_budget = PMB + MR` | MR = management reserve, CUR |
| F-COST-11 | `funding_required_total = PMB + MR`; check `cum_spend(p) ≤ cum_funding(p)` for all p | CUR |
| F-COST-12 | `S(p) = Σ_{q ≤ p} cost_period(q)` | cumulative curve (baseline → PV curve; current → forecast curve) |

### B.3 Earned value

| # | Formula | Variables and units |
|---|---|---|
| F-COST-13 | `PV(t) = S_baseline(t)` | CUR, cumulative; per-period PV = ΔS |
| F-COST-14 | `EV = Σ_tasks BAC_task × earned_pct_task`, with `earned_pct ≤ 1` | earned_pct from ev_method: percent; fixed {s, f}: s at actual start, s+f at actual finish; milestones: Σ weights of done / Σ weights; units: done/total; LOE: = PV share |
| F-COST-15 | `AC = Σ actuals up to t_s` | CUR, entered |
| F-COST-16 | `SV = EV − PV`; `SV% = SV / PV` | CUR; % |
| F-COST-17 | `CV = EV − AC`; `CV% = CV / EV` | CUR; % |
| F-COST-18 | `SPI = EV / PV` | dimensionless; null if PV = 0 |
| F-COST-19 | `CPI = EV / AC` | dimensionless; null if AC = 0 |
| F-COST-20 | `EAC_bottom_up = AC + ETC_manual` | CUR |
| F-COST-21 | `EAC_budget_rate = AC + (BAC − EV)` | CUR |
| F-COST-22 | `EAC_cpi = BAC / CPI` | CUR |
| F-COST-23 | `EAC_cpi_spi = AC + (BAC − EV) / (CPI × SPI)` | CUR |
| F-COST-24 | `EAC_weighted = AC + (BAC − EV) / (w_c·CPI + w_s·SPI)`, `w_c + w_s = 1` | CUR; interpretation of PMBOK's "weighted" variant |
| F-COST-25 | `ETC = EAC − AC` (statistical) or entered (bottom-up) | CUR |
| F-COST-26 | `VAC = BAC − EAC` | CUR |
| F-COST-27 | `TCPI_BAC = (BAC − EV) / (BAC − AC)` | dimensionless |
| F-COST-28 | `TCPI_EAC = (BAC − EV) / (EAC − AC)` | dimensionless |
| F-COST-29 | `SPI_agile = points_completed / points_planned`; `CPI_agile = value_completed / AC` | per iteration |
| F-COST-30 | `pct_complete_project = EV / BAC` | % |

### B.4 Estimation and risk

| # | Formula | Variables and units |
|---|---|---|
| F-COST-31 | `E_tri = (O + M + P) / 3` | same unit as inputs (h, days, CUR) |
| F-COST-32 | `E_pert = (O + 4M + P) / 6` | idem |
| F-COST-33 | `σ_pert = (P − O) / 6` | idem |
| F-COST-34 | `commit(z) = E + z·σ`, z = 1, 2, 3 ≈ 68 %, 95 %, 99.7 % | normal approximation |
| F-COST-35 | `σ_path = sqrt(Σ σᵢ²)` over the critical path | assumes independence (document) |
| F-COST-36 | `duration_param = quantity / productivity` (or `quantity × h_per_unit`) | e.g. m ÷ (m/h) = h |
| F-COST-37 | `EMV = p × impact` (opportunities with opposite sign); `contingency_basis = Σ EMV` | CUR or days |
| F-COST-38 | `reserve_pct = Σ EMV / direct_cost` | % (FEP04 sizing) |
| F-COST-39 | `criticality_i = (#iterations where i is critical) / N` | [0, 1] |
| F-COST-40 | `time_buffer = P_k − deterministic_finish` (k = 80 by default) | working days |
| F-COST-41 | `EV_decision = certain_cost + Σ p_j × impact_j`; indifference `p* : EV_A(p*) = EV_B` | CUR; optional |

### B.5 Use case points

| # | Formula | Variables and units |
|---|---|---|
| F-COST-42 | `UAW = 1·n_simple + 2·n_average + 3·n_complex` (actors) | points |
| F-COST-43 | `UUCW = 5·n_simple + 10·n_average + 15·n_complex` (use cases by 1-3 / 4-7 / ≥ 8 transactions) | points |
| F-COST-44 | `UUCP = UAW + UUCW` | points |
| F-COST-45 | `TFactor = Σ_{i=1..13} w_i·v_i`, v ∈ [0,5]; `TCF = 0.6 + 0.01·TFactor` | TCF ∈ [0.60, 1.30] |
| F-COST-46 | `EFactor = Σ_{i=1..8} w_i·v_i`; `EF = 1.4 − 0.03·EFactor` | EF ∈ [0.42, 1.70] |
| F-COST-47 | `UCP = UUCP × TCF × EF` | points |
| F-COST-48 | `n_unfav = #{E1..E6 : v < 3} + #{E7,E8 : v > 3}`; `CF = 20 if n_unfav ≤ 2; 28 if 3-4; undefined if ≥ 5` | h/UCP |
| F-COST-49 | `E = UCP × CF` | person-hours |
| F-COST-50 | Reading A: `total = E`; Reading B: `total = E / 0.40`; `effort_phase = total × share_phase` | person-hours; default shares 0.10/0.20/0.40/0.15/0.15 |
| F-COST-51 | `cost = total × bill_rate` | CUR |

### B.6 Cash flow and appraisal

| # | Formula | Variables and units |
|---|---|---|
| F-COST-52 | `net_t = inflows_t − outflows_t`; `cum_t = Σ_{k ≤ t} net_k` | CUR per period t (t = 0 is the investment calendar) |
| F-COST-53 | `PV(F_t) = F_t / (1 + i)^t` | i per period |
| F-COST-54 | `NPV = Σ_{t=0..n} net_t / (1 + i)^t` (with net_0 = −investment) | CUR |
| F-COST-55 | `IRR = r such that Σ_t net_t / (1 + r)^t = 0` | possibly multiple roots if net_t changes sign more than once |
| F-COST-56 | `i_month = (1 + i_year)^(1/12) − 1` | rate conversion |
| F-COST-57 | `FV_investment_calendar = Σ_t C_t (1 + i)^(n − t)` | capitalizes pre-start-up spending to period 0 |
| F-COST-58 | `discounted_payback = min T : Σ_{t ≤ T} net_t / (1 + i)^t ≥ 0` | periods |
| F-COST-59 | `BCR = PV(benefits) / PV(costs incl. investment)` | dimensionless (optional) |

---

## Appendix C. Glossary EN ↔ ES

| English | Español | Abbrev. |
|---|---|---|
| Actual cost | Costo real | AC |
| Analogous estimating | Estimación análoga | |
| Apportioned effort | Esfuerzo prorrateado | |
| Basis of estimates | Base de las estimaciones | |
| Beta (PERT) distribution | Distribución beta (PERT) | |
| Bottom-up estimating | Estimación ascendente | |
| Budget at completion | Presupuesto hasta la conclusión | BAC |
| Buffer | Amortiguador / colchón de cronograma | |
| Cash flow | Flujo de caja | |
| Change control board | Comité de control de cambios | CCB |
| Contingency reserve | Reserva para contingencias | |
| Control account | Cuenta de control | CA |
| Control threshold | Umbral de control | |
| Cost aggregation | Agregación de costos | |
| Cost baseline | Línea base de costos | |
| Cost performance index | Índice de desempeño del costo | CPI |
| Cost variance | Variación del costo | CV |
| Crashing | Intensificación | |
| Criticality index | Índice de criticidad | |
| Discount rate | Tasa de descuento | i |
| Earned value | Valor ganado | EV |
| Effort | Esfuerzo (horas-hombre) | |
| Estimate at completion | Estimación a la conclusión | EAC |
| Estimate to complete | Estimación hasta la conclusión | ETC |
| Expected monetary value | Valor monetario esperado | EMV / VME |
| Fast tracking | Ejecución rápida | |
| Funding limit reconciliation | Conciliación del límite de financiamiento | |
| Funding requirements | Requisitos de financiamiento | |
| Internal rate of return | Tasa interna de retorno | IRR / TIR |
| Investment calendar | Calendario de inversiones | |
| Level of effort | Nivel de esfuerzo | LOE |
| Management reserve | Reserva de gestión | MR |
| Net present value | Valor actual neto | NPV / VAN |
| Over-allocation | Sobreasignación | |
| Parametric estimating | Estimación paramétrica | |
| Payback period | Período de recuperación de la inversión | PRI |
| Percent complete | Porcentaje completado | |
| Performance measurement baseline | Línea base para la medición del desempeño | PMB |
| Planned value | Valor planificado | PV |
| Planning package | Paquete de planificación | |
| Productive hours | Horas productivas | |
| Project budget | Presupuesto del proyecto | |
| Resource breakdown structure | Estructura de desglose de recursos | RBS / EDR |
| Resource calendar | Calendario de recursos | |
| Resource histogram | Histograma de recursos | |
| Resource leveling | Nivelación de recursos | |
| Resource smoothing | Estabilización (alisamiento) de recursos | |
| Rough order of magnitude | Orden de magnitud aproximado | ROM |
| Salvage value | Valor de desecho | |
| Schedule baseline | Línea base del cronograma | |
| Schedule performance index | Índice de desempeño del cronograma | SPI |
| Schedule variance | Variación del cronograma | SV |
| Sell rate / bill rate | Tarifa de venta | |
| Sensitivity analysis (tornado) | Análisis de sensibilidad (diagrama de tornado) | |
| Status date / data date | Fecha de corte / fecha de estado | |
| Three-point estimating | Estimación por tres valores | |
| To-complete performance index | Índice de desempeño del trabajo por completar | TCPI |
| Use case points | Puntos de casos de uso | UCP |
| Unadjusted actor weight / use case weight | Peso de actores / de casos de uso sin ajustar | UAW / UUCW |
| Technical complexity factor | Factor de complejidad técnica | TCF |
| Environmental factor | Factor de ambiente | EF |
| Conversion (productivity) factor | Factor de conversión (horas por UCP) | CF |
| Variance at completion | Variación a la conclusión | VAC |
| Work package | Paquete de trabajo | WP |
| Working capital | Capital de trabajo | |
| Firm fixed price / Time and materials | Precio fijo cerrado / Tiempo y materiales | FFP / T&M |
| Cost plus fixed/incentive/award fee | Costo más honorario fijo/incentivo/premio | CPFF / CPIF / CPAF |

---

## Appendix D. Gaps: what SPEC.md misses or gets wrong, according to these sources

Ordered by impact on correctness.

1. **No reserves model, and the S-curve stack is undefined.** SPEC §7.5 has no contingency and
   no management reserve. According to the sources, contingency (time and cost) is *inside* the
   baselines and must be explicit, while management reserve is *outside* the baseline but inside
   the budget and funding, and releasing it changes the baseline. Without this split, BAC, PV,
   the S-curve and the "budget" are ambiguous, and a proposal's numbers cannot be defended.
   Add `[[reserve]]`, buffer tasks and the cost stack (§4.1-4.3). (PMBOK 6 §7.2.2.6, §7.3.2.3,
   §7.3.3.1, §6.4.2.6; FEP02 p. 77; FEP04 p. 48-49.)
2. **EVM is under-specified.** SPEC lists only PV, EV, AC, SPI and CPI. Missing:
   - BAC, CV, SV, VAC, ETC, all EAC variants and both TCPIs;
   - per-period vs cumulative series;
   - the EV cap (EV ≤ PV budget per component);
   - AC as *entered* data with matching definitions;
   - control thresholds;
   - and above all **how EV is measured**: EV techniques (0/100, 50/50, weighted milestones,
     units, physical %, LOE).

   Without `ev_method`, "percent complete" will default to elapsed time, which is not a physical
   measure. (PMBOK 6 §7.4.2.2-§7.4.2.3, §7.1.3.1, §6.1.3.1.)
3. **No control accounts or work packages.** SPEC's WBS has only `task | summary | milestone`.
   EVM and cost aggregation happen at the work-package and control-account level, a WP belongs
   to exactly one control account, and planning packages exist for rolling wave. Add `task.role`
   and `account_code`. (PMBOK 6 §5.4.3.1, §7.3.2.2; WBS PS §2.2.1.)
4. **Baseline semantics are too thin.** "Named snapshots" is not enough. Baselines change only
   through approved change and *only from the last baseline forward*: past PV must be preserved,
   with versions linked to a change reference, and one baseline marked active as the PMB. As
   written, `alma baseline save "Informe 3"` could overwrite history. Add `baseline revise`,
   versioning and lint `L-BAS-06`. (PMBOK 6 §4.2, §4.6.3.2, §5.6.3.3.)
5. **PV is mislabeled in §7.5.** SPEC computes "cumulative planned value … for the current
   schedule and each baseline". By definition PV is the *baseline* budget for scheduled work; a
   curve from the current schedule is a forecast. Rename it, and state in every curve which
   schedule (baseline n or current, leveled or unleveled) it comes from. (PMBOK 6 §7.4.2.2,
   §7.3.3.1.)
6. **The cost model is too simple.** §7.5 uses `units × rate × working time + fixed cost`. Missing:
   - material quantities with units and prices;
   - equipment per-use cost;
   - date-effective rate tables;
   - cost rate vs bill rate, with the productive-hours, overhead and margin chain;
   - cost categories (needed for cost reports and the cash-flow export);
   - currency/fx;
   - accrual timing for fixed costs (start, end, milestone);
   - non-flat work profiles.

   (PMBOK 6 §7.2.3.1, §7.1.3.1; FEP03 p. 54; FEP02 p. 90.)
7. **Leveling vs smoothing terms and their critical-path effect.** §7.4's "within float only"
   is smoothing and must respect *free* float as well as total float. "Allow end-date slip" is
   leveling, which usually changes the critical path; the engine must report that change. Use
   the standard names. (PMBOK 6 §6.5.2.3; FEP02 p. 82.)
8. **Resource model gaps.** There is no RBS (category, type, skills; role vs named person), no
   OBS link, no time-phased availability (one `max_units` is unrealistic), and `kind` lacks
   `equipment`. (PMBOK 6 §9.2.3.3, §9.3.3.3, §9.1.2.2, §X4.6.)
9. **Effort-driven scheduling is not specified.** SPEC says `duration | effort` but not how they
   relate. Add task types (fixed duration, fixed work, fixed units) and F-COST-01/F-COST-02 on resource
   calendars. Do not assume linearity silently when resources are added. (FEP02 p. 71; PMBOK 6
   §6.4.)
10. **Funding limits are absent.** There are no funding periods, no reconciliation check and no
    funding staircase on the S-curve. The sources treat reconciling spend with funding limits
    (by rescheduling with date constraints) as part of determining the budget. (PMBOK 6
    §7.3.2.5, §7.3.3.2.)
11. **Uncertainty data are missing from the model even though Monte Carlo is "Later".** Store
    three-point estimates (`o, m, p`, distribution) and risks (`probability`, `impact`,
    `affects`, `correlation_group`) now. Otherwise plans written today cannot be simulated later,
    and PERT expected durations, σ, P80 buffers and the criticality index have no inputs.
    (PMBOK 6 §6.4.2.4, §11.4.2.4-§11.4.2.5; FEP04 p. 52-54.)
12. **No status-date / progress-snapshot model.** M4 mentions a status date, but not:
    - a history of status snapshots, needed for trend analysis;
    - rescheduling incomplete work after the status date;
    - three date sets (baseline, current, actual);
    - float variance in `alma diff`.

    (PMBOK 6 §6.6.2.1, §6.6.1.3, §6.6.3.5.)
13. **Procurement and contracts are not modeled.** There is no backward (ALAP) procurement
    chain from the need date, no payment milestones (pay on accepted milestone, with
    retentions), and no contract type affecting how the buyer's cost is computed (FFP price vs
    T&M hours). These change both the S-curve and the cash flow. (FEP06 p. 23, 43, 53-54; PMBOK 6
    §12.1.1.6.)
14. **There is no estimation trail.** Pillar 2 ("explainable") covers dates only. The sources
    insist that money be traceable through size → effort → duration → cost and that estimates
    carry a basis, range and confidence. Add `estimate{method, basis, …}`, `alma explain
    --cost`, and the optional UCP estimator (§8.5) as a showcase of agent-friendly, reproducible
    estimation. (FEP03 p. 5, 55, 69; PMBOK 6 §7.2.3.2, §9.2.3.2.)
15. **The cash-flow export is not specified.** M2 mentions "XLSX export of costs per period",
    but a cash-flow export needs:
    - a period-0 (investment calendar) convention;
    - category rows (capex, opex, admin, HR);
    - a cumulative row;
    - optional inflows;
    - an NPV/IRR/payback report, with IRR's multiple-root caveat and annual-to-monthly rate
      conversion.

    (EVAL7 p. 2-5; EVAL8 p. 9-12.)
16. **Agile EVM is missing for hybrid plans.** Iterations exist, but there is no story-point
    SPI or burnup. The rule should be to report agile and predictive EVM separately rather than
    mixing them into one index. (APG §5.4.1.)

### Not found in the knowledge base (do not cite these sources for them)

- Exact percentage splits of 0/100, 50/50 or 20/80 EV rules, and the EV rule for LOE. Only the
  categories are named (PMBOK 6 §7.1.3.1, §6.1.3.1; FEP02 p. 64).
- Critical chain / feeding and project buffers. PMBOK 6, as indexed, does not cover them; the
  "buffer" idea here comes from contingency reserve plus FEP04's P80 − P50 rule.
- Earned schedule (ES/SPI(t)), which fixes SV ending at 0. It is not in these sources; consider
  it as an extension, cited elsewhere.
- The weighted EAC formula. PMBOK names the 80/20 and 50/50 weighting but prints no equation, so
  F-COST-24 is our interpretation.
