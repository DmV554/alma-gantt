# Plan file format reference (`*.alma.toml`), version 0

- **Status:** accepted (all decisions in §1, by the owner, 2026-10-29). Recorded in ADRs 0006–0008.
- **Date:** 2026-10-29
- **Supersedes:** the format fragments in `SPEC.md` §6 and §8, the field proposals
  scattered through `docs/knowledge/*.md`, and the shape of `examples/hybrid-terminal.alma.toml`.

This is the single reference for what a plan file may contain. Today, that information is split
across `SPEC.md` §6, §7.6 and §8, the two knowledge notes and the fixture, and those sources
disagree with each other (§11 lists every conflict and how this document resolves it). The parser
in `alma-format`, the JSON Schema printed by `alma schema`, and `alma guide` must all match this
file. A test enforces that (§10).

---

## 1. Decisions

Each item resolves a conflict or fills a gap. All are accepted; the ADR that records each one is
listed in §12.

| # | Decision | Rule |
|---|---|---|
| D1 | What `1d` means in working time | A fixed conversion: `1d` = `settings.schedule.hours_per_day` (default 8) working hours, `1w` = `days_per_week` days (default 5). `L-CAL-01` warns when a calendar's day has a different length. (MS Project semantics) |
| D2 | Duration grammar | `<number><unit>`, units `min h d w` (working) and `emin eh ed ew` (elapsed). No months. §4 |
| D3 | Keys the parser does not know | Error `E-FMT-02` with a "did you mean" suggestion. Free-form data goes under `custom`. Keys reserved for later milestones fail with `E-FMT-05`, not "unknown". §3 |
| D4 | WBS role vs scheduling shape | Two orthogonal fields. `kind` (scheduling shape) = `task \| milestone \| loe \| buffer`. `wbs_role` = `deliverable \| control_account \| work_package \| planning_package`. A summary is any node with children (it is never stored). §6.2 |
| D5 | Milestone fields | Keep SPEC §7.6.3: `class = contractual \| internal \| external`, `owner`, `acceptance`. Drop the notes' `mandatory`, `visibility` and `criterion` (they carry the same information) |
| D6 | Iterations | `[[cadence]]` (start, length, count) generates the iterations. A flat list of iterations is not stored. Releases are reserved for M4. §6.6 |
| D7 | Time reserve | `kind = "buffer"` with `reserve = "contingency" \| "management"`. Cost reserves (`[[reserve]]`) are reserved for M2 |
| D8 | Duration vs three-point estimate | Both can be stored. `duration` is the committed value; `estimate` supports it. With no `duration`, the engine uses tE. §6.3 |
| D9 | Effort | Field `effort`, scheduling type `sched_type = fixed_duration \| fixed_effort \| fixed_units` (SPEC's wording) |
| D10 | Assignment key | `assign` (as in SPEC §8 and the fixture), not `assignments` |
| D11 | IDs | Globally unique across every object type, `^[a-z0-9][a-z0-9-]{0,62}$`. §3.2 |
| D12 | Diagnostic codes | Hard errors are `E-<AREA>-nn` (not configurable); lint rules are `L-<AREA>-nn`. Collisions between the notes are renumbered. §9 |
| D13 | Formula ids | `F-SCH-nn` (scheduling note) and `F-COST-nn` (cost note) instead of two sequences that both start at `F1`/`F-01`. Test names become `f_sch_07_total_float_ss` |
| D14 | Baseline storage | Baseline *metadata* goes in the plan; snapshots (frozen dates and costs) go in a sidecar `.alma/baselines/<id>.toml`, so ADR 0003 holds and plan diffs stay small. Reserved for M2 |
| D15 | Format version | `[project] format = 0`. Once at 1, adding accepted keys is non-breaking, and changing the meaning of one requires a new version |
| D16 | When the fixture "lints cleanly" | `alma check` exits 0 with zero `error` and zero `warn`; `info` is allowed. Any rule turned off in the fixture carries a comment explaining why |

---

## 2. Scope tiers

Not every field has an effect in M0. Each key belongs to one tier:

| Tier | Meaning | Parser behavior |
|---|---|---|
| **A** | Used by the M0 engine or lint | Parsed, validated and used |
| **B** | Accepted now, used by a later milestone | Parsed, validated (types, references) and preserved; no effect on dates |
| **R** | Reserved name for a later milestone | Rejected with `E-FMT-05` ("`<key>` is reserved for M<n>, not supported yet") |

Why reserve instead of half-implementing: accepting a key later is a non-breaking change, while
changing what an already accepted key means is a breaking one. Tier B is limited to what the
fixture already uses, plus what SPEC §7.6.4 asks to capture from M0 (three-point estimates).

---

## 3. General rules

### 3.1 File
- One project per file, named `<name>.alma.toml`, UTF-8, TOML 1.0.
- Top-level items in this order (`alma fmt` enforces it; reading accepts any order):
  `[project]`, `[settings.*]`, `[rules.*]`, `[[calendar]]`, `[[resource]]`, `[[cadence]]`,
  `[[task]]`.
- `[[task]]` tables appear in WBS order (depth-first, siblings in display order) and are flat:
  hierarchy comes from `parent`, never from nested tables. The engine never infers sequence from
  file order.
- Keys inside a table follow the canonical order of the tables below. Reading accepts any order;
  CLI writes and `alma fmt` put keys in canonical order and keep comments attached to their key.
- Computed values (ES/EF/LS/LF, float, costs, WBS codes) are never written (ADR 0003).

### 3.2 IDs
- Pattern `^[a-z0-9][a-z0-9-]{0,62}$`: lowercase, digits and hyphens, at most 63 characters.
- Unique across all object types (tasks, resources, calendars, cadences, generated iterations), so
  `alma show <id>` is never ambiguous.
- IDs never change meaning; renaming an ID is a command (`alma task rename`) that rewrites every
  reference in the same transaction.

### 3.3 Values
| Value | TOML type | Rule |
|---|---|---|
| Date | local date `2027-01-04` | ISO 8601. Datetimes are not accepted in v0 |
| Duration, lag | string `"15d"` | Grammar in §4 |
| Money | string `"4.2"` | Decimal, parsed with `rust_decimal`. A TOML float is an error (`E-FMT-03`) so money never goes through `f64` |
| Units, ratios | integer or float | At most 4 decimal places, converted to `Decimal` exactly |
| Enum | string | Lowercase, from the listed values |

### 3.4 Unknown and free-form keys
- An unknown key is `E-FMT-02`, with the closest known key as the fix hint (edit distance ≤ 2).
  This catches agent typos (`duraton`) that a lenient parser would silently ignore.
- `custom = { ... }` on tasks and resources holds arbitrary user fields (strings, numbers, booleans,
  dates, arrays of these). alma preserves them, can filter and group by them, and never interprets them.

---

## 4. Durations and lags

```
duration  = [ "-" ] number unit          ; "-" only allowed in lags
number    = digits [ "." 1*2digit ]      ; at most two decimals: "0.5d", "1.25h"
unit      = "min" | "h" | "d" | "w"      ; working time
          | "emin" | "eh" | "ed" | "ew"  ; elapsed (calendar) time
```

- Working units convert with `settings.schedule`: `1d = hours_per_day h`, `1w = days_per_week d`
  (D1). Elapsed units are wall-clock time: `1ed = 24h`, `1ew = 7ed`.
- Internally every duration is an integer number of minutes. A literal that is not a whole number
  of minutes is `E-FMT-04` (`"0.33h"`).
- No months or years: their length in working time depends on the date, which would make a
  duration ambiguous. Use `w` or `d`.
- `"0d"` is valid (milestones). A negative task duration is `E-FMT-04`; a negative lag is a lead
  (`L-DEP-06`).
- Lags are measured on the project calendar unless `settings.schedule.lag_calendar` says
  otherwise (`project | predecessor | successor`; the engine ADR defines the semantics).

---

## 5. `[project]`, `[settings]`, `[rules]`

### `[project]`
| Key | Tier | Type | Default | Meaning |
|---|---|---|---|---|
| `format` | A | integer | required | Format version, `0` for this document (D15) |
| `name` | A | string | required | Project name |
| `start` | A | date | required | Project start; the forward pass anchor |
| `calendar` | A | calendar id | built-in `standard` (Mon–Fri, 09:00–13:00 and 14:00–18:00) | Default calendar |
| `deadline` | A | date | none | Project deadline; the backward pass anchor when `late_anchor = "deadline"` |
| `currency` | B | string | none | Currency label for money values (`"UF"`, `"USD"`) |
| `description` | B | string | none | Narrative, exported with reports |
| `status_date`, `estimate_class`, `fx` | R (M4/M2) | | | |

### `[settings.schedule]`
| Key | Tier | Type | Default | Meaning |
|---|---|---|---|---|
| `hours_per_day` | A | number | `8` | Working hours in `1d` (D1) |
| `days_per_week` | A | number | `5` | Working days in `1w` |
| `late_anchor` | A | `early_finish \| deadline` | `early_finish` | What the backward pass starts from |
| `critical_definition` | A | `float_threshold \| longest_path` | `float_threshold` | |
| `critical_threshold` | A | duration | `"0d"` | Total float at or below this is critical |
| `near_critical` | A | duration | `"5d"` | Threshold for near-critical paths |
| `lag_calendar` | A | `project \| predecessor \| successor` | `project` | Calendar that lags are counted on |
| `phase` | A | `draft \| preliminary \| baselined` | `draft` | Lint severities can depend on it (e.g. `L-SCH-05`) |
| `reporting_period` | B | `week \| month` | `month` | Used by `L-WBS-06` and later by EVM |
| `rolling_wave_horizon` | B | duration | none | Used by `L-WBS-11` |
| `three_point` | A | `beta \| triangular` | `beta` | Formula for tE (F-SCH-03/04) |
| `leveling` | A | `off \| smoothing \| leveling` | `off` | How the scheduler resolves resource over-allocation (ADR 0011). `smoothing` delays tasks within their free float only; `leveling` may move the finish date |
| `progress_logic`, `agile_drives`, `control_threshold` | R (M4) | | | |

`[settings.cost]` is reserved (R, M2).

### `[rules]`
```toml
[rules.severity]            # override the default severity of any lint rule
"L-WBS-05" = "off"          # off | info | warn | error

[rules.params]              # rule parameters, names from the rule registry
max_lag = "10d"
```
- Only `L-*` codes can be configured. `E-*` codes are always errors (§9).
- A single node can silence a rule with `allow = ["L-DEP-02"]` (task key, tier A). Prefer
  `allow` over turning a rule off for the whole plan.

---

## 6. Objects

### 6.1 `[[calendar]]` (tier A)
| Key | Type | Default | Meaning |
|---|---|---|---|
| `id` | id | required | |
| `name` | string | id | |
| `workdays` | array of `mon..sun` | `["mon","tue","wed","thu","fri"]` | Working weekdays |
| `hours` | string or array of `"HH:MM-HH:MM"` | `["09:00-13:00","14:00-18:00"]` | Working intervals per day: sorted, non-overlapping, within one day |
| `exceptions` | array of tables | `[]` | `{ name, from, to, working = false, hours? }`; `from`/`to` inclusive. With `working = true`, it is a working period (optionally with its own `hours`) |

Per-weekday hours and per-resource exceptions are reserved for M2 (keys `week`, `resource`).

### 6.2 `[[task]]`: identity, WBS and dictionary
| # | Key | Tier | Type | Meaning |
|---|---|---|---|---|
| 1 | `id` | A | id | Required |
| 2 | `parent` | A | task id | Omitted at WBS level 1 |
| 3 | `name` | A | string | Required |
| 4 | `kind` | A | `task \| milestone \| loe \| buffer` | Scheduling shape. Default `task`. A node with children is a summary; `kind` on a summary is `E-FMT-03` |
| 5 | `wbs_role` | A | `deliverable \| control_account \| work_package \| planning_package` | Scope meaning (D4). Omitted on activities. A leaf `work_package` with a `duration` is valid: it is scheduled directly, with no activities under it |
| 6 | `approach` | A | `predictive \| iterative \| flow` | Inherited by descendants; the project default is `predictive`. Changes which lint rules apply |
| 7 | `class` | A | `contractual \| internal \| external` | Milestones only (D5). `external` = an event owned by a third party |
| 8 | `owner` | A | string | Accountable person. If it matches a resource id it links to it; otherwise it is a name |
| 9 | `description` | B | string | WBS dictionary |
| 10 | `deliverable` | B | string | WBS dictionary |
| 11 | `acceptance` | A | string | Acceptance criterion (milestones and work packages) |
| 12 | `excludes` | B | string | WBS dictionary: what is out of scope |
| 13 | `assumptions` | B | array of strings | |
| 14 | `references` | B | array of strings | Requirement ids, documents |

### 6.3 `[[task]]`: scheduling
| # | Key | Tier | Type | Meaning |
|---|---|---|---|---|
| 15 | `sched_type` | A | `fixed_duration \| fixed_effort \| fixed_units` | Default `fixed_duration` (D9) |
| 16 | `duration` | A | duration | Working or elapsed. Milestones: absent or `"0d"` |
| 17 | `effort` | A | duration (`h`/`d`) | Person-time. With `fixed_effort`, duration = F-SCH-01 |
| 18 | `estimate` | A | table | `{ o, m, p, dist?, basis?, confidence? }` (durations; `dist` defaults to `settings.schedule.three_point`). With no `duration`, the engine uses tE (D8). Both stored → `duration` wins; `L-EST-09` (info) when it falls outside [o, p] |
| 19 | `calendar` | A | calendar id | Overrides the project calendar |
| 20 | `constraint` | A | table | `{ type, date }`, `type` ∈ `ASAP ALAP SNET SNLT FNET FNLT MSO MFO`. A start date means the first working minute of that day; a finish date means the last working minute of that day |
| 21 | `deadline` | A | date | Soft: affects late dates and float, never moves the task |
| 22 | `spans` | A | table | `kind = "loe"` only: `{ from = <id>, to = <id> }`; dates are derived from those tasks |
| 23 | `reserve` | A | `contingency \| management` | `kind = "buffer"` only (D7) |
| 24 | `deps` | A | array of tables | §6.4 |

### 6.4 Dependencies (`deps`, tier A)
`deps` lives on the successor and lists its predecessors.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `pred` | task id | required | Predecessor |
| `type` | `FS \| SS \| FF \| SF` | `FS` | |
| `lag` | duration | `"0d"` | Negative = lead |
| `nature` | `mandatory \| discretionary` | `mandatory` | Fast-tracking only touches discretionary links |
| `scope` | `internal \| external` | `internal` | |
| `rationale` | string | none | Expected for any lag, lead, discretionary or external link (`L-DEP-07/08/12`) |

### 6.5 `[[task]]`: resources, cost and metadata
| # | Key | Tier | Type | Meaning |
|---|---|---|---|---|
| 25 | `assign` | A/B | array of `{ resource, units }` | `units` is a fraction of capacity (1 = full time). Used in M0 only by `fixed_effort`/`fixed_units`; histogram and leveling in M2 |
| 26 | `fixed_cost` | B | table | `{ amount, accrue = start \| prorated \| end, category? }` |
| 27 | `allow` | A | array of `L-*` codes | Silences those lint rules on this node |
| 28 | `tags` | B | array of strings | |
| 29 | `custom` | B | table | Free-form fields (§3.4) |

Reserved on tasks (R): `actual_start`, `actual_finish`, `remaining`, `physical_pct`,
`progress_rule`, `ev_method` (M4); `points`, `iteration`, `kind = story \| epic` (M4); `crash`,
`overlap` (M3+); `risk_ref`, `payment`, `payment_ref`, `requirement_refs`, `contract`,
`account_code`, `org_unit`, `effort_type`, `apportion_to`, `location` (M2+).

### 6.6 `[[resource]]` (tier B)
| Key | Type | Default | Meaning |
|---|---|---|---|
| `id` | id | required | |
| `name` | string | id | |
| `kind` | `work \| equipment \| material \| cost` | `work` | |
| `rate` | table | none | `{ amount, per = hour \| day \| week \| month \| use }` |
| `max_units` | number | `1` | Capacity, for leveling (M2) |
| `calendar` | calendar id | project calendar | |

Reserved (R, M2): `parent`, `category`, `skills`, `org_unit`, `availability`,
`available_from`, `available_to`, `rates`, `bill_rate`. Assignment keys `work`, `quantity`,
`profile` are reserved as well.

### 6.7 `[[cadence]]` (tier B)
| Key | Type | Default | Meaning |
|---|---|---|---|
| `id` | id | required | |
| `name` | string | id | Label of the sprint band |
| `prefix` | string | `id` | Generated iteration ids are `<prefix><nn>` (`s01`…), zero-padded to two digits (three if `count > 99`) |
| `start` | date | required | |
| `length` | duration | required | Working time on `calendar` |
| `count` | integer ≥ 1 | required | |
| `calendar` | calendar id | project calendar | |

Generated iteration ids join the global id namespace. Per-iteration overrides, `[[release]]`,
capacity and velocity are reserved (R, M4).

### 6.8 Other reserved top-level items (R)
`[[release]]` (M4), `[[reserve]]` (M2), `[[risk]]` (M2), `[[funding]]` (M2), `[[contract]]` (M3),
`[[scenario]]` (M2), `[[cashflow_item]]` (M2), `[[baseline]]` (M2, D14), `[ucp]` (later).

---

## 7. Cross-field rules enforced by the parser

These are `E-*` errors because the plan cannot be scheduled with them; everything else is lint.

- Every reference (`parent`, `pred`, `calendar`, `resource`, `spans.*`) resolves (`E-REF-03`).
- `parent` links form a forest (`E-REF-04`); dependencies form a DAG (`E-DEP-01`, reported with the
  cycle path).
- `spans` only with `kind = "loe"`, `reserve` only with `kind = "buffer"`, `class` only with
  `kind = "milestone"` (`E-FMT-03`).
- A calendar has at least one working minute per week (`E-CAL-01`).

---

## 8. Complete minimal example

```toml
[project]
format = 0
name = "Example"
start = 2027-01-04

[[task]]
id = "design"
name = "Design"
duration = "10d"

[[task]]
id = "build"
name = "Build"
duration = "15d"
deps = [{ pred = "design" }]

[[task]]
id = "done"
name = "Delivered"
kind = "milestone"
deps = [{ pred = "build" }]
```

---

## 9. Diagnostics

Every diagnostic has this shape (JSON shown):

```json
{ "code": "E-DEP-01", "severity": "error", "message": "...", "ids": ["a", "b", "a"],
  "span": { "line": 42, "col": 1 }, "fix": "alma dep rm b a" }
```

`span` is present when the diagnostic points at the file; `fix` only when the fix is safe to run
blind.

### 9.1 Code scheme (D12)
- `E-<AREA>-nn`: the file is invalid or cannot be scheduled. Always `error`, not configurable.
  The CLI exits with code 1.
- `L-<AREA>-nn`: lint rules from the knowledge notes, with configurable severity (§5).
- Codes are stable: a removed rule keeps its number retired, and a number is never reused.

### 9.2 Hard errors in M0
| Code | Meaning |
|---|---|
| `E-FMT-01` | TOML syntax error |
| `E-FMT-02` | Unknown key (fix hint: closest known key) |
| `E-FMT-03` | Wrong type, value or key combination |
| `E-FMT-04` | Invalid duration or lag literal |
| `E-FMT-05` | Key reserved for a later milestone |
| `E-FMT-06` | Missing or unsupported `format` version |
| `E-REF-01` | Duplicate id |
| `E-REF-02` | Id does not match the pattern |
| `E-REF-03` | Reference to an unknown id |
| `E-REF-04` | `parent` cycle |
| `E-DEP-01` | Dependency cycle (`SPEC.md` §8a's `E012`) |
| `E-DEP-02` | Task depends on itself or on one of its ancestors/descendants |
| `E-CAL-01` | Calendar with no working time |
| `E-CAL-02` | Invalid or overlapping `hours` intervals |
| `E-OPS-01` | A command adds an id that exists with a different definition (use `--upsert`) |
| `E-OPS-02` | A command refers to a task, link or assignment that does not exist |
| `E-OPS-03` | A removal would leave dangling links or children (use `--cascade`) |
| `E-OPS-04` | Invalid command argument, or the plan already has errors |
| `E-CLI-01` | Invalid invocation or unreadable file (exit code 2) |

New lint rule introduced by this document: `L-CAL-01` (warn), a calendar whose working day is not
`hours_per_day` long (D1).

### 9.3 Renumbering of the knowledge notes (to apply to the notes on acceptance)
The two notes chose codes independently. The scheduling note's numbers stay; the cost note's
colliding codes move:

| Cost note | Becomes | Reason |
|---|---|---|
| `L-EST-01` (top-down vs bottom-up) | `L-EST-06` | Collides with "estimate without basis" |
| `L-EST-02` (o ≤ m ≤ p) | `L-EST-02` | Same rule in both notes; merged |
| `L-EST-03` (mixed PERT/triangular) | `L-EST-07` | Collision |
| `L-EST-04` (effort-driven linear scaling) | `L-EST-08` | Collision |
| `L-EST-05` (large WP without basis) | merged into `L-EST-01` | Same check; becomes a parameter |
| `L-EST-06..10` (use case points) | `L-UCP-01..05` | Their own area |
| `L-BASE-01..04` | `L-BAS-06..09` | One area name, `BAS` |
| `L-SCHED-lag` | removed | It is a golden test, not a rule |
| (scheduling note) `L-DEP-06a/b/c` | `L-DEP-06`, `L-DEP-11`, `L-DEP-12` | Codes are two digits |
| (new, D8) | `L-EST-09` | Duration outside [o, p] |

Formulas (D13): scheduling note `F-01..` → `F-SCH-01..`; cost note `F1..` → `F-COST-01..`.

---

## 10. Keeping this document true

- `alma-format` keeps a key registry (key, tier, type, default). A test parses the field tables in
  this file and fails if they differ from the registry, so the document cannot drift from the code.
- `alma schema format` prints the JSON Schema generated from the same registry.
- Every `E-*` code has a test that triggers it; every example in this file is parsed by a test.

---

## 11. Conflicts resolved

| Topic | Sources in conflict | Resolution |
|---|---|---|
| Iterations | SPEC §6/§8 `start/end`; fixture `length/repeat`; scheduling note `[[cadence]]` | `[[cadence]]` (D6) |
| WBS roles | SPEC §7.6 (4 roles + activity kinds); scheduling note 1.6 (6 roles incl. `activity`, `milestone`); cost note 3.4 (`role`) | `wbs_role` with 4 values + `kind` (D4) |
| Milestone fields | SPEC §7.6 `class`; scheduling note `mandatory/visibility/criterion`; cost note `acceptance = {criteria, accepted_on}` | SPEC's `class`, `owner`, `acceptance` as string (D5) |
| Time reserve | scheduling note `kind = "reserve"`; cost note `kind = "buffer"` + `[[reserve]]` | `buffer` for time, `[[reserve]]` for cost (D7) |
| Scheduling type | scheduling note `sched_type`/`fixed_effort`; cost note `task.type`/`fixed_work` | `sched_type`, `fixed_effort`, key `effort` (D9) |
| Assignments | SPEC §6 `assignments[]`; SPEC §8 and fixture `assign` | `assign` (D10) |
| `fixed_cost` | SPEC §6 scalar; fixture table | Table (§6.5) |
| Settings location | SPEC §6 `settings` in project; notes `[settings.schedule]`, `[cost]` | `[project]` for identity, `[settings.schedule]`, `[settings.cost]` |
| Summary dependencies | SPEC §7.2 expand; SPEC §7.6 lint | Allowed, `L-DEP-09` warns, no silent expansion |
| `duration` + `estimate` | Scheduling note: store only o/m/p; fixture stores both | Both allowed (D8) |
| Lint and formula ids | Collisions between notes | §9.3 |
| Cycle code | SPEC §8a `E012` | `E-DEP-01` |

---

## 12. Follow-up changes

Applied in the same change that accepted this document:

1. **ADRs:** 0005 `chrono` vs `jiff` (already planned, independent of this document);
   0006 durations and working time (D1, D2); 0007 plan format v0 (D3–D11, D14, D15);
   0008 diagnostic and formula codes (D12, D13).
2. **`SPEC.md`** (with the owner's agreement): §6 and §8 point here and use the new field names;
   §7.2 drops the silent summary expansion; §8a `E012` → `E-DEP-01`; §10.3 and §14 "ADR-001" →
   ADR 0005.
3. **`CLAUDE.md`:** test-name example `f_07_total_float_ss` → `f_sch_07_total_float_ss`.
4. **Knowledge notes:** apply the renumbering in §9.3.
5. **Fixture `examples/hybrid-terminal.alma.toml`:**
   - add `format = 0`;
   - calendar `hours = ["09:00-13:00", "14:00-18:00"]` (8 h, D1);
   - `[[iteration]]` → `[[cadence]] id = "sprints", prefix = "s", start, length, count = 12`;
   - `platform` becomes a `control_account` with two work packages (`platform-env`,
     `platform-cicd`); successors link to those, not to the summary. Without that,
     `platform` is a work package outside any control account (`L-COST-02`, error);
   - replace the 20-day lag on `gate-ocr` with an activity `gate-ocr-procure` (`L-DEP-11`: model a
     long wait as owned work);
   - add `owner` to every work package and `owner` + `acceptance` to the contractual milestones
     (`L-WBS-04`, `L-SCH-02`);
   - `[rules.severity]`: turn off `L-WBS-05` (the 8/80-hour work package rule does not fit a
     nine-month, multi-team plan) and `L-WBS-06`, each with a comment giving the reason (D16).
