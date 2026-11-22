# Development notes: how to resume work on alma-gantt

Read this file after `CLAUDE.md` (how to work in the repo) and `SPEC.md` (what to build).
`CLAUDE.md` "Current state" is the short summary. This file adds what a new session, on any
machine, needs to pick the work up: where things stand, how features are built here, and the
traps already found.

Last updated 2026-11-22.

## 1. Where things are

All work is merged into `main`. There are no open branches.

| Milestone | Content | ADRs |
|---|---|---|
| M0 | Model, calendars, CPM, TOML format, lint, agent CLI, SVG/PNG/CSV export, `alma guide`, recorded acceptance session | 0001–0009 |
| M0.5 | MCP server, `alma mcp` | 0010 |
| M2 | Leveling, costs, baselines, reserves, risks and funding, XLSX workbook, resource calendars, histogram and S-curve charts | 0011–0017 |
| M4 (in progress) | Progress and status date, earned value, baseline revisions, iterations, stories and velocity, burndown and burnup | 0018–0022 |

- M2 was built before M1, by decision of the owner: agents needed resource and cost checks
  before a desktop editor existed.
- Numbers: 201 tests and ADRs 0001–0022. CI runs `cargo fmt --check`, `clippy -D warnings`
  and `cargo test`.

**Left for M4** (`SPEC.md` §5; knowledge notes, scheduling §5.4, §5.9 and resources §5.8):
- `[[release]]` (reserved today): iterations plus a release milestone, and `L-AGL-05`.
- Agile earned value for an `iterative` container: budget × accepted points ÷ planned points.
- `settings.schedule.agile_drives = planned_iterations | velocity_forecast`.
- Baseline comparison bars on the Gantt chart. The snapshot already holds baseline task
  dates.
- The web build (WASM, same engine in the browser). It needs the `alma-wasm` crate; do not
  create it before it is needed.
- Smaller follow-ups:
  - `L-EVM-06` (bottom-up EAC; needs a manual ETC per task);
  - `L-EVM-07` and per-period EV and AC (need status snapshots over time, e.g.
    `.alma/status/*.toml`);
  - `L-BAS-08` (management reserve drawdowns).

**Other milestones:** M1 (the Tauri + Svelte desktop editor) and M3 (print preflight,
multi-page PDF, MSPDI and GanttProject import/export). Ask the owner which comes next.

**The fixture** `examples/hybrid-terminal.alma.toml` is the golden test:
- it has `leveling = "leveling"`, because the brief needs 7 developers and the team has 6;
- its leveled finish is 2027-09-24, and the finish with logic alone is 2027-07-30;
- total cost 4,714.50 UF, of which gate-sw is 648 UF;
- it must parse, schedule and lint with no `error` and no `warn`.

## 2. Using alma (on any machine)

```sh
git clone https://github.com/DmV554/alma-gantt && cd alma-gantt
cargo install --path crates/alma-cli        # rust-toolchain.toml selects the toolchain
alma init plan.alma.toml --name "Project" --start 2027-01-05
alma guide                                  # what an agent reads first
claude mcp add alma -- alma --file /absolute/path/plan.alma.toml mcp
```

Every command takes `--json`, and mutations take `--dry-run`. Baseline snapshots live in
`.alma/baselines/<plan stem>/<id>.toml`; commit them with the plan.

## 3. How a feature is built here

**Every feature ships with all of these, in the same change:**
- An ADR: the next number, template `docs/adr/0000-template.md`, written for decisions not in
  `SPEC.md`. Mark our interpretations as ours.
- `docs/format.md` rows for new keys, and the matching tier in
  `crates/alma-format/src/keys.rs`. A test (`format_doc_matches_key_registry`) keeps them in
  sync; new sections go in its `DOC_TABLES` list.
- A section or line in `docs/agent-guide.md`: it is what `alma guide` prints, and agents learn
  the tool from it.
- An update to `CLAUDE.md` "Current state".
- Contract tests through the real binary (`crates/alma-cli/tests/contract.rs`): JSON output,
  dry run, a byte-identical file on refusal, error codes. Plus MCP tests
  (`crates/alma-mcp/tests/tools.rs`).
- Engine tests worked out by hand, named with the rule or formula id (`l_bas_06_…`,
  `f_cost_21_to_28_…`), and proptests where an invariant exists.

**A new batch operation:**
- Add the `Op` variant in `crates/alma-ops/src/mutate.rs`, its entry in
  `crates/alma-ops/src/ops.schema.json`, and an `op_description` in `crates/alma-mcp/src/lib.rs`.
  It becomes an MCP tool automatically.
- The schema JSON is formatted by hand. Insert entries as text; never rewrite it with
  `json.dump`.
- Add the op to the random-command proptest in `crates/alma-ops/tests/robust.rs`. Removal ops
  go in its not-idempotent list.
