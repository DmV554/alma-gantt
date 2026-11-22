# alma — guide for agents

`alma` edits and checks a project plan stored in one TOML file (`*.alma.toml`). Every command
takes `--json` (structured output) and every change takes `--dry-run` (show the effect, write
nothing). A change either applies fully and leaves a valid file, or changes nothing.

Pick the file with `--file plan.alma.toml`, `ALMA_FILE`, or run in a directory with one plan.

## Through MCP

`alma mcp` serves the same operations as MCP tools over stdio. In Claude Code:

```
claude mcp add alma -- alma mcp            # uses the only *.alma.toml in the working directory
claude mcp add alma -- alma --file /path/plan.alma.toml mcp
```

Tools mirror the commands below: `check`, `list_tasks`, `show_task`, `critical_path`,
`explain`, `list_resources`, `resource_usage`, `cost`, `scurve`, `baseline_list`, `variance`,
`baseline_save`, `diff`, `keys`, `render` (returns a chart as an image: the Gantt chart,
`chart: "histogram"` with a `resource`, or `chart: "scurve"`), `export`, `init`, `apply`, and
one tool per operation (`task_add`, `dep_add`, `level`, `settings_set`, `reserve_add`, …, same
fields as `alma schema`). Every tool takes an optional `file`; mutations take `dry_run`.

## Recommended loop

0. Set up time: `cal add` for working days and hours, `cal except` for holidays and blackout
   periods, `project set --calendar ID`. The default calendar is Mon–Fri, 09:00–13:00 and
   14:00–18:00.
1. Build the WBS first: control accounts and work packages (`task add --wbs-role ...`).
2. Add milestones and durations. A work package can be scheduled directly (give it a
   duration) or split into activities under it; split when one person or crew cannot own it.
3. Add logic (`dep add`). Every activity needs a predecessor and a successor, except the start
   and finish milestones.
4. Add resources and assignments (`res add`, `assign`), then `alma res ls`. When a resource is
   over-allocated (`L-RES-01`), pick a fix: `alma level` delays tasks until people are free
   (the finish may move), `alma level --mode smoothing` only uses free float (the finish stays),
   or change units, capacity or logic. `alma res usage <id>` shows demand vs capacity per week.
5. Run `alma check --json` and fix what it reports; `alma explain <id>` says why a date is
   where it is. Repeat until there are no `error` or `warn` diagnostics.
6. Look at the result: `alma export gantt.png` and open the image. Critical work is red,
   summaries are black brackets, milestones are diamonds, sprints are shaded bands.
7. When the plan is agreed, freeze it: `alma baseline save b1 --name "Contract"`. From then on
   `alma variance --changed` shows what slipped or got more expensive, and `alma scurve`
   shows planned value next to the forecast. Baselines are never overwritten: a revision is a
   new id with `--change CR-…`. Commit `.alma/baselines/` with the plan.

For many changes at once, write the operations to a file and run `alma apply ops.json`
(all or nothing). `alma schema` prints the JSON Schema of operations.

## Commands

```
alma init plan.alma.toml --name "Project" --start 2027-01-04
alma task add <id> --name "..." [--parent P] [--after S] [--duration 15d] [--kind milestone]
               [--wbs-role work_package] [--owner pm] [--deadline 2027-10-29]
               [--constraint SNET:2027-03-01] [--field key=value] [--upsert]
alma task set <id> [--duration 10d] [--name "..."] [--estimate 25d/30d/40d --basis "why"]
               [--field key=value] [--field estimate.dist=triangular] [--unset key]
alma task move <id> [--parent P] [--after S]
alma task rm <id> [--cascade]            # refuses if others depend on it
alma dep add <pred> <succ> [--type FS|SS|FF|SF] [--lag 2d] [--nature discretionary]
               [--rationale "why"]
alma dep rm <pred> <succ>
alma res add <id> [--name "..."] [--rate 4.2/day] [--max 3] [--kind equipment]
alma res set <id> [--rate 5/day] [--max 4] [--calendar ID] [--unset rate]
alma assign <task> <resource> [--units 0.5]
alma unassign <task> <resource>
alma level [--mode leveling|smoothing|off]   # saved in the plan; every schedule is leveled
alma cal add <id> [--workdays mon,tue,wed,thu,fri] [--hours 09:00-13:00,14:00-18:00]
alma cal except <calendar> --name "Holiday" --from 2027-09-17 [--to 2027-09-20] [--working]
alma project set [--calendar ID] [--deadline 2027-12-10] [--name "..."]
alma settings set key=value ... [--unset key]   # [settings.schedule]: hours_per_day, phase, …
alma rule set L-WBS-05 off|info|warn|error      # this plan only; prefer `allow` per task
alma iter add sprints --prefix s --start 2027-04-05 --length 10d --repeat 12   # s01..s12
alma iter rm sprints
alma apply ops.json | -                  # batch (JSON array or JSON lines)
alma fmt [--check]                       # canonical layout; comments kept

alma export gantt.png [--range 2027-01..2027-06] [--depth 2] [--scale week] [--dark]
alma export gantt.svg | plan.csv | - --format csv
alma export dev.png --chart histogram --resource dev [--period week]   # demand vs capacity
alma export curve.png --chart scurve [--period month] [--baseline b1]  # forecast vs planned

alma check [--min warn] [--limit N --cursor C]
alma ls [--parent P] [--critical] [--kind milestone] [--fields id,name,start,finish,tf]
alma show <id>
alma critical
alma explain <id>                        # names the resource a leveled task waited for
alma res ls                              # resources, booked hours, over-allocations
alma res usage <id> [--period day|week|month]   # demand vs capacity per period
alma cost [--by wbs|resource|category] [--depth 2]   # what the plan costs, and where
alma reserve add <id> --amount 300 [--kind contingency|management] [--scope task] [--basis "..."]
alma reserve rm <id>
alma scurve [--period month] [--csv] [--baseline b1]   # forecast and planned value per period
alma baseline save <id> [--name "..."] [--change CR-12] [--reason "..."] [--date 2027-01-04]
alma baseline ls
alma variance [--baseline b1] [--changed] [--limit N]   # dates and cost vs the baseline
alma diff [--rev HEAD | --against old.alma.toml]   # what moved since then
alma keys [task|project|calendar|resource|cadence|task.deps|task.estimate ...]   # field reference
alma schema
alma guide
```

