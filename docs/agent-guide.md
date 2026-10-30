# alma — guide for agents

`alma` edits and checks a project plan stored in one TOML file (`*.alma.toml`). Every command
takes `--json` (structured output) and every change takes `--dry-run` (show the effect, write
nothing). A change either applies fully and leaves a valid file, or changes nothing.

Pick the file with `--file plan.alma.toml`, `ALMA_FILE`, or run in a directory with one plan.

## Recommended loop

1. Build the WBS first: control accounts and work packages (`task add --wbs-role ...`).
2. Add activities and milestones under the work packages, with durations.
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
alma task set <id> [--duration 10d] [--name "..."] [--unset key]
alma task move <id> [--parent P] [--after S]
alma task rm <id> [--cascade]            # refuses if others depend on it
alma dep add <pred> <succ> [--type FS|SS|FF|SF] [--lag 2d] [--nature discretionary]
               [--rationale "why"]
alma dep rm <pred> <succ>
alma res add <id> [--name "..."] [--rate 4.2/day] [--max 3]
alma assign <task> <resource> [--units 0.5]
alma unassign <task> <resource>
alma apply ops.json | -                  # batch (JSON array or JSON lines)

alma export gantt.png [--range 2027-01..2027-06] [--depth 2] [--scale week] [--dark]
alma export gantt.svg | plan.csv | - --format csv

alma check [--min warn] [--limit N --cursor C]
alma ls [--parent P] [--critical] [--kind milestone] [--fields id,name,start,finish,tf]
alma show <id>
alma critical
alma explain <id>
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

## Reading results

- Mutations return `changed` (tasks whose dates moved, in working days), `end_date_delta`
  and `new_diagnostics`. `noop: true` means the command was already applied.
- Diagnostics have a stable `code`, the `ids` involved, a `hint`, and a `fix` command only when
  running it blind is safe. `E-*` codes are hard errors; `L-*` codes are lint rules that the plan
  can configure under `[rules.severity]` or silence per task with `allow = ["L-..."]`.
- Exit codes: 0 ok, 1 the plan or the change has errors, 2 invalid command.

The file format reference is `docs/format.md`.