- Numeric task keys (`physical_pct`, `points`) must be written as TOML numbers: see
  `set_task_fields`. Date keys are listed in `is_date_key`.

**Commands that write more than the plan** (`baseline save`, `baseline revise`, which write a
snapshot) are not batch ops. They get a hand-written MCP tool and use `mutate::apply_edit`.
They write the snapshot first and the plan second, and remove the snapshot if the plan cannot
be written.

**Lint rules:**
- Rules go in `RULES` (`crates/alma-sched/src/lint.rs`) in code order; a test checks the order.
- A severity that depends on the phase follows the `L-SCH-05` / `L-COST-04` pattern: warn
  while draft, error after.
- Rules that need the active baseline (`L-BAS-03/06/09`, `L-EVM-*`) go in `lint_baseline`.
  `alma_ops::load_file` reads the active and parent snapshots and adds them, so plain
  `load(src)` (used inside mutations) never sees them.
- Thresholds are `[rules.params]` (`alma rule param`), not new settings keys.

**Money and time:**
- Money is `rust_decimal`. Cost amounts are rounded to 12 decimal places so period shares add
  up exactly. Reports round to cents through `alma_ops::cost::money`.
- Time arithmetic is in working minutes on calendars (`WorkCalendar`).
- An actual date on a non-working day means its next working minute.

**Visual review:**
- Render PNGs into `review/<date>/` (ignored by git) and look at them, e.g.
  `alma export review/x.png --chart burnup`.
- Several real bugs were found only this way:
  - a split bar for a weekend gap;
  - a release milestone ignoring its agile container;
  - a title overlapping the legend.

**Commits:**
- Conventional Commits, with no AI attribution trailers.
- One commit per feature block, each passing fmt, clippy and tests.
- Never commit, push or open a PR unless the owner asks.

## 4. Key engine concepts added in M4

- **`Progress`** (`crates/alma-sched/src/cpm.rs`): `NotStarted`, `InProgress { done_until,
  resume, remaining, waits }` or `Complete`.
  - One `place` closure gives the dates of started work, and of stories fixed to their
    iteration. The forward pass and leveling both use it.
  - Not-started work is floored at the data date inside `earliest`.
  - Leveling books started work first.
- **Work spans:** `TaskDates::work_spans` returns one span, or two when a task in progress waits
  (`waits`) for a predecessor. Cost (`Accrual::Spread.pause`), demand and the Gantt bar use the
  spans.
- **Snapshot format 1** (`crates/alma-format/src/snapshot.rs`):
  - each task's own `budget` and daily `pv`;
  - `revised_at` for revisions;
  - format 0 is still read (`per_task = false`), but earned value refuses it (`L-EVM-04`).
- **Earned value** (`crates/alma-sched/src/evm.rs`):
  - reserves are not earned;
  - level-of-effort tasks, and money carried by a summary itself, earn as planned;
  - rows roll up over descendants;
  - baseline tasks the plan lost stay in BAC.
- **Stories:**
  - A story with an `iteration` is in the logic network, fixed to the window, so links on its
    container wait for the last planned sprint.
  - A backlog story is out of the network and out of roll-ups (`Driver::Backlog`).
  - The Gantt chart never draws stories.
- **Iterations** (`crates/alma-sched/src/agile.rs`): generated back to back from a cadence.
  `windows()` extends a cadence past `count` for forecasts.

## 5. Traps already found

- **zsh**:
  - it does not word-split `$VAR`, so `alma $F …` with `F="-f path"` fails; pass the flag
    literally;
  - `ls` may be aliased; use `\ls` or `find`;
  - `cat > file` with no stdin hangs.
- **clap** reads a negative value after a flag as another flag: use `--cost=-5`.
- **`cargo fmt`** reformats code, so exact-match edit scripts written before it can miss.
  Re-read before editing. Edit scripts that `assert` the old text before writing are safe.
- **`/tmp`** may be a small tmpfs. Building in `/tmp` worktrees can fill it; use
  `CARGO_TARGET_DIR` on the main disk.
- **The backward pass after leveling** must use the leveling placement order, because the
  implicit resource links break the original topological order.
- **Leveling and baselines are settings or sidecars**, never dates stored in the plan
  (ADR 0003, ADR 0011).
- **Grep filters on test output:** `failed` also matches "0 failed". Filter on
  `panicked|FAILED`.
- **Snapshot tests:** a plan with progress but no `[[baseline]]` triggers `L-BAS-01`. Filter it
  in tests that check other rules.

## 6. Useful commands

```sh
cargo run -q -p alma-cli -- -f examples/hybrid-terminal.alma.toml check
cargo run -q -p alma-cli -- -f plan.alma.toml evm --depth 2
cargo run -q -p alma-cli -- -f plan.alma.toml iter ls
gh pr list; gh pr checks <n> --watch
PROPTEST_CASES=2000 cargo test -p alma-ops --test robust   # thorough random-command run
```
