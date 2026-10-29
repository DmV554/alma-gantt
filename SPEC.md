# alma-gantt — Product and technical specification

**Status:** draft v0.2 (2026-10-29; v0.2 points the format at `docs/format.md`, ADRs 0005–0008) · **Owner:** Daniel Miranda · **Audience:** the agent (or person) that builds it.

This document says *what* to build, *why*, and *with what*. It is the single source of truth for the
first milestones. When something here is wrong or ambiguous, fix this document first (with a short
ADR in `docs/adr/`), then the code.

---

## 1. Problem

Planning tools for Gantt charts fall in three camps, and all disappoint:

- **SaaS tools (GanttPRO, TeamGantt, Monday, etc.):** watermarks or paywalls on export, data locked in
  a proprietary cloud, poor PDF output, no way to version a plan next to the documents it belongs to.
- **Open-source desktop tools (GanttProject, ProjectLibre):** free, but dated UX, odd file formats,
  weak printing (huge PNGs, unreadable small fonts), no hybrid agile support, hard to automate.
- **Plan-as-code tools (TaskJuggler, Mermaid gantt, utf8proj):** versionable and scriptable, but no
  real interactive editor, so non-programmers on the team cannot use them.

The trigger for this project: a university team (9 people) had to deliver a project proposal with a
Gantt that has real dependencies, zero-duration milestones, visible Scrum sprints, a detailed
go-live, resource leveling, a cost-loaded **S-curve**, and figures that print legibly (≥ 9 pt) in a
LaTeX document. No free tool did all of that without friction.

## 2. Vision

> The most robust open-source Gantt tool: a scheduling engine you can trust, a plan that is a
> plain-text file in git, exports that print perfectly, and an interface built so that **AI agents
> do most of the work** while humans review and edit by hand whenever they want. Free, no
> watermarks, ever.

Four pillars:

0. **Agent-first, human-friendly.** The primary author of a plan is an LLM agent (Claude Code or
   similar) working granularly: add a task, link it, assign it, check, explain, render, repeat. Every
   operation the GUI can do is also a CLI/MCP command with structured input and output. Humans get
   the same plan in a polished editor that live-reloads what the agent changes. See §8a.

1. **Plan as a file.** The plan is a human-readable, diff-friendly text file. The GUI edits it; git
   versions it; a CLI checks it in CI. Nothing lives only in an app database.
2. **An engine you can explain.** Every computed date can answer "why is this here?" (which
   dependency, calendar, constraint or leveling decision pushed it). Deterministic: same input, same
   output, on every machine.
3. **Print-grade output.** Vector PDF/SVG with a minimum font size check, multi-page tiling with
   repeated headers, page sizes from A4 to A0, ready to `\includegraphics` in LaTeX or Typst.

## 3. Users and core jobs

| User | Job to be done |
|---|---|
| Project manager in a hybrid project | Build a predictive plan (contract milestones, procurement, field work) with Scrum sprints inside it, level resources and produce a baseline |
| Student team / consultancy writing a proposal | Produce a defensible Gantt, WBS, S-curve and cost table and embed them in a document, with all numbers traceable |
| Developer / tech lead | Keep the plan in the repo, review changes as diffs in PRs, fail CI when the plan breaks a rule |
| Stakeholder | Open a read-only view or a PDF and understand it without training |

## 4. Value proposition and differentiators

What we offer that the alternatives do not, in priority order:

1. **Built for AI agents.** Granular commands with JSON output, machine-readable diagnostics with
   fix hints, an MCP server, `explain` and `render --png` so a model can verify its own work. No
   other Gantt tool is designed for this.
2. **No watermark, no paywall, permissive license** (MIT OR Apache-2.0).
3. **Git-native text format** with stable ordering and formatting, so diffs are small and reviews
   are meaningful. Comments in the file survive edits made in the GUI.
