# alma — guide for agents

`alma` edits and checks a project plan stored in one TOML file (`*.alma.toml`). Every command
takes `--json` (structured output) and every change takes `--dry-run` (show the effect, write
nothing). A change either applies fully and leaves a valid file, or changes nothing.

Pick the file with `--file plan.alma.toml`, `ALMA_FILE`, or run in a directory with one plan.

## Recommended loop

0. Set up time: `cal add` for working days and hours, `cal except` for holidays and blackout
   periods, `project set --calendar ID`. The default calendar is Mon–Fri, 09:00–13:00 and
   14:00–18:00.
1. Build the WBS first: control accounts and work packages (`task add --wbs-role ...`).
2. Add milestones and durations. A work package can be scheduled directly (give it a
   duration) or split into activities under it; split when one person or crew cannot own it.
3. Add logic (`dep add`). Every activity needs a predecessor and a successor, except the start
   and finish milestones.
4. Run `alma check --json` and fix what it reports; `alma explain <id>` says why a date is
   where it is. Repeat until there are no `error` or `warn` diagnostics.
5. Look at the result: `alma export gantt.png` and open the image. Critical work is red,
   summaries are black brackets, milestones are diamonds, sprints are shaded bands.

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
alma res add <id> [--name "..."] [--rate 4.2/day] [--max 3]
alma assign <task> <resource> [--units 0.5]
alma unassign <task> <resource>
alma cal add <id> [--workdays mon,tue,wed,thu,fri] [--hours 09:00-13:00,14:00-18:00]
alma cal except <calendar> --name "Holiday" --from 2027-09-17 [--to 2027-09-20] [--working]
alma project set [--calendar ID] [--deadline 2027-12-10] [--name "..."]
alma rule set L-WBS-05 off|info|warn|error      # this plan only; prefer `allow` per task
alma iter add sprints --prefix s --start 2027-04-05 --length 10d --repeat 12   # s01..s12
alma iter rm sprints
alma apply ops.json | -                  # batch (JSON array or JSON lines)
alma fmt [--check]                       # canonical layout; comments kept

alma export gantt.png [--range 2027-01..2027-06] [--depth 2] [--scale week] [--dark]
alma export gantt.svg | plan.csv | - --format csv

alma check [--min warn] [--limit N --cursor C]
alma ls [--parent P] [--critical] [--kind milestone] [--fields id,name,start,finish,tf]
alma show <id>
alma critical
alma explain <id>
alma diff [--rev HEAD | --against old.alma.toml]   # what moved since then
alma keys [task|project|calendar|resource|cadence|task.deps|task.estimate ...]   # field reference
alma schema
alma guide
```

## Values

- Durations: `15d`, `4h`, `2w`, `30min` in working time (`1d` = 8 working hours by default);
  `3ed`, `2eh` in elapsed time. Lags may be negative (`-2d`, a lead). No months.
- Dates: `YYYY-MM-DD`.
- Ids: lowercase letters, digits and hyphens, unique across the whole plan.
- `kind`: `task` (default), `milestone`, `loe` (level of effort), `buffer` (time reserve).
  A task with children is a summary; do not give it a `kind`.
- `wbs_role`: `deliverable`, `control_account`, `work_package`, `planning_package`.
- Contract dates: use `--deadline` on the milestone, not a hard constraint. A missed deadline
  shows as negative float (`L-SCH-05`) without moving the task.

## Things M0 does not do yet

- Tasks cannot be linked to a sprint (`iteration` is reserved for M4). To align work with
  sprints, start it with `--constraint SNET:<sprint start>` or link it to a milestone.
- Resource over-allocation is not checked and there is no leveling until M2: check the
  assignment units yourself against each resource's `--max`.
- The 8–80 h work package rule (`L-WBS-05`) suits small plans. For large work packages, split
  them into activities, or turn the rule off for the plan with `alma rule set L-WBS-05 off`
  and say why in your summary.

## Reading results

- Mutations return `changed` (tasks whose dates moved, in working days), `end_date_delta`
  and `new_diagnostics`. `noop: true` means the command was already applied.
- Diagnostics have a stable `code`, the `ids` involved, a `hint`, and a `fix` command only when
  running it blind is safe. `E-*` codes are hard errors; `L-*` codes are lint rules that the plan
  can configure under `[rules.severity]` or silence per task with `allow = ["L-..."]`.
- Exit codes: 0 ok, 1 the plan or the change has errors, 2 invalid command.

`alma keys <table>` lists every key a table accepts and which ones are reserved for later
milestones; the full format reference is `docs/format.md` in the repository.