## Values

- Durations: `15d`, `4h`, `2w`, `30min` in working time (`1d` = 8 working hours by default);
  `3ed`, `2eh` in elapsed time. Lags may be negative (`-2d`, a lead). No months.
- Dates: `YYYY-MM-DD`.
- Rates: `AMOUNT/PER` with PER `hour`, `day` (= `hours_per_day`), `week`, `month` (52/12
  weeks) or `use`. A time rate costs units × working hours; a `use` rate costs units × amount
  once (a fee, or a quantity × unit price for materials). Fixed costs: `--field
  fixed_cost.amount=180 --field fixed_cost.accrue=start --field fixed_cost.category=investment`.
- Reserves: contingency (identified risks) is part of the cost baseline and accrues at the
  finish of its scope; management reserve (the unforeseen) is outside it, in the budget only.
  `alma cost` shows the stack: work + contingency = cost baseline; + management = budget.
  Give every contingency a `--basis`. Time reserves are buffer tasks (`--kind buffer`).
- Ids: lowercase letters, digits and hyphens, unique across the whole plan.
- `kind`: `task` (default), `milestone`, `loe` (level of effort), `buffer` (time reserve).
  A task with children is a summary; do not give it a `kind`.
- `wbs_role`: `deliverable`, `control_account`, `work_package`, `planning_package`.
- Contract dates: use `--deadline` on the milestone, not a hard constraint. A missed deadline
  shows as negative float (`L-SCH-05`) without moving the task.

## Things alma does not do yet

- Tasks cannot be linked to a sprint (`iteration` is reserved for M4). To align work with
  sprints, start it with `--constraint SNET:<sprint start>` or link it to a milestone.
- A resource has one capacity (`--max`) for the whole project, and its calendar is not combined
  with the task calendar yet. For a team that grows or shrinks, use two resources.
- Leveling picks tasks by least float; other priority rules come later. To force an order,
  add a dependency with a rationale.
- The 8–80 h work package rule (`L-WBS-05`) suits small plans. For large work packages, split
  them into activities, or turn the rule off for the plan with `alma rule set L-WBS-05 off`
  and say why in your summary.

## Reading results

- Mutations return `changed` (tasks whose dates moved, in working days), `end_date_delta`,
  `cost_delta` when the total cost changed,
  `critical_path_changed` (`added`, `removed`) when the critical path changed, and
  `new_diagnostics`. `noop: true` means the command was already applied. Changing the leveling
  mode also returns `leveling`: each delayed task, the resource it waited for and the task
  holding it, and what is still over-allocated.
- With leveling on, float counts resources: a task that holds people another critical task
  waits for is critical too.
- Diagnostics have a stable `code`, the `ids` involved, a `hint`, and a `fix` command only when
  running it blind is safe. `E-*` codes are hard errors; `L-*` codes are lint rules that the plan
  can configure under `[rules.severity]` or silence per task with `allow = ["L-..."]`.
- Exit codes: 0 ok, 1 the plan or the change has errors, 2 invalid command.

`alma keys <table>` lists every key a table accepts and which ones are reserved for later
milestones; the full format reference is `docs/format.md` in the repository.