4. **Explainable scheduling.** "Why this date?" panel; a leveling audit trail ("T-42 delayed 3 days
   because Developer was over-allocated by T-17 and T-31; priority rule: least total float").
5. **Hybrid agile as a first-class concept.** Sprints, releases and iterations are real objects
   (a calendar of iterations with bands in the timeline), not fake tasks. Predictive and agile
   branches coexist in one WBS.
6. **Cost-loaded schedule and S-curve out of the box.** Resource rates × assignments + fixed costs
   per task → planned value per period → S-curve; with baseline and actuals, earned value (PV, EV,
   AC, SPI, CPI).
7. **Print preflight.** Before exporting, a check reports text below the minimum point size at the
   chosen page size and scale, clipped labels, and pages that would split a row.
8. **WBS and Gantt from the same data.** WBS tree diagram export, WBS dictionary, 100 % rule check.
9. **Lint rules for plans** (configurable, runnable in CI): task without predecessor or successor,
   milestone with duration, dangling dependencies, over-allocation after leveling, negative float,
   missing owner, etc.
10. **Interop:** import/export Microsoft Project XML (MSPDI) and GanttProject `.gan`, export XLSX,
   CSV, Mermaid; optional `.mpp` import through an external converter.

Non-goals (at least until v1): team chat, timesheets, portfolio management across many projects,
a hosted multi-tenant SaaS, Kanban boards as a primary view.

## 5. Scope by milestone

### M0 — Engine and format (CLI only)
- Domain model, text format (read/write, round-trip preserving comments), CPM engine (FS/SS/FF/SF,
  lags in working or elapsed time, calendars, constraints), validation diagnostics.
- Agent CLI (§8a): granular mutation commands, queries, `explain`, `check --json`, `apply` (batch),
  `export --svg/--png/--csv`, `schema`, plus `alma guide` (the same text as `docs/agent-guide.md`) for models.
- **Done when:** the sample plans in `examples/` schedule correctly, property tests pass (see §12),
  a plan of 5,000 tasks schedules in < 50 ms on a laptop, and **an agent given only `alma guide`
  can build the sample hybrid plan from a written brief using the CLI, with zero invalid commands
  left unexplained** (record the session as an acceptance test).

### M0.5 — MCP server
- `alma mcp` exposes the same operations as MCP tools (same schemas as the CLI JSON), so any
  MCP client can plan without a shell.

### M1 — Interactive editor (desktop, read/write)
- Task grid (spreadsheet-like inline editing, outline indent/outdent), timeline with bars,
  milestones, summary bars, dependency arrows, today line, critical path highlight.
- Drag to move/resize, drag between bars to create a dependency, undo/redo, keyboard-first editing,
  command palette, zoom (day → week → month → quarter), light and dark themes.
- Open/save `.alma` files from disk; external changes (git pull) reload safely.
- **Done when:** a user can rebuild the sample hybrid plan from scratch in the GUI without touching
  the file, and the resulting file diff is clean.

### M2 — Resources, costs, leveling, S-curve
- Resources (people/roles, equipment, material), rates, calendars per resource, assignments with
  units; fixed costs; resource histogram; leveling (see §7.4) with audit trail; baseline snapshot;
  S-curve and cost tables; XLSX export of costs per period.

### M3 — Print and interop
- Print preflight, multi-page PDF tiling, WBS tree diagram export, MSPDI import/export, GanttProject
  import, Mermaid export.

### M4 — Agile and tracking
- Iterations/sprints and releases as objects, sprint bands, progress tracking (status date, % complete,
  remaining duration), earned value, baseline comparison bars, web build (same UI in the browser,
  WASM engine, no server).

### Later
- Real-time collaboration (CRDT), plugin/scripting API, Monte Carlo schedule risk analysis,
  multi-project resource pools.

## 6. Domain model

Overview only. The normative field reference, including names, types and which milestone uses each
field, is `docs/format.md` (ADR 0007); where this sketch and that file differ, that file wins.

```
Project
 ├─ settings: start, currency, default calendar, time unit, status date
 ├─ calendars[]: working weekdays, working hours, exceptions (holidays, blackout periods)
 ├─ resources[]: id, name, kind (work | material | cost), rate(s), max units, calendar
 ├─ cadences[]: id, start, length, count → generated iterations (sprints; optional)
 ├─ tasks[] (tree = WBS)
 │    id (stable, user-visible slug), name, kind (task | milestone | loe | buffer),
 │    wbs_role (deliverable | control_account | work_package | planning_package)
 │    duration | effort, constraint (ASAP, ALAP, SNET, FNLT, MSO…), calendar override
 │    deps[]: { pred, type: FS|SS|FF|SF, lag (working | elapsed), }
 │    assign[]: { resource, units }
 │    fixed_cost, owner, wbs dictionary fields (description, deliverable, acceptance)
 │    iteration (optional), tags, custom fields
 │    progress: percent, actual start/finish (M4)
 ├─ baselines[]: named snapshots of computed dates and costs
 └─ rules: lint configuration
```

- **IDs are stable slugs** chosen by the user or generated (`gate-ocr-install`), never row numbers,
  so diffs and references survive reordering. The WBS code (1.2.3) is derived from the tree.
- **Computed values are never stored in the source file** (early/late dates, float, costs). They
  can be written to a separate cache or export, never mixed with the source.

## 7. Scheduling engine

### 7.1 Time model
- Internal unit: minutes (`i64`); dates as `jiff::civil::Date` (ADR 0005). `1d` is
  `hours_per_day` working hours (ADR 0006).
- Durations in working time by default; elapsed time (`3ed`) supported for lags and tasks.
- Money with `rust_decimal`, never floats.

### 7.2 CPM
- Build a DAG of tasks; detect cycles and report the cycle path as a diagnostic.
- Topological order (stable, ties broken by source order), forward pass (early start/finish),
  backward pass (late start/finish), total and free float, critical path (float ≤ 0, configurable).
- All four dependency types with positive and negative lags; summary tasks roll up from children;
  a dependency on a summary task applies to its leaf descendants and is flagged by `L-DEP-09`
  (§7.6.1).
- Constraints applied in the passes; conflicts produce diagnostics, not panics.

### 7.3 Explainability
- Each computed start/finish keeps a *driver*: the dependency, constraint, calendar shift or
  leveling decision that set it. The UI walks drivers backwards to show the chain.

### 7.4 Resource leveling
- No algorithm guarantees an optimal leveled schedule (resource-constrained scheduling is NP-hard),
  so we choose a **deterministic heuristic**: serial schedule generation scheme, priority rule
  configurable (default: least total float, then earliest late finish, then source order).
- Leveling only delays tasks within the options the user allows (within float only, which is
  *smoothing*, or allow end-date slip, which is *leveling*; §7.6.5); it never changes dependencies.
- Output: new dates + an audit log entry per delayed task (which resource, which competing tasks,
  how many days). Leveled and unleveled schedules can both be shown.

### 7.5 Costs and S-curve
- Cost per task = Σ (assignment units × resource rate × working time) + fixed cost.
- Distribution over time: linear across the task's working days by default (front/back-loaded later).
- S-curve = cumulative planned value per period (day/week/month), for the current schedule and each
  baseline; with progress: EV and AC series, SPI, CPI.

### 7.6 Refinements from the knowledge base (binding)

Two research notes, written from PMBOK 6, the Practice Standard for WBS, the Agile Practice Guide,
estimation readings and course material, refine §6–§7. They are part of this spec; read the
relevant part before building each module:

- `docs/knowledge/scheduling-and-wbs.md` — WBS, activities and logic, estimating, CPM, leveling,
  compression, baselines, agile/hybrid. Annex A: lint rules (`L-WBS`, `L-SCH`, `L-DEP`, `L-EST`,
  `L-BAS`, `L-AGL`…); Annex B: formulas; Annex C: EN↔ES glossary.
- `docs/knowledge/resources-cost-evm.md` — resources, cost, reserves, earned value, change control,
  risk ↔ schedule, use case points, cash flow/NPV/IRR. Same annex structure (`L-RES`, `L-COST`,
  `L-EVM`, `L-CF`…).

Decisions taken from them (they override anything above that contradicts them):

1. **WBS ≠ activity network.** Tree nodes carry a `wbs_role`: `deliverable`, `control_account`,
   `work_package`, `planning_package`; leaves that are scheduled are activities (`task`,
   `milestone`, `loe`). The WBS has no sequence: dependencies on summary/WBS nodes are a lint
   warning, not silently expanded. WBS diagram and dictionary are generated from this tree.
2. **Dependencies carry metadata:** `nature` (mandatory | discretionary), `scope` (internal |
   external) and `rationale` (required for any lag/lead). Lags never carry effort or cost.
3. **Milestones** have `class` (contractual | internal | external), owner and acceptance criterion.
   A soft `deadline` is reported (negative float) but never overrides logic; hard constraints are
   linted for overuse. The backward pass anchors on the project early finish unless a project
   deadline is set; report negative float, near-critical paths (configurable threshold) and
   multiple critical paths.
4. **Scheduling type per task:** fixed duration | fixed work (effort) | fixed units, with the
   effort→duration formula in Annex B. Store optional three-point estimates (`o`, `m`, `p`) and a
   basis of estimate from M0, so Monte Carlo can be added later without a format change.
5. **Leveling vs smoothing are separate operations.** Smoothing stays within free/total float and
   never moves the end date; leveling may, and must report when the critical path changed.
6. **Reserves are explicit:** contingency reserve (inside the cost/schedule baseline, can be tied to
   identified risks) and management reserve (outside the baseline, inside the budget). BAC,
   PV and the S-curve are defined with respect to that split.
7. **Baselines are versioned and approved:** a baseline changes only through a recorded change
   (id, reason, date), from the change date forward, never rewriting past PV. One active baseline;
   `alma baseline save` never overwrites history.
8. **Earned value is complete:** PV, EV, AC, BAC, SV, CV, SPI, CPI, EAC (all variants), ETC, VAC,
   TCPI, per period and cumulative; each task declares an EV measurement method (0/100, 50/50,
   physical % complete, milestone weights, LOE), and EV never exceeds the task budget. **PV always
   comes from a baseline;** the curve of the current plan is a forecast and is labeled as such.
9. **Cost model:** resource types (work, equipment, material with quantity and unit price, cost),
   effective-dated rates, cost rate vs billing rate, cost categories (investment, operation,
   admin, HR) for cash-flow export, accrual timing of fixed costs (start | prorated | end),
   resource breakdown structure and time-varying availability.
10. **Agile/hybrid:** each WBS branch declares an `approach` (predictive | iterative | flow); rules
    for predictive logic (e.g. open ends) do not apply inside iterative/flow branches. Iterations
    follow a cadence of equal timeboxes; releases are distinct from iterations; backlog items carry
    points; velocity, capacity, burndown/burnup and feature-based EV are reports.
11. **Procurement and payments** appear in the schedule: procurement chains scheduled backwards from
    the need date, and payment milestones that feed the cash flow.
12. **Reports/exports added:** milestone schedule, network diagram, resource histogram, S-curve
    (baseline PV, EV, AC, forecast), monthly cash flow with total and cumulative, NPV/IRR summary.

Formulas marked as "extension" or "interpretation" in the notes (e.g. weighted EAC, path variance
sum) are not from the sources; implement them behind an explicit option and document them as such.

## 8. File format

The normative reference is `docs/format.md` (ADRs 0006–0008). This section keeps the rationale and
a short example.

**Decision:** TOML, one file per project (`plan.alma.toml`), edited with `toml_edit` so comments,
ordering and formatting survive GUI edits. Rationale: widely known, good Rust support, readable,
line-oriented diffs. A custom DSL (like TaskJuggler or utf8proj) reads nicer by hand but makes
lossless round-trip from a GUI much harder; we can add a DSL front-end later if wanted.

Rules: tasks appear in WBS order; one `[[task]]` table per task with a `parent` key (flat list, not
nested tables, so moving a task is a one-line diff); keys in a fixed order; dates ISO 8601.

```toml
# Implementation plan — Stage 1
[project]
format = 0
name = "Terminal TOS replacement"
start = 2027-01-04
calendar = "cl-standard"
currency = "UF"

[[calendar]]
id = "cl-standard"
workdays = ["mon", "tue", "wed", "thu", "fri"]
hours = ["09:00-13:00", "14:00-18:00"]
exceptions = [
  { name = "Peak season freeze", from = 2027-12-15, to = 2028-04-30, working = false },
]

[[resource]]
id = "dev"
name = "Developer"
rate = { amount = "4.2", per = "day" }
max_units = 6

[[cadence]]
id = "sprints"
prefix = "s"
start = 2027-03-01
length = "10d"
count = 12

[[task]]
id = "gate"
name = "Gate subproject"
wbs_role = "control_account"

[[task]]
id = "gate-design"
parent = "gate"
name = "Gate design"
duration = "15d"
assign = [{ resource = "dev", units = 2 }]

[[task]]
id = "gate-build"
parent = "gate"
name = "Gate build (sprints 1-6)"
duration = "60d"
deps = [{ pred = "gate-design", type = "FS", lag = "2d" }]

[[task]]
id = "h4"
name = "H4 — Stage 1 software delivered for testing"
kind = "milestone"
deps = [{ pred = "gate-build" }]
```

## 8a. Agent interface (first-class)

Design rule: **anything a human can do in the GUI, an agent can do with one command, and every
command is safe to run blind.** The GUI and the CLI call the same command layer (§9), so they can
never drift apart.

### Commands
Granular, one concept per command, stable IDs everywhere:

```
alma init plan.alma.toml --name "TOS replacement" --start 2027-01-04
alma task add gate-build --name "Gate build" --parent gate --duration 60d
alma task set gate-build --duration 55d
alma task move gate-build --parent yard --after yard-design
alma task rm gate-build                      # refuses if others depend on it, unless --cascade
alma dep add gate-design gate-build --type FS --lag 2d
alma dep rm gate-design gate-build
alma res add dev --name Developer --rate 4.2/day --max 6
alma assign gate-build dev --units 3
alma iter add s01 --start 2027-03-01 --length 10d --repeat 12   # sprint calendar
alma baseline save "Informe 3"
alma level --rule least-float --within-float
```

Queries and verification (read-only):

```
alma show gate-build                 # one task, computed fields included
alma ls --parent gate --fields id,name,es,ef,tf --limit 50
alma critical                        # critical path as an ordered list
alma explain gate-build              # why this date: driver chain
alma check                           # lint + scheduling diagnostics
alma diff --baseline "Informe 3"     # what moved and by how much
alma render --png out.png --range 2027-01..2027-06   # image a multimodal model can look at
alma schema [command]                # JSON Schema of inputs/outputs
alma guide                           # compact usage guide for LLMs (= docs/agent-guide.md)
```

### Contract every command honors
- **Structured output:** `--json` on every command (default when stdout is not a TTY is still
  human text; agents pass `--json`). Output schemas are versioned and published via `alma schema`.
- **Small by default:** list commands paginate (`--limit`, `--cursor`) and accept `--fields`, so an
  agent never floods its context window with a 2,000-task dump.
- **Validated and atomic:** a command either applies fully and leaves a valid file, or changes
  nothing. After each mutation the plan is rescheduled; the result reports what moved
  (`{"changed": [...], "end_date_delta": "+3d", "new_diagnostics": [...]}`).
- **Diagnostics an agent can act on:** stable code (`E-DEP-01` cycle; scheme in ADR 0008),
  message, the IDs involved, and a suggested fix command when one exists (`"fix": "alma dep rm a b"`).
- **Dry run:** `--dry-run` on every mutation shows the effect without writing.
- **Batch:** `alma apply ops.json` (or JSONL on stdin) runs many commands in one transaction; all or
  nothing. This is how an agent builds a large WBS in one step after planning it.
- **Idempotent where possible:** `add` with an existing identical definition is a no-op, not an
  error; `--if-not-exists` / `--upsert` flags make retries safe.
- **Deterministic:** same file + same commands = same bytes on disk and same output.
- **Exit codes:** 0 ok, 1 diagnostics with errors, 2 invalid command; documented.
- **No interactive prompts**, ever, unless `--interactive` is passed.

### Working with the file directly
Agents may also edit `plan.alma.toml` with plain text tools; `alma check` validates it and
`alma fmt` normalizes it. The CLI is preferred because it enforces invariants, but the file stays
simple enough to edit by hand or by a model.

### Humans in the loop
- The desktop app watches the file and live-reloads, highlighting what the last agent change touched,
  so a person can watch the plan being built and step in.
- Every mutation is appended to an operation log (`.alma/log.jsonl`: who — agent or human —, when,
  command) for review and undo; git remains the long-term history.
- Locking: the GUI and an agent never write at the same time; a write that loses a race is retried
  on the new version or rejected with a clear diagnostic.

## 9. Architecture

```
crates/
  alma-core      domain model, ids, units, money, durations (no I/O, no UI)
  alma-sched     calendars, CPM, constraints, leveling, costs, EVM, diagnostics
  alma-format    TOML read/write with toml_edit (lossless), schema validation
  alma-interop   MSPDI XML, GanttProject .gan, CSV, XLSX, Mermaid
  alma-layout    view model → display list (rects, lines, paths, text) for a given zoom/page
  alma-export    display list → SVG, PDF; print preflight
  alma-ops       command layer: typed commands, validation, transactions, op log, JSON schemas
                 (shared by CLI, MCP server and GUI so they cannot drift)
  alma-cli       `alma` binary (all commands in §8a)
  alma-mcp       MCP server exposing alma-ops as tools
  alma-wasm      wasm-bindgen bindings of core+sched+layout for the web UI
app/
  src-tauri/     Tauri 2 shell: file system, native menus, file watching
  ui/            Svelte 5 + TypeScript front-end (shared by desktop and web builds)
```

Key ideas:

- **The engine is a pure library.** It never touches the file system or the UI, so the same code runs
  in the CLI, the desktop app, the browser (WASM) and tests.
- **One layout engine for screen and print.** `alma-layout` computes geometry once; the UI draws the
  display list on a Canvas 2D (virtualized, only visible rows/time range), and `alma-export` writes
  the same list to SVG/PDF. What you see is what prints. Text measurement must use the *same bundled
  font files* on both sides (see risks).
- **Commands, not mutations.** Every edit is a command (`MoveTask`, `AddDependency`, …) applied to the
  model; undo/redo and future collaboration build on that log.
- **Incremental recompute.** After an edit, reschedule only the affected sub-graph when possible;
  full recompute must still be fast enough (< 50 ms for 5k tasks) to be the fallback.

## 10. Technology evaluation

### 10.1 Can it be Rust? Yes.

Rust is a very good fit for the engine, format, export and CLI (correctness, speed, WASM target,
single binary). The open question is only the **GUI layer**, because "a 10/10 interface" needs a rich
widget ecosystem, great text rendering, accessibility and fast iteration on design.

### 10.2 GUI options

| Option | What it is | Pros | Cons | Verdict |
|---|---|---|---|---|
| **Tauri 2 + web UI (Svelte 5/TS)** | Rust backend, system WebView front-end | Best design/iteration speed; huge ecosystem; same UI ships as a web app over WASM; small binaries and lower memory than Electron; Rust stays in charge of all logic | WebView differences across OSes (WebKitGTK on Linux is the weakest); IPC boundary; GUI tests go through the WebView | **Recommended** |
| GPUI (+ gpui-component) | Zed's GPU UI framework, pure Rust | Very fast; pure Rust; gpui-component ships 60+ components and powers a real commercial app | Pre-1.0 with frequent breaking changes; small ecosystem; no web target; accessibility immature | Revisit for a v2 native client |
| Slint | Declarative native toolkit, `.slint` markup | Fast startup, low memory, good tooling | Custom canvas-heavy widgets (timeline) are less natural; license is GPL/royalty-free/commercial mix — check before choosing | Viable, not preferred |
| egui | Immediate-mode | Simple, great for tools/editors | Hard to reach a polished "product" look; text/a11y limits | No |
| Iced / Dioxus | Elm-style / React-style in Rust | Pure Rust | Smaller widget ecosystems; Dioxus desktop is still WebView-based | No |

**Recommendation:** Tauri 2 shell + Svelte 5 front-end + Rust engine compiled both natively (desktop,
CLI) and to WASM (web build). This keeps ~all logic in Rust, gives the best chance at a top-tier UI,
and gets a zero-install web version almost for free. The timeline is our own Canvas renderer fed
by `alma-layout`; we do not depend on a third-party JS Gantt library (DHTMLX, Frappe, SVAR) because
the chart *is* the product and their free tiers gate advanced features or are too limited.

### 10.3 Libraries (verify current versions when starting; pin them)

| Need | Choice | Notes |
|---|---|---|
| Dates | `jiff` (civil dates only) | ADR 0005 |
| Money | `rust_decimal` | No floats for money |
| Graphs | own DAG on `Vec` indices, or `petgraph` | Own is simpler and faster for CPM |
| TOML | `toml_edit` | Lossless round-trip, keeps comments |
| Serde | `serde` | For interop formats |
| XML (MSPDI, .gan) | `quick-xml` + `serde` | MSPDI is plain XML |
| `.mpp` (binary MS Project) | MPXJ (Java, LGPL) as an **optional external converter** | No Rust reader exists; never link it into the core |
| XLSX write / read | `rust_xlsxwriter` / `calamine` | |
| SVG → PDF | `svg2pdf` / `krilla` (from the Typst/resvg family) | Vector PDF with embedded fonts |
| Text shaping/metrics | `rustybuzz` + `ttf-parser` (or `cosmic-text`) | Must match browser metrics: bundle the fonts |
| WASM | `wasm-bindgen`, `wasm-pack` or `trunk` | |
| Errors / diagnostics | `thiserror`, `miette` (CLI pretty diagnostics) | |
| Testing | `proptest`, `insta` (snapshots) | |
| Future collaboration | `loro` or `automerge` (CRDT, both Rust) | Later |

### 10.4 Prior art to study (not to depend on)

- **utf8proj** (MIT/Apache, Rust): plain-text plans, CPM with FS/SS/FF/SF and lag, deterministic
  leveling with audit trail, calendars, LSP and WASM playground, but no interactive editor.
  **Decision (2026-10-29): we write our own engine** and do not depend on it. Use it only as a
  reference for semantics and, optionally, as a differential-testing oracle.
- **TaskJuggler:** the most capable plan-as-code tool; reference for scheduling semantics and reports.
- **MPXJ:** the de-facto reference for MS Project / Primavera file semantics.
- **GanttProject, ProjectLibre:** what users already know; reference for import and expectations.
- **Frappe Gantt, DHTMLX Gantt (Community Edition now MIT), SVAR Gantt:** UI interaction patterns.

## 11. UI/UX principles ("10/10")

- **Speed is a feature:** 60 fps scroll and drag with 10k tasks; no spinner for a reschedule.
- **Keyboard-first:** spreadsheet-like grid (Enter, Tab, arrows, type to edit), `Alt+→/←` indent,
  `Ctrl+K` command palette, `Ctrl+Z/Y`; every action reachable without the mouse.
- **Direct manipulation:** drag bars, drag ends to resize, drag from bar to bar to link, right-click
  for dependency type and lag; snapping to working days.
- **Semantic zoom:** day/week/month/quarter/year; labels adapt instead of overlapping.
- **Calm visual language:** few colors with meaning (critical path, milestone, summary, sprint band,
  baseline ghost bar, over-allocation); everything else neutral. Light and dark themes.
- **Explain on hover:** hover a date to see its driver; hover a red resource cell to see the
  conflicting tasks.
- **Accessible:** visible focus, full keyboard path, ARIA on the grid, contrast ≥ WCAG AA, reduced
  motion respected.
- **Print preview as a mode**, not a dialog: page boundaries, preflight warnings inline.

## 12. Quality and testing

- **Property tests (proptest) on the engine:** for random DAGs, ES ≤ LS, EF ≤ LF, float ≥ 0 on
  feasible plans without constraints, every dependency satisfied, critical path is a connected chain
  from start to end, schedule is invariant to task order in the file.
- **Golden tests:** sample plans with known results (hand-checked), including textbook CPM examples.
- **Differential tests:** same plans through utf8proj and/or GanttProject exports; investigate diffs.
- **Round-trip tests:** read → write produces byte-identical output for formatted files; GUI edit
  changes only the affected lines.
- **Performance budgets in CI:** schedule 5k tasks < 50 ms; layout of the visible window < 4 ms.
- **Export tests:** snapshot SVGs (`insta`), preflight catches a known too-small font.

## 13. Risks

| Risk | Mitigation |
|---|---|
| Text metrics differ between browser Canvas and Rust export → layout mismatch | Bundle one font family; measure with the same font in both; snapshot-compare screen vs export |
| WebKitGTK on Linux renders/performs worse than other WebViews | Canvas rendering (not thousands of DOM nodes); test Linux early and often |
| Scope creep toward a full PM suite | Non-goals in §4; milestones gated by "done when" |
| Leveling results surprise users | Audit trail, "leveled vs unleveled" toggle, configurable rule, never silently change deps |
| GUI round-trip corrupts hand-written files | `toml_edit`, round-trip tests, refuse to save a file that changed on disk without merging |
| Calendar/time-zone bugs | Working-time arithmetic isolated and heavily property-tested |

## 14. Instructions for the building agent

1. Read this whole document. Record each decision you make that is not written here as an ADR in
   `docs/adr/NNN-title.md` (context, decision, consequences). Before building a module, read the
   matching part of `docs/knowledge/` (§7.6); implement lint rules from its Annex A tables and
   formulas from its Annex B, citing the rule/formula id in tests. Never copy text from the
   copyrighted sources those notes cite.
2. Start with **M0**. Create the Cargo workspace with the crates in §9 (only those M0 needs:
   `alma-core`, `alma-sched`, `alma-format`, `alma-ops`, `alma-cli`, plus `alma-layout`/`alma-export`
   for SVG/PNG).
3. Order of work for M0: model → calendars and working-time arithmetic (with property tests) →
   CPM (property + golden tests) → TOML read/write (round-trip tests) → diagnostics with codes →
   command layer with JSON schemas → CLI → SVG/PNG export → `alma guide` / `docs/agent-guide.md`.
4. Treat the agent contract in §8a as an API: every command gets tests for `--json` output,
   `--dry-run`, atomicity (a failing command leaves the file byte-identical) and idempotency.
5. Keep `alma-core` and `alma-sched` free of I/O and UI dependencies.
6. Every PR: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`; no test is ever disabled to
   make CI pass.
7. Conventional Commits; English for code, comments and docs.
8. The HTML prototype in `prototype/` shows the intended look and interactions; it is a sketch, not
   code to port line by line.

### Open decisions (resolve with the owner before or during M0)
- ~~`chrono` vs `jiff`~~: resolved in ADR 0005 (`jiff`).
- Project name and file extension (`.alma.toml` assumed).
- Svelte 5 vs SolidJS for the UI (default: Svelte 5).

## 15. Sources

- Comparison of Tauri, Slint, egui, Dioxus and Flutter on Linux (hands-on): https://medium.com/@yalovoy/i-built-the-same-app-with-five-gui-frameworks-tauri-slint-egui-dioxus-and-flutter-for-linux-31bd6f59ff6a
- gpui-component: https://github.com/longbridge/gpui-component · GPUI docs: https://docs.rs/gpui-pre/
- utf8proj: https://github.com/alanbld/utf8proj · https://docs.rs/utf8proj-solver
- MPXJ (LGPL, Java): https://github.com/joniles/mpxj
- DHTMLX Gantt Community Edition (MIT): https://www.dhtmlx.com/blog/tag/gantt/
- JS Gantt libraries roundup (Frappe, SVAR, others): https://blog.webix.com/best-free-javascript-gantt-chart-libraries/
- Typst as a Rust library: https://github.com/tfachmann/typst-as-library
- On leveling not having an optimal algorithm and MS Project behavior: https://planningplanet.com/comment/53104
