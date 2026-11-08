# CLAUDE.md — guide for agents building alma-gantt

Read this file first, then `SPEC.md` in full. This file says *how to work* in the repo; `SPEC.md`
says *what to build*. If they disagree, `SPEC.md` wins and you report the conflict.

## What this is

alma-gantt is an open-source Gantt/scheduling tool: a pure Rust engine (CPM, calendars, leveling,
costs, earned value), a plan stored as a TOML file in git, a CLI and MCP server designed for LLM
agents, and later a Tauri + Svelte desktop app for humans. Goal: the most robust open-source Gantt
tool, with agents doing most of the planning and humans able to edit anything by hand.

## Current state

- Milestones: **M0** (engine, file format, agent CLI) and **M0.5** (MCP server, `alma mcp`,
  ADR 0010) are done. **M2** (resources, costs, leveling) is in progress ahead of M1, so agents
  get resource and cost checks before the desktop editor exists. See `SPEC.md` §5.
- M2 so far: resource usage and over-allocation (`L-RES-01/02`, `alma res ls|usage`), leveling
  and smoothing as a schedule setting (`alma level`, ADR 0011), `settings.set`, costs and the
  forecast S-curve (`alma cost`, `alma scurve`, `alma res set`, ADR 0012), baselines with
  planned value and variance (`alma baseline save|ls`, `alma variance`, ADR 0013).
- Implemented: model and durations (`alma-core`), calendars, CPM and the M0 lint rules
  (`alma-sched`), the parser with the key registry (`alma-format`), queries and lossless
  mutations (`alma-ops`), and the CLI with the agent contract (`alma-cli`). The fixture schedules
  and lints cleanly.
- SVG/PNG/CSV export (`alma-layout`, `alma-export`, ADR 0009). Text widths are estimated until a
  font is bundled.
- Also done: `alma fmt`, `alma diff`, calendar/project/rule/cadence commands, `alma keys`, and
  the M0 acceptance test: a recorded agent session (`crates/alma-cli/tests/acceptance/`) that
  built the hybrid plan from a written brief using only `alma guide`, replayed in CI.
- Known gaps, by design until later milestones: reserves, funding and XLSX export (rest of M2),
  progress and earned value (M4), tasks cannot be linked to sprints (M4), text widths are estimated (ADR 0009).
- `docs/format.md` is the normative reference for plan files (ADRs 0005–0008). Read it before
  touching `alma-format` or the model.
- `examples/hybrid-terminal.alma.toml` is the **target fixture** for M0; making it parse,
  schedule and lint cleanly (no `error`, no `warn`) is part of M0.
- `prototype/index.html` is a UI sketch in plain JS (open it in a browser). Use it for look and
  interactions, not as code to port.

## Map of the repo

| Path | What it is |
|---|---|
| `SPEC.md` | Product and technical spec. The source of truth |
| `docs/adr/` | Architecture decisions (0001–0013 accepted). Template: `0000-template.md` |
| `docs/format.md` | Normative reference of the plan file format |
| `docs/knowledge/` | Project-management reference notes from PMBOK 6, the WBS standard, the Agile Practice Guide and estimation sources. Each concept maps to model fields, engine behavior, lint rules (Annex A) and formulas (Annex B) |
| `crates/alma-core` | Domain model: no I/O, no UI |
| `crates/alma-sched` | Scheduling engine: pure functions over `alma-core` |
| `crates/alma-format` | Lossless TOML read/write (`toml_edit`) |
| `crates/alma-ops` | Command layer shared by the CLI, the MCP server and the GUI |
| `crates/alma-layout` | Scheduled plan → display list (pure) |
| `crates/alma-export` | Display list → SVG and PNG |
| `crates/alma-mcp` | MCP server (`alma mcp`) on `rmcp`; tools generated from the batch schema |
| `crates/alma-cli` | The `alma` binary |
| `examples/` | Sample plans; they double as golden-test fixtures |
| `prototype/` | UI sketch |

Crates planned for later milestones (do not create them before they are needed): `alma-interop`,
`alma-wasm`, `app/`.

## Commands

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p alma-cli -- <args>
```

CI (`.github/workflows/ci.yml`) runs exactly these three checks; run them before every commit.
The toolchain is pinned in `rust-toolchain.toml`.

## Rules

### Architecture
- `alma-core` and `alma-sched` never do I/O, never read the clock, never depend on UI or CLI crates.
- The CLI, the MCP server and the GUI only call `alma-ops`; they never mutate the model directly (ADR 0004).
- A new batch operation in `ops.schema.json` becomes an MCP tool automatically (ADR 0010); keep
  the schema, `Op` and the docs in step.
- Computed values (dates, float, costs) are never written to the plan file (ADR 0003).
- Money uses `rust_decimal`, never floats. Time arithmetic is in working time on calendars.
- Same input means same output: no `HashMap` iteration order in output, ties broken by source order.
- Errors are values (`thiserror`); the engine reports diagnostics with stable codes, it does not panic
  on bad plans.

### Agent contract (SPEC.md §8a)
Every command you add must support `--json`, `--dry-run`, atomic writes (on failure the file stays
byte-identical), small paginated output, and diagnostics with a code, the IDs involved and a fix
hint when one is safe. Each of these properties gets a test. A feature is not done until an agent
can use it from the CLI.

### Domain knowledge
- Before building a module, read the matching part of `docs/knowledge/` (`SPEC.md` §7.6 lists which).
  Implement lint rules from Annex A and formulas from Annex B, and put the rule or formula id in
  the test name (e.g. `l_dep_01_open_start`, `f_sch_07_total_float_ss`; scheme in ADR 0008).
- Formulas marked "extension" or "interpretation" in those notes are not from the sources: put them
  behind an explicit option and document them as ours.
- Never copy text from PMBOK or the other copyrighted sources. Paraphrase in your own words; this
  repo is public.

### Testing
- Engine: property tests (`proptest`) for CPM invariants, golden tests from `examples/`, and
  textbook CPM cases worked out by hand in the test itself.
- Format: round-trip tests (read then write is byte-identical; one edit changes only its lines).
- Performance budget from `SPEC.md` §12: schedule 5,000 tasks in under 50 ms (add a bench when CPM lands).
- Never delete, disable or weaken a test to make a check pass. If a test is wrong, say why and fix it.

### Decisions and the spec
- A decision not written in `SPEC.md` goes into a new ADR in `docs/adr/` (next number, template
  0000) in the same change that implements it.
- Do not silently change `SPEC.md`. If it is wrong or incomplete, propose the change to the owner
  (Daniel) with the reason; edit it only once agreed, and mention it in the commit.
- Choose dependencies deliberately: well maintained, MIT/Apache-compatible licenses only (no GPL in
  the workspace), pinned versions. Check a crate's current version and maintenance before adding it.

### Git
- Conventional Commits (`feat(sched): forward pass with SS and FF lags`).
- Small commits, each passing fmt, clippy and tests.
- Never commit, push or open a PR unless the owner asks.
- Code, comments, docs and commit messages in English. Talk to the owner in Spanish.

## How to start M0

Follow `SPEC.md` §14: model → calendars and working-time arithmetic → CPM → TOML read/write →
diagnostics → command layer → CLI → SVG/PNG export → `docs/agent-guide.md` (the text `alma guide`
prints). The decisions that blocked M0 (dates library, duration grammar, file format, diagnostic codes)
are settled in ADRs 0005–0008.
