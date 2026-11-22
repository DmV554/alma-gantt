# Scope/WBS, scheduling and agile/hybrid: what the sources say and what alma-gantt should do

**Status:** research note, 2026-10-29. **Feeds:** `SPEC.md` §6 (domain model), §7 (engine), §8a (agent
interface), lint rules (§4 item 8).

**Sources used** (all read in full through the `terabyte-canon` knowledge base before citing):

| Id | Document |
|---|---|
| PMBOK 6 | *A Guide to the Project Management Body of Knowledge*, 6th ed., chapters 5.4 and 6 (plus glossary and §7.2.2.5, §7.3.2.5) |
| WBS Practice Standard | *Practice Standard for Work Breakdown Structures*, 2nd ed. |
| APG | *Agile Practice Guide* |
| FEP02 | Course slides "Requisitos, Alcance, Planificación y EDT" (sections 4 and 5) |
| FEP04, FEP06 | Two slides from other course decks, used once each where the professor ties the Gantt to risk responses and payment milestones |

**Copyright note.** These works are copyrighted. Everything below is a paraphrase written for this
project. Direct quotes are short (under 15 words), in quotation marks and cited. No table or figure is
reproduced. Each citation gives the `cita` string returned by the tool and the fragment id, so a
maintainer with access to the books can check it.

**How to read each entry.**
- **What it is**: the concept in our own words.
- **Implication for alma-gantt**: a concrete decision (model field, engine behavior, CLI, view,
  export or lint rule). Lint rules get a code `L-<AREA>-nn`, a default severity
  (`error` / `warn` / `info`) and a description of the check. Codes are proposals; the final numbering
  belongs in an ADR.
- **Source**: citation + fragment id.

---

## Part 1. Scope and the WBS

### 1.1 The WBS as a deliverable-oriented hierarchy

**What it is.** The work breakdown structure splits the *whole* approved scope into a tree of smaller,
manageable components. Its elements are deliverables (products, services, results) and the work to
produce them, not actions. The WBS orders scope; it does not add or remove any. If an element appears
that the scope statement does not contain, one of the two documents is wrong.

**Implication for alma-gantt.**
- The task tree in `SPEC.md` §6 is used as the WBS. That is fine, but the model must say which nodes
  are *WBS elements* (deliverables) and which are *schedule activities* (actions that produce a work
  package). See 1.6 and gap G-01.
- Add an optional `deliverable` (noun phrase) and `scope_ref` (link to requirement / scope statement
  item) on WBS elements, so the agent can trace every element back to scope.
- `alma wbs` view/export: the tree with codes, without dates (see 1.3).

**Source.** PMBOK 6, §5.4 (PMBOK6-0331); PMBOK 6, §5.4.3.1 (PMBOK6-0342); FEP02, p. 50 del PDF
(FEP02-0050); WBS Practice Standard, p. 121 del PDF, glossary (WBS2-0272).

### 1.2 The 100 % rule

**What it is.** The WBS covers all the work in scope and nothing outside it, including project
management and interim and internal deliverables. The rule holds at every level: the children of a node
add up to exactly the work of their parent. It also holds one level further down: the activities in a
work package add up to all the work of that package. Less than 100 % means orphan work that will show up
later with no owner and no budget; more than 100 % means duplicated work that is estimated and done
twice.

**Implication for alma-gantt.**
- The rule is about *work content*, which the engine cannot know. What it can check are its symptoms:
  - `L-WBS-01` (`warn`): a WBS element with exactly one child. Either the child repeats the parent
    or siblings are missing.
  - `L-WBS-02` (`warn`): the plan has no decomposition (fewer than two WBS levels).
  - `L-WBS-08` (`info`): no element is tagged as project-management work (`tags = ["pm"]` or
    `kind = "loe"`), although the 100 % rule includes it.
  - `L-WBS-09` (`warn`): two WBS elements with the same normalized name (likely duplication).
- Effort/cost roll-up is computed bottom-up (never typed by hand on a summary). If a summary carries
  its own `duration`, `effort` or `fixed_cost` in addition to its children, report `L-WBS-13`
  (`warn`, "summary carries its own work; this breaks the 100 % rule") unless it is explicitly marked
  `kind = "loe"` or `"planning_package"`.
- `alma wbs check --rule 100` prints, for each parent, its children and their share of effort/cost, so a
  human (or the agent) can review completeness. This is a review aid, not proof.

**Source.** WBS Practice Standard, §2.4 (WBS2-0033); PMBOK 6, §5.4.2.2 (PMBOK6-0340); WBS Practice
Standard, §5.3 (WBS2-0107); FEP02, p. 52 del PDF (FEP02-0052).

### 1.3 The WBS has no sequence

**What it is.** The WBS is a static view of scope. It has no dates, no durations and no precedence
arrows, and the left-to-right order of the drawing means nothing. Sequence appears only after work
packages are broken into activities and those activities are linked in the schedule. The professor
calls confusing the WBS with the schedule the most frequent mistake.

**Implication for alma-gantt.**
- WBS tree export (`alma export --wbs-svg`) draws no dates or arrows.
- Dependencies should connect activities and milestones, not WBS summary elements. `SPEC.md` §7.2
  currently lets dependencies on summary tasks "expand to their children". Keep that for import
  compatibility (MS Project allows it), but flag it:
  `L-DEP-09` (`warn`): "dependency attached to a WBS summary element; link the activities or a
  milestone instead". The fix hint should propose the expansion explicitly (one dep per child, or a
  start/finish milestone).
- The sibling order in the file is display order only. The engine must never infer sequence from it
  (this matches the `SPEC.md` §12 property "schedule is invariant to task order in the file").

**Source.** FEP02, p. 53 del PDF (FEP02-0053).

### 1.4 Decomposition criteria: deliverable- vs phase-oriented first level

**What it is.** The first level of the WBS can be organized by major deliverables, by life-cycle
phases (deliverables then sit at level 3), by subprojects or contracts (useful with several suppliers;
the seller builds a contract WBS for its part), or by discipline. All are valid. The professor
recommends deliverables because they connect directly to the scope statement. Phases fit schedules and
payment milestones but hide individual deliverables. Whatever the first level is, the lowest level is
always work packages. The WBS Practice Standard adds that the right logic depends on the organization:
functional lines, product life cycle stages, geography or subsystems.

**Implication for alma-gantt.**
- Project setting `wbs.orientation = "deliverable" | "phase" | "subproject" | "discipline"`
  (informational, used by `alma guide` to steer the agent and by the WBS export legend).
- When `orientation = "deliverable"`, `L-WBS-03` (`info`) flags level-1/2 WBS element names that start
  with a verb (see 1.8).
- `kind = "subproject"` (or `external = true` on a WBS element) for subcontracted branches whose
  inner WBS belongs to a seller. These should map one-to-one to the seller's WBS (WBS Practice Standard).

**Source.** PMBOK 6, §5.4.2.2 (PMBOK6-0338); FEP02, p. 54 del PDF (FEP02-0054); WBS Practice Standard,
§5.4.3 (WBS2-0128); WBS Practice Standard, §5.3 (WBS2-0106).

### 1.5 Depth: when to stop decomposing

**What it is.** Decompose until the cost and duration of each work package can be estimated and
managed with confidence: no further. How deep depends on the control needed, the size, complexity and
risk of the project. Branches do not need the same depth, and work packages need not all sit at the
same level. Going too deep wastes management effort, uses resources poorly and makes roll-ups harder.
The WBS Practice Standard lists questions that justify splitting further. Examples: the element holds
more than one deliverable, parts of it have different prerequisites, a part could be scheduled as a
unit, there are acceptance criteria before the whole element is done, or progress cannot be measured
objectively. The professor adds two sizing rules of thumb:
- **8/80 rule:** a work package takes between 8 and 80 hours of effort (at most about two weeks of
  one person).
- **Reporting-period rule:** a work package fits inside one reporting period. Otherwise it sits at
  "50 %" for weeks and nobody knows whether it is on track.

**Implication for alma-gantt.**
- No fixed depth limit. Asymmetric trees are valid and must render well.
- `L-WBS-05` (`warn`): a work package whose total effort is above `rules.wp_effort_max` (default
  80 h) or below `rules.wp_effort_min` (default 8 h). Only checked when effort is known.
- `L-WBS-06` (`warn`): a work package (or a leaf activity, configurable) whose duration is longer than
  `settings.reporting_period` (for example `"2w"`).
- `L-WBS-14` (`info`, off by default): a leaf with several distinct `deliverable` values, or with
  internal dependencies to different external elements. These are hints that the element should be
  split.

**Source.** PMBOK 6, §5.4.2.2 (PMBOK6-0337, PMBOK6-0339); WBS Practice Standard, §5.3 (WBS2-0107);
WBS Practice Standard, §5.4.1 (WBS2-0120, WBS2-0121); WBS Practice Standard, p. 48 del PDF
(WBS2-0122); WBS Practice Standard, §4.2.2 (WBS2-0071, WBS2-0072); FEP02, p. 55 del PDF
(FEP02-0055); FEP02, p. 56 del PDF (FEP02-0056).

### 1.6 Work packages, activities and the boundary between them

**What it is.** The work package is the lowest level of the WBS. It is the unit where cost and
duration are estimated and managed, and it gets a unique identifier. Inside the WBS it is not broken
down further. *Outside* the WBS, the Define Activities process breaks each work package into
activities (actions) that are sequenced, estimated and resourced. A work package may also be run as a
subproject with its own WBS (typically when a third party performs it). The WBS Practice Standard
defines a work package as including the schedule activities and milestones needed to complete it.

**Implication for alma-gantt.**
- Add a WBS role to every node:
  `wbs_role = "element" | "control_account" | "planning_package" | "work_package" | "activity" | "milestone"`.
  Summary/element/control account/planning package/work package are WBS nodes; activities and
  milestones are schedule nodes that hang under a work package. `kind` (task/summary/milestone) stays
  as the scheduling shape; `wbs_role` is the scope meaning. A plan that does not care can leave the
  role inferred (leaf = work package, no activities).
- WBS code (1.2.3) is assigned to WBS nodes only. Activities get their parent's code plus a local
  suffix, or no code (setting `wbs.code_activities = true|false`).
- `alma wbs` (WBS view) stops at work packages and hides activities. `alma ls`/Gantt shows both.
- `L-SCH-03` (`warn`): a work package with no activities, no duration and no effort (nothing to
  schedule).

**Source.** PMBOK 6, §5.4 (PMBOK6-0331); PMBOK 6, §6.2.2.2 (PMBOK6-0394); WBS Practice Standard,
§2.2.1 (WBS2-0025); FEP02, p. 56 del PDF (FEP02-0056); FEP02, p. 65 del PDF (FEP02-0065).

### 1.7 Control accounts and planning packages

**What it is.** A *control account* is a management point where scope, budget, actual cost and
schedule are integrated and compared with earned value. It sits at chosen points of the WBS. Each work
package belongs to exactly one control account, and each control account belongs to one organizational
unit. The sources differ on the minimum: PMBOK 6 says a control account has two or more work packages,
the WBS Practice Standard says one or more. A *planning package* sits below a control account and
above work packages. Its work content is known but it has no detailed activities yet. It is the vehicle
for rolling-wave planning (1.11).

**Implication for alma-gantt.**
- `wbs_role = "control_account"` with an optional `org_unit` (responsible organization) field. EVM
  roll-ups (M4) are reported per control account.
- `L-WBS-12` (`warn`, active only if the plan uses any control account): a work package with no
  control account ancestor. Two control accounts on the same path (nested) is an `error`, because a
  work package would belong to two.
- `L-WBS-15` (`info`): a control account with fewer than `rules.ca_min_wp` work packages (default 1,
  which follows the WBS Practice Standard. Set 2 for the PMBOK 6 reading).
- `wbs_role = "planning_package"` may carry a rough duration/effort/cost (top-down) and no children.
  See `L-WBS-11` in 1.11.

**Source.** PMBOK 6, §5.4.3.1 (PMBOK6-0342); WBS Practice Standard, §2.2.1 (WBS2-0024).

### 1.8 Quality characteristics of a good WBS

**What it is.** The WBS Practice Standard separates *core* characteristics (a WBS has them or it does
not) from *use-related* ones (they depend on the project). Core characteristics include: deliverable-
oriented; 100 % at every level; covers internal, external and interim deliverables, including
project management; elements named with nouns and adjectives, not verbs; a coding scheme that shows
the hierarchy in any format; at least two levels; built by the people who will do the work, with
expert input; and it evolves with scope until baselined, then changes only through change control.
Use-related characteristics include: enough depth to manage (but not so much that control costs too
much); elements fit for tracking and control; one accountable person or unit per element (linked to
the OBS through a responsibility matrix); compatible with accounting structures; and it may include
LOE, integration, procurement, training or documentation elements. The general tenets add that each
element has exactly one parent, deliverables are unique, and all significant reporting items (reviews,
monthly reports, test reports) appear in the WBS.

**Implication for alma-gantt.**
- One parent per element is guaranteed by the data model (`parent` key).
- Coding scheme: already derived (`SPEC.md` §6). Keep it stable and exportable in every view
  (Gantt grid column, WBS tree, CSV, XLSX), because the code is what links scope, cost and progress.
- `L-WBS-03` (`info`): WBS element (not activity) whose name starts with a verb. Use small per-language
  verb lists (en/es), configurable, and suppressible per element. It is a heuristic, so `info`.
- `L-WBS-04` (`warn`): a work package without exactly one accountable `owner`. An `owner` that is an
  organizational unit rather than a person also triggers a hint; see the FEP04 rule in 2.13.
- `alma wbs review` (read-only) prints a checklist report: the core characteristics alma can verify
  (levels, codes, owners, single child, duplicates, PM branch) plus the ones it cannot (marked "human
  check"). This gives the agent a structured self-review step.

**Source.** WBS Practice Standard, §4.2.1 (WBS2-0067, WBS2-0068, WBS2-0069); WBS Practice Standard,
§4.2.2 (WBS2-0070 to WBS2-0075); WBS Practice Standard, §5.3 (WBS2-0106, WBS2-0107); WBS Practice
Standard, §4.6 (WBS2-0086).

### 1.9 Common WBS defects (diagnostic checklist)

**What it is.** The WBS Practice Standard links project symptoms to WBS defects. Missed deadlines can
come from deliverables left out or defined too vaguely. Budget overruns can come from a WBS without
summary points where cost and schedule can be measured. Unusable products can come from missing
training, integration or test deliverables. Scope that never ends can come from a WBS with no closeout
or no clear end point. Confused responsibilities can come from overlapping ownership. One of its
questions is whether decomposition was finished *before* dependencies and durations were set.

**Implication for alma-gantt.**
- `L-WBS-10` (`info`): the plan has no final milestone (a zero-duration node with no successors, at
  the end of the network). This is the "specific end point" symptom.
- `L-WBS-16` (`info`): no element tagged `closeout` (or named like close/cierre) in the WBS.
- Ordering hint for agents in `alma guide`: build the WBS first, then activities, then logic, then
  durations. `alma apply` batches should follow that order. This is a recommended workflow, not a hard
  rule.

**Source.** WBS Practice Standard, §4.5 (WBS2-0082 to WBS2-0085).

### 1.10 WBS dictionary

**What it is.** The WBS shows only names. The dictionary explains each element: what work it holds
(and what it leaves out), the deliverable, acceptance criteria, responsible person or organization,
related milestones and activities, resources, effort and cost estimates, assumptions and constraints,
quality requirements, technical references and contract data (for example a charge number). Writing
the dictionary often exposes ambiguity and leads to WBS revisions. It may also hold traceability to
requirements or statements of work, and implied dependencies before they become schedule logic.

**Implication for alma-gantt.**
- WBS dictionary fields on WBS nodes (all optional, in this order in the TOML so diffs stay stable):
  `description`, `excludes`, `deliverable`, `acceptance`, `owner`, `org_unit`, `assumptions[]`,
  `constraints_note`, `quality_reqs`, `references[]` (requirement ids, documents), `contract_ref`
  (charge number / agreement). Milestones, activities, effort, cost and resources are *derived* from
  the tree and assignments and must not be duplicated as text.
- `alma export --wbs-dictionary (md|csv|xlsx|typst)` generates the dictionary from the plan, one entry
  per work package, with derived fields filled in. This keeps the WBS, the dictionary and the Gantt
  consistent by construction.
- `L-WBS-07` (`info`): a work package missing `description`, `deliverable` or `acceptance`. The list of
  required fields is configurable (`rules.wbs_dictionary_required`).

**Source.** PMBOK 6, §5.4.3.1 (PMBOK6-0343); WBS Practice Standard, p. 28 del PDF (WBS2-0052,
WBS2-0054); FEP02, p. 57 del PDF (FEP02-0057).

### 1.11 Code of accounts (WBS numbering)

**What it is.** Each WBS component gets a unique hierarchical code (1, 1.2, 1.2.3). Activities hang
from their package's code, cost items are charged to the same code, and progress is reported against
the code rather than the name. The code is what lets one report join scope, time and cost.

**Implication for alma-gantt.**
- Keep IDs (stable slugs) and WBS codes (derived, positional) separate, as `SPEC.md` §6 already does.
- Add optional *frozen* codes: after `alma baseline save`, the baseline records each element's code, and
  `alma diff --baseline` reports renumbering. Cost and progress reports for a contract usually must
  keep the codes that were approved.
- Export option `--code-format "1.2.3" | "01.02.03" | custom prefix`.

**Source.** FEP02, p. 58 del PDF (FEP02-0058); PMBOK 6, §5.4.3.1 (PMBOK6-0342); WBS Practice
Standard, §4.4.4 (WBS2-0081).

### 1.12 Rolling-wave planning

**What it is.** Near-term work is planned in detail and later work is kept at a higher level until
more is known. It is a form of progressive elaboration that applies to work packages, planning
packages and release planning, in both agile and predictive projects. As a result, parts of the WBS sit
at different depths at the same time. The length of each "wave" and how often the schedule is
reviewed are decisions for the schedule management plan.

**Implication for alma-gantt.**
- Planning packages (1.7) are the explicit representation of a not-yet-detailed wave.
- Setting `settings.rolling_wave_horizon` (for example `"6w"`).
  `L-WBS-11` (`warn`): a planning package whose early start falls within the horizon from the status
  date (or project start), meaning it should be decomposed now.
- `alma task split <id> --into ...` / `alma wbs elaborate <planning-package>`: turns a planning package
  into a work package with activities. It keeps the original rough estimate as `estimate.top_down` so
  the bottom-up result can be compared with it.

**Source.** PMBOK 6, §6.2.2.3 (PMBOK6-0395); PMBOK 6, §5.4.2.2 (PMBOK6-0340); WBS Practice Standard,
§5.4.1 (WBS2-0120); PMBOK 6, §6.1.2.2 (PMBOK6-0384).

### 1.13 Scope baseline and WBS change control

**What it is.** The scope baseline is the approved scope statement + WBS + WBS dictionary. After
approval it changes only through formal change control and serves as the reference for comparison.
Every approved scope change requires updating the dictionary, the network and the schedule. When
activity definition reveals work that was not in the baseline, that becomes a change request.

**Implication for alma-gantt.**
- `alma baseline save` should snapshot *scope* too (tree, codes, dictionary fields), not only dates and
  costs. `alma diff --baseline` then reports added, removed or moved WBS elements, separately from date
  variances.
- Baseline metadata: `approved_by`, `approved_on`, `change_ref` (change request id). A rebaseline
  without `change_ref` triggers `L-BAS-05` (`info`).

**Source.** PMBOK 6, §5.4.3.1 (PMBOK6-0341); WBS Practice Standard, §5.3 (WBS2-0108); PMBOK 6,
§6.2.3.4 (PMBOK6-0400); PMBOK 6, §6.2.3.5 (PMBOK6-0401).

---

## Part 2. Activities, milestones and logic

### 2.1 Schedule model vs schedule vs scheduling tool

**What it is.** PMBOK 6 separates three things. The *scheduling method* is, for example, critical path
or agile. The *schedule model* is the representation of the plan: activities, durations, dependencies,
constraints and other data. The *project schedule* is the output that model produces (dates, bars,
milestones). The *scheduling tool* provides the components and formats that support the method. The
early and late dates CPM computes are windows in which work could run, not the schedule itself.

**Implication for alma-gantt.**
- This maps exactly onto `SPEC.md`: the `.alma.toml` file is the schedule model; computed values are
  never stored in it; exports are the schedule. State this vocabulary in `alma guide` so agents use the
  PMBOK terms.
- `alma show` should label ES/EF/LS/LF as "windows" and planned/scheduled dates (after leveling) as
  "scheduled". Leveled dates may differ from ES/EF.

**Source.** PMBOK 6, p. 193 del PDF (PMBOK6-0371); PMBOK 6, p. 707 del PDF, glossary
(PMBOK6-1585); PMBOK 6, §6.5.2.2 (PMBOK6-0450).

### 2.2 Activity list and activity attributes

**What it is.** The activity list holds every scheduled activity with an identifier and a work
description clear enough for the performer. Attributes extend it over time. At first they are the ID,
WBS ID and name. Later they add description, predecessors and successors, relationship types, leads
and lags, resource needs, imposed dates, constraints, assumptions, location, calendar, responsible
person and type of effort. Attributes are also used to sort, filter and group activities in reports.
The professor stresses that an activity without an owner or predecessors "floats" and only shows up
when it is already blocking another.

**Implication for alma-gantt.**
- Fields to add on activities if missing: `location`, `effort_type` (`discrete | apportioned | loe`,
  see 2.3), `assumptions[]`, `basis` (basis of estimate, 3.5).
- `alma ls --group-by <attr>` and `--filter` on any attribute, including custom fields (PMBOK names
  reporting by attributes as one of their uses).
- `L-SCH-04` (`warn`): activity with no `owner` (and no assignment).

**Source.** PMBOK 6, §6.2.3.1 (PMBOK6-0397); PMBOK 6, §6.2.3.2 (PMBOK6-0398); FEP02, p. 65 del PDF
(FEP02-0065).

### 2.3 Level of effort and apportioned effort

**What it is.** *Level of effort* (LOE) is supporting work (project management, liaison, cost
accounting) that produces no definitive product. It runs at a steady rate for as long as the work it
supports lasts. *Apportioned effort* is work that cannot be split into discrete pieces but is
proportional to some discrete work (for example inspection as a share of construction). *Discrete
effort* is planned and measured directly against a deliverable.

**Implication for alma-gantt.**
- `kind = "loe"` (a.k.a. hammock): its start and finish are *derived* from the activities it spans
  (`spans = { from = "<id>", to = "<id>" }`). It is excluded from the critical path and does not drive
  any date. This is how project management work (100 % rule) gets onto the Gantt and into the S-curve
  without distorting the logic.
- `effort_type = "apportioned"` with `apportion_to = "<id>", factor = 0.1` for cost/effort roll-up.
- Lint: an LOE with predecessors or successors is an `error` (`L-SCH-09`), because it would drive dates.

**Source.** WBS Practice Standard, §2.2.1 (WBS2-0024, WBS2-0025); WBS Practice Standard, §4.2.2
(WBS2-0074).

### 2.4 Milestones and the milestone list

**What it is.** A milestone is a significant point or event, with zero duration: it uses no time and no
resources. The milestone list says for each one whether it is *mandatory* (required by contract or the
call for bids) or *optional* (set by the team, for example from history). The professor adds that a
milestone has a clear, objective, ideally measurable criterion, an owner, and a binary status (met or
not met). He also distinguishes three kinds. *External* milestones are committed to the client and far
apart. *Internal* ones are set by the team for early detection. *Miniature* (inch-pebble) ones are one
or two days apart, binary, and cover all the work. External milestones are the ones that get signed;
internal and miniature ones warn in time that an external one will be missed. The project charter
already carries a summary milestone schedule.

**Implication for alma-gantt.**
- Milestone fields: `mandatory: bool` (contractual), `visibility = "external" | "internal"`,
  `criterion` (acceptance condition), `owner`, optional `deadline` (see 4.5), optional
  `payment_ref` (FEP06 traceability, 2.13).
- `L-SCH-01` (`error`): a milestone with non-zero duration, effort or assignments. Already in
  `SPEC.md` §4.
- `L-SCH-02` (`warn` for mandatory, `info` otherwise): a milestone without `criterion` or `owner`.
- `L-SCH-10` (`info`): an external milestone with no internal milestone or work-package finish
  within `rules.milestone_watch_window` before it (no early warning).
- Views: milestone chart export (`alma export --milestones`), filtered by `visibility` and `mandatory`.
  See 4.12.

**Source.** PMBOK 6, §6.2.3.3 (PMBOK6-0399); FEP02, p. 66 del PDF (FEP02-0066); FEP02, p. 65 del
PDF (FEP02-0065); PMBOK 6, §6.1.1.1 (PMBOK6-0379).

### 2.5 Precedence diagramming method (PDM) and the four relationship types

**What it is.** PDM builds the network with activities as nodes and logical relationships as links. A
link is always read from predecessor to successor, and the acronym names the predecessor's end first.
- **FS:** the successor cannot start until the predecessor finishes (by far the most common).
- **SS:** the successor cannot start until the predecessor starts.
- **FF:** the successor cannot finish until the predecessor finishes.
- **SF:** the successor cannot finish until the predecessor starts (rare; the typical case is that the
  old system shuts down only once the new one is running).

Two relationships between the same pair (for example SS plus FF) are possible but not recommended: pick
the one with the highest impact. Closed loops are not recommended either.

**Implication for alma-gantt.**
- Engine: all four types, with the formulas in Annex B. Already in `SPEC.md` §7.2.
- Help text and `alma explain` must phrase links as "<succ> cannot <start|finish> until <pred>
  <starts|finishes> [+lag]", which is the PMBOK reading. This removes the most common SF/FS confusion
  for both agents and humans.
- `L-DEP-04` (`warn`): more than one link between the same ordered pair. Fix hint: keep the
  most constraining one.
- `L-DEP-05` (`info`): SF link used. Confirm it is intended, because it is very rarely correct.
- Cycles remain an `error` (`E-DEP-01`, ADR 0008). The PMBOK warns against closed loops.

**Source.** PMBOK 6, §6.3.2.1 (PMBOK6-0407, PMBOK6-0408); FEP02, p. 67 del PDF (FEP02-0067).

### 2.6 Dependency attributes: mandatory/discretionary, internal/external

**What it is.** Each dependency has an *origin*, which decides whether it may be changed. The two axes
combine:
- **Mandatory** (hard logic): required by law or contract, or inherent to the work (you cannot test
  before building). They must not be confused with schedule constraints in the tool.
- **Discretionary** (preferred or soft logic): best practice or team preference, where other orders
  are possible. They must be fully documented, because they can create arbitrary float and limit
  later options. They are the first candidates to change when fast-tracking.
- **External**: links to work outside the project, usually beyond the team's control (a supplier
  delivery, a permit hearing).
- **Internal**: between project activities, under the team's control.

The professor's rule: only discretionary dependencies can be reordered to shorten the plan. Changing a
mandatory one does not compress the schedule, it breaks it. External dependencies should be written down
as assumptions, and they become risks if they fail.

**Implication for alma-gantt.**
- Extend the dependency record:
  `deps[] = { pred, type, lag, nature = "mandatory" | "discretionary", scope = "internal" | "external", rationale, assumption_ref }`.
  Defaults: `nature = "mandatory"`, `scope = "internal"`, so old files stay valid.
- External predecessors usually have no task in the plan: support `kind = "external"` milestones (an
  event owned by a third party, typically with an SNET date or a `forecast` date and no owner inside
  the team).
- `L-DEP-07` (`info`): an external dependency without `assumption_ref` or `rationale`.
- `L-DEP-08` (`info`): a discretionary dependency without `rationale`.
- `alma compress --fast-track` (4.9) proposes changes *only* on discretionary links on the critical
  path, and never touches mandatory ones.
- `alma critical --json` reports, for each critical link, its `nature`, so the agent knows what can
  move.

**Source.** PMBOK 6, §6.3.2.2 (PMBOK6-0409, PMBOK6-0410, PMBOK6-0411); FEP02, p. 68 del PDF
(FEP02-0068).

### 2.7 Leads and lags, and why they must not replace logic

**What it is.** A *lead* lets a successor start earlier relative to its predecessor (written as a
negative lag in software). A *lag* delays it (for example SS+15d). The project team decides which links
need them, and documents them. PMBOK 6 is explicit: "The use of leads and lags should not replace
schedule logic." Duration estimates never include lags. Lags are meant for a set period that must pass
between activities without work or resource impact (curing, waiting periods). The professor: a lag is
waiting, not work. It uses no hours or resources but it does use calendar days of the committed time.
Leads used to accelerate a schedule increase coordination effort and quality risk.

**Implication for alma-gantt.**
- Lags already support working vs elapsed time (`SPEC.md` §7.1). Add `rationale` on the dependency
  (2.6) and show lags in `alma explain` as an explicit "wait" segment.
- `L-DEP-06` (`info`): any lead (negative lag). Fix hint: if the overlap is real work, split the
  predecessor and link the part that matters.
- `L-DEP-11` (`warn`): a lag longer than `rules.max_lag` (default `"10d"`) or longer than the
  predecessor's duration. The hint suggests modelling the wait as an activity (for example
  "Supplier delivery") or an external milestone, so it can be owned, tracked and risk-managed.
- `L-DEP-12` (`warn`): a lag with no `rationale`.
- Engine: lags never carry effort or cost and never appear in resource histograms or the S-curve.

**Source.** PMBOK 6, §6.3.2.3 (PMBOK6-0412, PMBOK6-0413); PMBOK 6, §6.5.2.5 (PMBOK6-0457);
PMBOK 6, §6.4.3.1 (PMBOK6-0437); PMBOK 6, §6.5.2.6 (PMBOK6-0459); FEP02, p. 69 del PDF
(FEP02-0069).

### 2.8 Open ends (dangling activities)

**What it is.** Every activity except the first and the last should have at least one predecessor and
at least one successor with a suitable relationship. The professor puts it as: an activity without
predecessors cannot really be scheduled; it floats and shows up late.

**Implication for alma-gantt.**
- `L-DEP-01` (`warn`): an activity with no predecessor, other than the project start milestone (or
  the first node if no start milestone exists).
- `L-DEP-02` (`warn`): an activity with no successor, other than the project finish milestone.
- Subtleties to implement:
  - An SS-only predecessor or an FF-only successor leaves the *other* end open (the finish of a node
    whose only successor link is SS drives nothing). Report it as `L-DEP-03` (`info`, "open finish
    via SS-only successors" / "open start via FF-only predecessors").
  - Exclude LOE/hammock nodes, external milestones (no predecessor expected) and items inside a
    flow-based or iteration container (see 5.8), where open ends are normal.
  - A node "connected" only through a constraint (for example SNET) is still an open start: the
    constraint does not replace logic (2.6, 4.5).
- `alma check --json` returns `fix` hints: "alma dep add <best-guess-pred> <id>" is *not* safe to
  guess. Return `"fix": null` with `"suggest": "link to the activity that produces its input, or to
  the start milestone"`.

**Source.** PMBOK 6, §6.3 (PMBOK6-0402); FEP02, p. 65 del PDF (FEP02-0065).

### 2.9 Network diagram, path convergence and divergence

**What it is.** The project schedule network diagram shows the logical relationships among activities.
It can be detailed or include summary activities, and is often accompanied by a narrative that explains
any unusual sequence. An activity with several predecessors is a *path convergence*. One with several
successors is a *path divergence*. Activities that are both carry more risk, because many activities
affect them and they affect many. Schedule network analysis also considers adding aggregated reserve
where paths converge or diverge.

**Implication for alma-gantt.**
- View/export `alma export --network (svg|pdf)`: activity-on-node, layered left to right, with the
  node box ES | EF / name / LS | LF / TF (the box the professor uses). Two modes: *pure logic* (no time
  scale) and *time-scaled logic diagram*. See 4.12.
- `alma analyze --hotspots`: lists nodes ranked by in-degree × out-degree, flagging those on critical
  or near-critical paths. `L-DEP-10` (`info`, off by default): a node with ≥ `rules.hotspot_in`
  predecessors and ≥ `rules.hotspot_out` successors on a near-critical path.
- Plan-level `narrative` field (markdown) exported with the diagram.

**Source.** PMBOK 6, §6.3.3.1 (PMBOK6-0415); PMBOK 6, §6.5.2.1 (PMBOK6-0449); FEP02, p. 70 del PDF
(FEP02-0070); FEP02, p. 79 del PDF (FEP02-0079).

### 2.10 Project calendars and resource calendars

**What it is.** A project calendar says which days and shifts are available for work and which are
not. A schedule model may need more than one project calendar for activities with different work
periods. Resource calendars give the availability of people and equipment. Converting effort to
duration requires the calendar: holidays and vacations stretch duration without changing effort.

**Implication for alma-gantt.**
- Already in `SPEC.md` §6 (calendars, per-resource calendar, task calendar override). Add shifts
  (several working intervals per day) and per-resource exceptions (vacations).
- `alma explain` must name the calendar exception that moved a date ("shifted 3d by 'Fiestas Patrias'
  on calendar cl-standard").

**Source.** PMBOK 6, §6.5.3.4 (PMBOK6-0468); PMBOK 6, §6.6.1.2 (PMBOK6-0477); FEP02, p. 71 del PDF
(FEP02-0071).

---

## Part 3. Duration estimating

### 3.1 Effort is not duration

**What it is.** Effort is the amount of work (person-hours or person-days). It is estimated per work
package and drives cost, and it does not change when people are added. Duration is calendar time. It
is what the contract commits to, and it depends on how many people work and at what dedication. The
conversion divides effort by people × daily hours × dedication and then applies the calendar.
Duration does not fall linearly as resources grow. Diminishing returns, onboarding (knowledge transfer
and learning curve) and extra coordination channels mean that past some point more people lengthen
the work. Some work takes fixed time whatever the staffing (a 24-hour stress test). PMBOK names three
duration types: fixed duration, fixed effort (work) and fixed number of resources.

**Implication for alma-gantt.**
- Task scheduling type: `sched_type = "fixed_duration" | "fixed_effort" | "fixed_units"` (MS Project
  semantics). The engine recomputes the free variable when another changes:
  `duration = effort / (Σ units × hours_per_day)` on the task calendar (Annex B, F-SCH-10).
- `SPEC.md` §6 says "duration | effort". Make both storable together with the type flag, plus a
  consistency check: `L-EST-03` (`warn`) when the stored duration, effort and units disagree beyond
  rounding.
- `alma assign` reports the resulting duration change ("+units 2→3: duration 15d→10d,
  fixed_effort"). Optional `diminishing_returns` coefficient is out of scope; document that the linear
  conversion is an approximation the sources warn against.

**Source.** FEP02, p. 71 del PDF (FEP02-0071); FEP02, p. 72 del PDF (FEP02-0072); PMBOK 6, §6.4
(PMBOK6-0417, PMBOK6-0418, PMBOK6-0419).

### 3.2 Analogous estimating

**What it is.** A top-down estimate based on the real duration (or size, cost, complexity) of a
similar past project or activity, adjusted for known differences. It is cheap and fast but the least
accurate. It is used early, when detail is scarce, and it is only reliable when the reference is truly
similar.

**Implication for alma-gantt.** `estimate = { method = "analogous", reference = "<project/task>",
value = "20d" }`. Used on planning packages and summaries before decomposition (1.12). The bottom-up
total is shown next to it (`alma show --estimates`).

**Source.** PMBOK 6, §6.4.2.2 (PMBOK6-0428); FEP02, p. 74 del PDF (FEP02-0074).

### 3.3 Parametric estimating

**What it is.** An algorithm on historical data: quantity of work × hours per unit, or quantity ÷ rate
(for example 1,000 m of cable at 25 m/h gives 40 h). It can be quite accurate if the model and data are
good.

**Implication for alma-gantt.** `estimate = { method = "parametric", quantity = 1000, unit = "m",
rate = "25 m/h" }`. The engine derives effort (or duration), and changing the quantity re-estimates.
Units of measure come from the schedule management plan (4.13).

**Source.** PMBOK 6, §6.4.2.3 (PMBOK6-0429); FEP02, p. 74 del PDF (FEP02-0074).

### 3.4 Three-point estimating and PERT

**What it is.** Instead of one number, give optimistic (tO), most likely (tM) and pessimistic (tP)
values. The expected value depends on the assumed distribution:
- Triangular: tE = (tO + tM + tP) / 3. Used when history is thin or the data is judgment.
- Beta / PERT: tE = (tO + 4 tM + tP) / 6, with σ = (tP − tO) / 6. Gives four times the weight to the
  most likely value.

PMBOK 6 shows only the triangular formula for duration (§6.4.2.4) and both formulas for cost (§7.2.2.5).
The professor shows both for duration plus σ. His example: 4/7/16 days gives tE = 8 d and σ = 2 d.
Committing to tE means roughly a 50 % chance of being late. ±1σ, ±2σ and ±3σ cover about 68 %, 95 %
and 99.7 %, and for a bid with late-delivery penalties he recommends committing at about 2σ and
covering the rest with reserve.

**Implication for alma-gantt.**
- `estimate = { method = "three_point", o = "4d", m = "7d", p = "16d", dist = "beta" | "triangular" }`.
  `duration` is then *derived* (tE by default; or a percentile with `commit = "p95"` under a normal
  approximation). Store only o/m/p, never the derived tE (computed values stay out of the file).
- `L-EST-02` (`error`): not o ≤ m ≤ p, or o < 0.
- `alma estimate <id>` prints tE, σ and the 1/2/3σ intervals (F-SCH-03 to F-SCH-05).
- Path-level PERT: report expected project duration and an approximate probability of meeting a date
  *on the critical path only*, with a clear warning that it ignores merge bias. The real answer is
  Monte Carlo (4.11). The sources do not give path aggregation. Summing variances along a path is the
  classical PERT assumption, outside these documents; label it as such in the output.
- Capture three-point data from M0, even though Monte Carlo is "Later" in `SPEC.md`, so plans built
  now can be simulated later.

**Source.** PMBOK 6, §6.4.2.4 (PMBOK6-0430); PMBOK 6, §7.2.2.5 (PMBOK6-0523); FEP02, p. 75 del PDF
(FEP02-0075); FEP02, p. 76 del PDF (FEP02-0076).

### 3.5 Bottom-up estimating and basis of estimates

**What it is.** When an activity cannot be estimated with confidence, break it into finer pieces,
estimate those and add them up. It is the most accurate technique and the most expensive to produce.
Techniques combine: analogy early, replaced by bottom-up once the WBS is complete. Every estimate should
document its basis: how it was made, assumptions, known constraints, range (for example ±10 %),
confidence level and the individual risks that affect it. Estimates can be given as a range (for example
2 weeks ± 2 days) or as a probability (15 % chance of exceeding 3 weeks).

**Implication for alma-gantt.**
- `basis` (free text) and `confidence` (for example `"p80"` or `"±10%"`) on any estimate.
- `L-EST-01` (`info`, configurable to `warn` for work packages): an estimate without `basis`.
- `alma export --basis-of-estimates (md|xlsx)`: one line per work package with method, inputs, result,
  range and basis. This is the traceability table a proposal needs ("every number traceable",
  `SPEC.md` §3).

**Source.** PMBOK 6, §6.4.2.5 (PMBOK6-0431); PMBOK 6, §6.4.3.1 (PMBOK6-0437); PMBOK 6, §6.4.3.2
(PMBOK6-0438); PMBOK 6, §6.4 (PMBOK6-0420); FEP02, p. 74 del PDF (FEP02-0074).

### 3.6 Reserve analysis: contingency vs management reserve

**What it is.** *Contingency reserve* (schedule reserve) covers identified, accepted risks
(known-unknowns). It lives *inside* the schedule baseline, can be a percentage or a fixed number of
periods, can be pulled out of single activities and aggregated, and should be clearly shown in the
schedule. *Management reserve* covers unforeseen in-scope work (unknown-unknowns). It sits *outside* the
baseline but inside the total duration requirement, and using it may require rebaselining. Using it
needs sponsor or management approval. The professor adds two points. Using contingency is normal and
does not change the baseline, while using management reserve does. And a reserve that is not declared
in the schedule does not exist: the delay will be read as non-compliance. Because of Parkinson's law,
student syndrome and procrastination, padding hidden inside each activity gets consumed inside that
activity, so it is better to concentrate reserve at project level. (This is the idea behind
critical-chain buffers. PMBOK 6 and FEP02 do not develop critical chain further.)

**Implication for alma-gantt.**
- `kind = "reserve"` task with `reserve = "contingency" | "management"`, usually placed before a
  mandatory milestone (a feeding or project buffer). Rules:
  - A contingency reserve is part of the baseline. A management reserve is excluded from baseline
    dates but included in a "commitment date" computation (`alma critical --with-management-reserve`).
  - Reserves carry no effort or cost (unless a `cost` is set explicitly) and are drawn differently in
    the Gantt (hatched).
  - `alma reserve consume <id> --by 3d --reason ...` records consumption in the op log. `alma status`
    shows reserve remaining against the critical path delay.
- `L-EST-04` (`info`): a plan with mandatory milestones or deadline constraints and no contingency
  reserve on the paths that drive them.
- `L-EST-05` (`info`, off by default): many three-point activities committed at a high percentile
  (individual padding) while no project-level reserve exists. The hint suggests committing at tE and
  aggregating the margin.

**Source.** PMBOK 6, §6.4.2.6 (PMBOK6-0433, PMBOK6-0434); FEP02, p. 77 del PDF (FEP02-0077); FEP02,
p. 73 del PDF (FEP02-0073); PMBOK 6, §6.4 (PMBOK6-0419); PMBOK 6, §6.5.2.1 (PMBOK6-0449).

---

## Part 4. Developing and controlling the schedule

### 4.1 Critical path method: forward and backward pass

**What it is.** CPM computes early start/finish and late start/finish for every activity, *ignoring
resource limits*, by a forward pass from the start of the network and a backward pass from its end.
Forward pass: an activity's early start is the latest of its predecessors' early finishes (for FS
links), and early finish = early start + duration. Backward pass: an activity's late finish is the
earliest of its successors' late starts, and late start = late finish − duration. The forward pass
gives the project duration; the backward pass gives each activity's flexibility. The resulting dates
are windows, not the schedule.

**Implication for alma-gantt.**
- Engine as in `SPEC.md` §7.2, generalized to all link types and lags (Annex B, F-SCH-06/F-SCH-07).
- **Anchor of the backward pass** (missing in `SPEC.md`): by default the project's computed early
  finish. Optionally a project `deadline` or a finish constraint on the final milestone. If the anchor is
  later than the early finish, critical activities get *positive* float. If it is earlier, they get
  *negative* float (4.3). Make it a setting: `settings.late_anchor = "early_finish" | "deadline"`.
- Golden test: the professor's example network (A–H, durations 3/2/4/2/6/4/2/4, critical path
  A→C→F→H, project length 15 periods, G with 6 periods of float) as a hand-checked fixture. Note that
  its slide uses the "elapsed periods" convention (ES of the first activity = 0, EF = ES + d). The
  engine works on calendar dates, where a 1-day task starting Monday also finishes Monday. Document
  the conversion in the engine and test both conventions.

**Source.** PMBOK 6, §6.5.2.2 (PMBOK6-0450); FEP02, p. 78 del PDF (FEP02-0078); FEP02, p. 79 del
PDF (FEP02-0079); FEP02, p. 80 del PDF (FEP02-0080); FEP02, p. 81 del PDF (FEP02-0081).

### 4.2 Total float, free float and the critical path

**What it is.** *Total float*: how long an activity can be delayed or extended from its early start
without delaying the project finish *or violating a schedule constraint* (LS − ES = LF − EF). *Free
float*: how long it can be delayed without delaying the early start of any successor or violating a
constraint. The critical path is the longest path through the network. It sets the shortest possible
project duration and normally has zero total float, more precisely the *least* float. A one-day slip on
it slips the project by a day. A project can have several critical paths, and the critical path can
change during execution.

**Implication for alma-gantt.**
- `SPEC.md` §7.2 defines critical as "float ≤ 0, configurable". Better: critical = total float ≤
  `settings.critical_threshold` (default 0) **or** "on the longest path" (the least-float path, which
  is the right definition when a deadline makes all floats positive or negative). Offer both:
  `settings.critical_definition = "float_threshold" | "longest_path"`.
- Free float per link type (Annex B, F-SCH-09), shown in `alma show`.
- `alma critical` returns *all* critical paths as ordered chains (there can be several), and supports
  `--to <milestone>` (the driving path to any milestone, not only the project end).
- Property test addition to `SPEC.md` §12: 0 ≤ FF ≤ TF for every activity on unconstrained
  feasible plans.

**Source.** PMBOK 6, §6.5.2.2 (PMBOK6-0450, PMBOK6-0451, PMBOK6-0452); FEP02, p. 78 del PDF
(FEP02-0078); FEP02, p. 79 del PDF (FEP02-0079).

### 4.3 Negative float and near-critical paths

**What it is.** With PDM and constraints, critical paths can have positive, zero or *negative* total
float. Positive float appears when the backward pass starts from a constraint later than the early
finish. Negative float appears when duration and logic violate a constraint on late dates. *Negative
float analysis* looks for ways to bring a late schedule back. The fixes are shorter durations (more
resources or less scope), changing discretionary logic, adjusting leads/lags or changing constraints.
Networks may also have several *near-critical* paths, and many tools let the user set the parameters
that define criticality. During control, progress on near-critical paths reveals schedule risk.

**Implication for alma-gantt.**
- Negative float is a *result*, not a crash. The engine computes it and `alma check` reports
  `L-SCH-05` (`error` by default; `warn` while `settings.phase = "draft"`): an activity with negative
  total float. The payload lists the violated constraint, the driving path and the size of the
  overrun. Fix hints point to `alma compress` (4.9) and list the discretionary links on that path.
- `alma critical --near <threshold>` (for example `--near 5d`) lists near-critical paths.
  `L-SCH-07` (`info`): the number of near-critical paths above `rules.max_near_critical`.
- UI: near-critical bars get a secondary emphasis (`SPEC.md` §11 "few colors with meaning").

**Source.** PMBOK 6, §6.5.2.2 (PMBOK6-0451); PMBOK 6, §6.5.2.6 (PMBOK6-0458); PMBOK 6, §6.6.2.2
(PMBOK6-0483).

### 4.4 Excessive float as a symptom

**What it is.** PMBOK 6 warns that undocumented discretionary dependencies can create arbitrary total
float values. In practice, very large float usually means missing logic (open ends) rather than real
flexibility.

**Implication for alma-gantt.** `L-SCH-08` (`info`): an activity whose total float exceeds
`rules.max_float_ratio` (default 0.5) × project duration. The hint links to `L-DEP-02` if the
activity has no successor.

**Source.** PMBOK 6, §6.3.2.2 (PMBOK6-0410); PMBOK 6, §6.3 (PMBOK6-0402).

### 4.5 Schedule constraints and imposed dates

**What it is.** Activity attributes include imposed dates and constraints. Mandatory dependencies are
logic and must not be modelled as constraints. Constraints and imposed dates are what schedule
compression tries to meet. Imposed date constraints are also the means to reschedule work so that
spending stays within funding limits.

**Implication for alma-gantt.**
- Keep constraint types (`ASAP`, `ALAP`, `SNET`, `SNLT`, `FNET`, `FNLT`, `MSO`, `MFO`) and add a soft
  `deadline` (it never moves the task; it only affects late dates and float and produces a diagnostic
  when missed). Deadlines are the right way to model contractual milestone dates without overriding
  logic.
- `L-SCH-06` (`warn`): a hard constraint (`MSO`, `MFO`, `SNLT`, `FNLT`) on an activity that also has
  predecessors. A constraint that overrides logic hides real delays. Prefer `deadline`.
- `L-SCH-11` (`info`): an `SNET` used as the *only* thing that positions an activity with no
  predecessors (a constraint used instead of logic; see 2.8).
- Funding: `alma level --funding-limit <curve>` (M2+) can place SNET constraints to keep cumulative
  cost under a cash-flow ceiling. It should be audited like resource leveling.

**Source.** PMBOK 6, §6.2.3.2 (PMBOK6-0398); PMBOK 6, §6.3.2.2 (PMBOK6-0409); PMBOK 6, §6.5.2.6
(PMBOK6-0458); PMBOK 6, §7.3.2.5 (PMBOK6-0542).

### 4.6 Schedule network analysis as an iterative loop

**What it is.** Developing an acceptable schedule is iterative. Set milestones, define and sequence
activities, estimate, compute, then review durations, resources and reserves. The people assigned
check their dates against their calendars and other commitments. Then logic conflicts are resolved and
leveling is done if needed, before approval and baselining. The analysis also reviews whether the
critical path carries high-risk activities or long-lead items that need reserve or risk responses.

**Implication for alma-gantt.**
- This is exactly the loop an agent runs: mutate → reschedule → `check` → `explain` → `render`. Put it
  in `alma guide` as the recommended loop, with a "ready to baseline" gate:
  `alma check --gate baseline` runs a stricter profile (no open ends, no negative float, every
  mandatory milestone has a criterion, leveling done, reserves declared).
- `L-SCH-12` (`info`): an activity tagged `risk:high` or `long_lead` on the critical path with no
  reserve downstream of it.

**Source.** PMBOK 6, §6.5 (PMBOK6-0441); PMBOK 6, §6.5.2.1 (PMBOK6-0449).

### 4.7 Resource leveling

**What it is.** Leveling moves start and finish dates to balance resource demand with supply when
resources are limited, available only at certain times, or over-allocated (one resource on two
activities at once). It can also aim at constant usage. Leveling uses available float and can change the
critical path, and it often lengthens the project.

**Implication for alma-gantt.**
- Matches `SPEC.md` §7.4 (serial SGS, priority rule, audit trail). Add:
  - After leveling, *recompute* the critical path on the leveled schedule (resource-driven links
    become drivers). `alma explain` names the competing tasks as the driver (already planned).
  - `alma level` output includes `critical_path_changed: bool` and `end_date_delta`.
  - Both leveled and unleveled schedules are kept as alternative schedule data (4.10).
- `L-RES-01` (`warn`): over-allocation remaining after leveling (already listed in `SPEC.md` §4).

**Source.** PMBOK 6, §6.5.2.3 (PMBOK6-0453); FEP02, p. 82 del PDF (FEP02-0082); PMBOK 6, §6.5
(PMBOK6-0441).

### 4.8 Resource smoothing

**What it is.** Smoothing keeps resource demand under predefined limits *without* changing the
critical path or delaying completion. Activities move only within their free and total float. Unlike
leveling, it may not be able to resolve every over-allocation.

**Implication for alma-gantt.**
- `SPEC.md` §7.4 "within float only" is smoothing. Name it as such:
  `alma level --mode smooth` (within float, end date fixed) vs `alma level --mode level` (end date may
  slip). Default `--mode smooth` is the safer choice for an agent working blind.
- Smoothing reports the over-allocations it could *not* resolve as diagnostics (`L-RES-01`), never
  silently.
- Option `--smooth-float free|total`: free float only (no successor moves) or total float (successors
  may move, end date fixed).

**Source.** PMBOK 6, §6.5.2.3 (PMBOK6-0454); FEP02, p. 82 del PDF (FEP02-0082).

### 4.9 Schedule compression: crashing and fast tracking

**What it is.** Both shorten the schedule without cutting scope, to meet constraints or imposed dates.
- **Crashing**: add resources (overtime, extra people, paid expediting) to critical activities, aiming
  at the least added cost. It only works where more resources really shorten the activity, and it may
  raise risk and cost.
- **Fast tracking**: overlap activities or phases that would normally run in sequence. It only helps
  on the critical path, it raises rework and risk, it may raise cost, and leads used for acceleration
  increase coordination and quality risk.
Negative float analysis is the starting point.

**Implication for alma-gantt.**
- `alma compress --target <date|milestone> --strategy crash|fast-track --dry-run`: read-only
  proposals, ranked.
  - Crash: needs per-activity `crash = { min_duration, cost_per_day }`. Greedy cheapest-first on the
    current critical path(s), recomputing after each step (paths can switch).
  - Fast-track: proposes turning FS links with `nature = "discretionary"` on the critical path into SS
    + lag, or a smaller lag. It never touches mandatory links.
  - Output is a list of `alma` commands an agent can `apply` after human review, with the resulting
    end date, added cost and a risk note.
- Mark fast-tracked links (`overlap = true`) so the risk is visible in reports.

**Source.** PMBOK 6, §6.5.2.6 (PMBOK6-0458, PMBOK6-0459); PMBOK 6, §6.4.2.6 (PMBOK6-0432);
FEP02, p. 82 del PDF (FEP02-0082); FEP02, p. 68 del PDF (FEP02-0068).

### 4.10 What-if scenarios and alternative schedules

**What it is.** What-if analysis runs the schedule network under different scenarios (a late key
delivery, a longer engineering phase, a strike, a permit delay) to test feasibility and size reserves
and responses. PMBOK lists alternative schedules as part of schedule data: best and worst case, leveled
and unleveled, with and without imposed dates. It also lists applied reserves, resource histograms and
cash-flow projections. In control, what-if analysis helps bring the model back in line with the plan and
baseline.

**Implication for alma-gantt.**
- `alma whatif --set gate-build.duration=70d --set dep:a->b.lag=5d --json`: computes in memory and
  returns the deltas (end date, critical path changes, milestones moved, float consumed). Nothing is
  written. This is different from `--dry-run` on a mutation because it accepts several overrides at
  once.
- Named scenarios stored in the file: `[[scenario]] id = "worst", overrides = [...]`. They are inputs,
  not computed values, so they can live in the source. `alma diff --scenario worst`.
- Exports can show "baseline vs current vs scenario" bars.

**Source.** PMBOK 6, §6.5.2.4 (PMBOK6-0455); PMBOK 6, §6.5.3.3 (PMBOK6-0467); PMBOK 6, §6.6.2.1
(PMBOK6-0482).

### 4.11 Simulation (Monte Carlo)

**What it is.** Simulation models the combined effect of individual risks and other uncertainty on the
objectives. Monte Carlo recomputes the schedule many times with durations drawn from probability
distributions, and gives the probability of finishing by each date (for example 10 % by one date and
90 % by a later one).

**Implication for alma-gantt.**
- Later milestone in `SPEC.md` §5. Requirements to plan for now:
  - Inputs: three-point estimates (3.4), plus optional discrete risk events (`risk = { p = 0.2,
    impact = "+10d", applies_to = [...] }`).
  - Deterministic RNG with a seed in the command (`--seed`), so results are reproducible
    (`SPEC.md` pillar 2).
  - Outputs: finish-date distribution (P10/P50/P80/P90), criticality index per activity (share of runs
    where it was critical), and a sensitivity ranking.
  - Export: an S-shaped cumulative probability chart, the same chart style as the cost S-curve.

**Source.** PMBOK 6, §6.5.2.4 (PMBOK6-0456).

### 4.12 Presentation formats: bar chart, milestone chart, network diagram; summary vs detailed

**What it is.** The schedule can be tabular but is usually graphical:
- **Bar (Gantt) chart:** activities on the vertical axis, dates on the horizontal, bars from start to
  finish. Easy to read. Float may or may not be shown depending on the audience. For management
  reports, summary bars between milestones or across related work packages are shown in a
  WBS-structured layout.
- **Milestone chart:** like a bar chart but only the start or finish of major deliverables and key
  external interfaces. The professor calls it the format for management and the client.
- **Network diagram:** activity-on-node with no time scale (pure logic), or time-scaled (a logic bar
  chart / time-scaled logic diagram, which can put several activities in sequence on one line).
PMBOK shows three linked levels of the same project: milestone schedule, summary schedule, and
detailed linked bar chart, all reported at a status date. A summary form may be called the master or
milestone schedule. The professor notes the weaknesses of a plain Gantt: it hides relationships and is
hard to read with many tasks.

**Implication for alma-gantt.**
- Export presets: `--view milestone` (only `kind = milestone`, filter by `visibility`/`mandatory`),
  `--view summary --depth N` (roll up to WBS level N), `--view detailed` (all activities with
  dependency arrows), `--view network` (AON, pure logic or time-scaled).
- Dependency arrows *on* by default in the detailed Gantt (this answers the professor's "a Gantt does
  not show relationships"). Allow `--no-arrows` for printing summaries.
- `--show-float` flag (float as a thin trailing bar), off by default for client-facing presets.
- Status-date line and a "data date" stamp on every export when progress exists.
- `L-PRS-01` (`info`): exporting a detailed Gantt with more than `rules.max_rows_per_page` rows per
  page at the chosen font size. This ties into print preflight, `SPEC.md` §4 item 6.

**Source.** PMBOK 6, §6.5.3.2 (PMBOK6-0463, PMBOK6-0464, PMBOK6-0465, PMBOK6-0466); FEP02,
p. 82 del PDF (FEP02-0082).

### 4.13 Schedule management plan settings

**What it is.** The schedule management plan fixes the rules for building and controlling the schedule:
scheduling method and tool; release and iteration lengths for adaptive work; level of accuracy (an
acceptable range, possibly with contingency); units of measure (hours, days or weeks, working or
calendar time, quantities per resource); the link to the WBS; how status and progress are recorded;
control thresholds (allowed variance, usually a percentage of the baseline, before action is needed);
rules of performance measurement (how % complete is set, which EVM technique, SV/SPI); and report
formats and frequency. The professor stresses the control threshold, which separates a tolerable
deviation from one that must be reported to the client: define it before you need it. He also lists
progress rules such as milestone-based, fixed percentage at start and finish, or units completed.

**Implication for alma-gantt.**
- A `[settings.schedule]` block in the file, so these rules are versioned with the plan:
  `time_unit`, `working_or_elapsed_default`, `accuracy` (for example `"-10%/+15%"`),
  `reporting_period`, `rolling_wave_horizon`, `control_threshold = { sv_pct = 10, finish_slip = "5d" }`,
  `progress_rule_default = "physical" | "0/100" | "50/50" | "20/80" | "milestone_weights" | "units"`,
  `critical_definition`, `critical_threshold`.
- Per work package override `progress_rule`, used by EV computation (M4).
- `alma guide` prints the active settings so the agent reasons within them.

**Source.** PMBOK 6, §6.1.3.1 (PMBOK6-0386, PMBOK6-0387, PMBOK6-0388); FEP02, p. 64 del PDF
(FEP02-0064).

### 4.14 Schedule baseline

**What it is.** The approved version of the schedule model, with baseline start and finish dates,
changed only through formal change control. Actual dates are compared with it to find variances. The
professor's warning: progress reported without a baseline is not control, only an opinion about the
project's state.

**Implication for alma-gantt.**
- `SPEC.md` has named baselines. Add `approved_by`, `approved_on`, `change_ref` (1.13) and a
  designated `current` baseline used by default in variance reports.
- `L-BAS-01` (`warn`): progress recorded (any actual start, % complete or remaining duration) but no
  baseline exists.
- Baseline captures: dates, durations, work, cost per task, WBS codes and scope (1.13), and reserves.

**Source.** PMBOK 6, §6.5.3.1 (PMBOK6-0462); FEP02, p. 83 del PDF (FEP02-0083).

### 4.15 Schedule data and project schedule outputs

**What it is.** Schedule data is the information that describes and controls the schedule. At minimum:
milestones, activities, activity attributes, and documented assumptions and constraints. Often also:
resource requirements per period (histogram), alternative schedules, applied reserves, cash-flow
projections, and order and delivery schedules. The project schedule shows at least a planned start and
finish for each activity. It stays preliminary until resource assignments are confirmed. A *target*
schedule with target dates may also be kept.

**Implication for alma-gantt.**
- `alma export --schedule-data (xlsx|json)`: a single bundle with milestones, activities, attributes,
  assumptions, histogram, alternative schedules and reserves.
- Plan state flag `settings.phase = "draft" | "preliminary" | "baselined"`. Lint severities can depend
  on it (for example negative float is `warn` in draft, `error` once baselined).
- `target` dates can be modelled as a named baseline (`alma baseline save target`).

**Source.** PMBOK 6, §6.5.3.3 (PMBOK6-0467); PMBOK 6, §6.5.3.2 (PMBOK6-0463).

### 4.16 Control schedule: status, variance, trends, forecasts

**What it is.** Controlling the schedule means updating it with actual performance and managing changes
to the baseline through integrated change control. Work performance data are: which activities
started, their progress (actual duration, remaining duration, physical % complete), and which finished.
The techniques are:
- performance reviews (actual vs baseline dates, % complete, remaining duration);
- trend analysis (is performance getting better or worse?);
- variance analysis (start, finish, duration and *float* variances; cause and degree; and whether
  action is needed);
- CPM on progress (critical path variance directly hits the end date, while a long delay off the
  critical path may matter little);
- earned value (SV, SPI);
- what-if;
- adjusting leads and lags to bring late work back in line;
- resource optimization;
- forecasts of the finish date from performance to date.
Contractor and supplier status updates are part of control. The professor's summary: everything is
measured against the approved baseline.

**Implication for alma-gantt.**
- Progress fields per activity: `actual_start`, `actual_finish`, `remaining_duration`,
  `physical_pct` (separate from duration %), `status_date` at project level (already listed in
  `SPEC.md` §6/M4).
- Engine with progress: completed work is fixed at its actuals. Remaining work is scheduled from the
  status date (no unfinished work before the data date). Out-of-sequence progress follows a setting:
  `settings.progress_logic = "retained_logic" | "progress_override"`.
- `alma status` (read-only): variances against the current baseline: start, finish, duration and float
  deltas, plus a list of items breaching the control thresholds.
  `L-BAS-03` (`warn`): variance above `control_threshold`.
- `L-BAS-02` (`error`): actual dates or progress present but no `status_date`.
  `L-BAS-04` (`error`): actual start/finish later than the status date, or progress on an activity
  whose predecessors under mandatory FS logic have not started. (Derived consistency rules; not
  stated literally in the sources.)
- `alma forecast` (M4): projected finish from performance to date (SPI-based and CPM-based side by
  side).

**Source.** PMBOK 6, §6.6 (PMBOK6-0474, PMBOK6-0475); PMBOK 6, §6.6.1.3 (PMBOK6-0478); PMBOK 6,
§6.6.2.1 (PMBOK6-0480, PMBOK6-0481, PMBOK6-0482); PMBOK 6, §6.6.2.2 (PMBOK6-0483); PMBOK 6,
§6.6.2.5 (PMBOK6-0486); FEP02, p. 83 del PDF (FEP02-0083); PMBOK 6, p. 707 del PDF, glossary
(PMBOK6-1585).

### 4.17 Risk responses and payment milestones must appear in the schedule (professor's emphasis)

**What it is.** Two cross-checks from other decks of the same course apply directly to Gantt quality.
(1) A planned risk response that is not an activity in the WBS and schedule, with a duration, a named
owner and a budget, will not get done. The test is to look for each response in the schedule.
(2) An evaluator follows a chain from requirement to metric to test to *milestone in the Gantt/WBS* to
payment clause. If that chain breaks at the milestone, the proposal is not tied together.

**Implication for alma-gantt.**
- `risk_ref` on activities, and `payment_ref` / `requirement_refs[]` on milestones.
- `L-SCH-13` (`info`, active when the plan declares risks or payment items): a declared risk response
  with no activity referencing it, or a payment item with no milestone referencing it.
- `alma trace <requirement|risk|payment>` lists the linked WBS elements and milestones, with dates.

**Source.** FEP04, p. 67 del PDF (FEP04-0067); FEP06, p. 101 del PDF (FEP06-0101).

---

## Part 5. Agile and hybrid scheduling

### 5.1 Life-cycle continuum and hybrid approaches

**What it is.** Life cycles range along a continuum. *Predictive* cycles exploit known, stable
requirements and run work in sequence. *Iterative* cycles refine partial work through feedback.
*Incremental* cycles deliver usable pieces early. *Agile* cycles combine iterative and incremental.
All projects plan; agile teams replan as they learn. A *hybrid* approach combines any of these in one
project. The APG gives several patterns:
- agile development followed by a predictive rollout (possibly done by another team);
- agile and predictive practices used at the same time (for example short iterations and stand-ups,
  but upfront estimation and predictive progress tracking);
- a mostly predictive project with a small agile part where uncertainty is high;
- a mostly agile project with a predictive component (for example a single integration of a vendor's
  component);
- hybrids designed around risk, for example incremental delivery by building with a predictive plan
  for each well-known building.
PMBOK adds that large organizations may combine predictive, adaptive and hybrid techniques. The
professor: a fixed-price tender sets a predictive frame (scope frozen in a baseline, changes by request),
and combining approaches in one project is a hybrid life cycle.

**Implication for alma-gantt.**
- `approach = "predictive" | "iterative" | "flow"` on any WBS branch (inherited by descendants;
  project default `predictive`). It changes how the engine and lint treat that branch:
  - `predictive`: full CPM, all logic rules.
  - `iterative`: the branch is scheduled as a container whose span is the set of iterations assigned to
    it (5.3). Items inside do not need FS logic between them, so `L-DEP-01/02` are suppressed inside
    and applied to the container instead.
  - `flow`: a capacity-based container (5.7).
- The predictive skeleton (contract milestones, procurement, rollout, go-live) keeps hard logic *into
  and out of* agile containers. For example "Release 1 accepted" (milestone) → "Training rollout"
  (predictive). This is how predictive and agile coexist in one critical path.
- `alma guide` presents the APG patterns as templates: `alma init --template hybrid-agile-then-rollout`,
  `hybrid-predictive-with-agile-component`, and so on.

**Source.** Agile Practice Guide, §3.1 (APG-0032, APG-0033, APG-0034); Agile Practice Guide, §3.1.1
(APG-0036, APG-0037, APG-0038); Agile Practice Guide, §3.1.6 (APG-0048, APG-0049); Agile Practice
Guide, p. 42 del PDF (APG-0050); Agile Practice Guide, §3.1.8 (APG-0051, APG-0052); Agile Practice
Guide, §3.1.10 (APG-0054); PMBOK 6, p. 196 del PDF (PMBOK6-0376); FEP02, p. 8 del PDF (FEP02-0008).

### 5.2 Iteration-based vs flow-based agile

**What it is.** In *iteration-based* agile, the team works in timeboxes of equal length and finishes
the most important features first, without doing all requirements, then all analysis, and so on. In
*flow-based* agile, the team pulls features from the backlog when it has capacity, manages work in
progress per board column, and features take different times. Planning, reviews and retrospectives
then follow a cadence agreed with stakeholders instead of iteration boundaries. Teams can mix the two:
flow with a cadence.

**Implication for alma-gantt.** The `approach` values `iterative` and `flow` above. A flow container
may still have a `cadence` (review/replan dates) drawn as thin ticks on the timeline.

**Source.** Agile Practice Guide, §3.1.4 (APG-0044, APG-0045); Agile Practice Guide, §3.3
(APG-0061).

### 5.3 Iterations, timeboxes and cadence

**What it is.** A *timebox* is a fixed period (1 week, 2 weeks, 3 weeks, 1 month). An *iteration* is a
timeboxed development cycle in which all the work needed to deliver value is done. A *sprint* is the
Scrum name for it. *Cadence* is the rhythm of execution. Iterations give the team a cadence for
delivery, demos and feedback. PMBOK puts iteration and release lengths in the schedule management plan
and notes iterations of typically 2 weeks or 1 month. Timeboxing limits scope creep because essential
features go first. The APG suggests showing working product at least every two weeks.

**Implication for alma-gantt.**
- Make iterations a cadence object, not a flat list:
  `[[cadence]] id = "team-a", length = "2w", start = 2027-03-01, count = 12, calendar = "...", team = [...]`.
  Iterations are generated (s01..s12), with optional per-iteration overrides (name, goal, a holiday
  shortening). `SPEC.md` §8a already has `alma iter add --length --repeat`; persist the cadence rather
  than twelve expanded records, so a change of start date is a one-line diff.
- Fields per iteration: `goal`, `capacity` (points or hours), `committed`, `completed` (M4).
- Lint:
  - `L-AGL-01` (`error`): overlapping iterations within the same cadence.
  - `L-AGL-02` (`warn`): iterations of unequal working length within one cadence (the APG defines
    iterations as equal-duration timeboxes). Explicit overrides are allowed with a `reason`.
  - `L-AGL-03` (`info`): a gap between consecutive iterations of one cadence.
  - `L-AGL-06` (`info`): an iteration longer than one month or shorter than one week.
  - `L-AGL-04` (`warn`): an item assigned to an iteration but scheduled outside its window.
- Timeline: sprint bands (already in `SPEC.md` §4 item 4), one band row per cadence.

**Source.** Agile Practice Guide, §2 (APG-0293, APG-0281, APG-0286); Agile Practice Guide, §5.2.8
(APG-0117); Agile Practice Guide, §5.2.5 (APG-0112); Agile Practice Guide, §3.3 (APG-0061); PMBOK 6,
§6.1.3.1 (PMBOK6-0387); PMBOK 6, §6.6 (PMBOK6-0475).

### 5.4 Release planning and the roadmap

**What it is.** Agile release planning gives a high-level timeline for a release, typically 3 to 6
months, derived from the product vision and roadmap. It fixes how many iterations the release has and
lets the product owner and team decide what can be built and when a releasable product is ready,
given goals, dependencies and impediments. Because features are what the customer values, a timeline
that says which feature lands at the end of which iteration is easy to understand. The product owner
may keep a roadmap of the expected sequence of deliverables and replan it as the team delivers. The
chain is vision → roadmap → release plan → iteration plan.

**Implication for alma-gantt.**
- A `release` is distinct from an iteration: `[[release]] id = "r1", iterations = "team-a:s01..s06",
  milestone = "r1-accepted"`. The release end is a *milestone* in the predictive network, so contract
  logic can depend on it.
- `L-AGL-05` (`info`): a release milestone that does not coincide with the end of its last iteration,
  or a release with no iterations.
- Roadmap view: features (or epics) as bars spanning the iterations planned to deliver them.
  `alma export --view roadmap`.

**Source.** PMBOK 6, §6.5.2.8 (PMBOK6-0461); Agile Practice Guide, §5.2.2 (APG-0104).

### 5.5 Backlog, epics and stories in the WBS

**What it is.** The backlog is the ordered list of all the team's work, in story form. Only enough
stories for a broad view of the first release and the next iteration need to exist. In agile, the
requirements *are* the backlog, and scope is defined and redefined throughout. The WBS can follow this:
epics decompose into user stories. Refinement (progressive elaboration of the backlog) is a short
regular activity, often about an hour midway through a two-week iteration.

**Implication for alma-gantt.**
- Inside an `iterative` branch, children are backlog items: `kind = "story"` (or `"epic"` for an
  intermediate level), with `points`, `priority` (order), optional `iteration`. Stories carry no
  duration or dependencies by default. Their dates come from the iteration they are assigned to.
- Unassigned stories are backlog. They do not appear as bars, only in a backlog list view and in the
  burnup totals.
- This is an import target (CSV / Jira-style export), not a replacement for a backlog tool. Keep it
  lightweight, consistent with the non-goals in `SPEC.md` §4.

**Source.** Agile Practice Guide, §5.2.2 (APG-0104, APG-0105); Agile Practice Guide, §6.8
(APG-0175, APG-0176); PMBOK 6, §5.4.2.2 (PMBOK6-0339); PMBOK 6, p. 194 del PDF (PMBOK6-0373).

### 5.6 Velocity, capacity and forecasting

**What it is.** *Velocity* is the sum of story points of the features actually finished in an
iteration. It is used to plan the next iteration's capacity. Teams may need four to eight iterations
to reach a stable velocity, and story points are relative and specific to each team: velocities cannot
be compared or added across teams. Once velocity (or average cycle time) is reliable, the remaining
duration can be forecast. For example 500 points left at 50 points per iteration gives about 10
iterations, and 30 stories at 3 days each gives 90 working days. The variability should be shown
(for example with fan or "hurricane" charts). Measuring points without finished stories measures
capacity, not progress. Reduced capacity (holidays, absences) means planning less.

**Implication for alma-gantt.**
- Per cadence: `velocity_history[]` (actuals, M4) or a planning assumption `planned_velocity`.
- `alma agile forecast --cadence team-a` computes remaining iterations = ceil(remaining points /
  velocity) with a range from min/max (or P20/P80) of recent velocities. It returns a forecast finish
  date for the container and its release milestone.
- Engine option: an `iterative` container's *forecast* finish can drive the predictive network
  (`settings.agile_drives = "planned_iterations" | "velocity_forecast"`). Default is planned iterations,
  so the predictive plan is not destabilized by noise.
- Capacity per iteration is derived from the team's resource calendars (holidays reduce capacity).
- `L-AGL-07` (`warn`): committed points in an iteration exceed its capacity.
- Never sum velocity across cadences (enforced in the API: no cross-cadence velocity field).

**Source.** Agile Practice Guide, §5.4.1 (APG-0126, APG-0127, APG-0128, APG-0131, APG-0133,
APG-0134); Agile Practice Guide, §5.2.5 (APG-0113); PMBOK 6, §6.6 (PMBOK6-0475).

### 5.7 Burndown and burnup charts

**What it is.** A *burndown* plots work remaining against time left in a timebox, with an ideal
diagonal line. Daily actuals and a trend line forecast the end-of-iteration variance. A *burnup* plots
work completed toward a release, and when it also plots total scope it shows scope growth. Both use the
same data. Feature burnup/burndown charts and product backlog burnup charts show completion trends
across iterations. Earned value in agile is based on *finished* features. A burnup with story points on
one axis and spending on the other can stand in for EVM.

**Implication for alma-gantt.**
- Exports (M4): `alma export --chart burndown --iteration s03`, `--chart burnup --release r1`
  (completed line + total scope line), `--chart backlog-burnup`. Same print-grade pipeline as the
  S-curve.
- Agile EV: for `iterative` branches, EV for a period = budget of the container × (points of *accepted*
  stories ÷ planned points). Optional spend axis on the burnup.
- `alma status` for an iteration includes "ideal vs actual remaining" and the trend forecast.

**Source.** PMBOK 6, §6.6.2.1 (PMBOK6-0480); Agile Practice Guide, §5.4.1 (APG-0128, APG-0129,
APG-0130, APG-0134, APG-0135, APG-0136); Agile Practice Guide, §2 (APG-0281); FEP02, p. 83 del PDF
(FEP02-0083).

### 5.8 On-demand (pull) scheduling and Kanban

**What it is.** On-demand scheduling, typical of Kanban, comes from lean pull systems and the theory of
constraints. It limits work in progress to balance demand with throughput, and pulls the next item from
a backlog or queue as soon as capacity frees up. There is no pre-built schedule for the items. It suits
incremental product evolution in operations or sustainment, with items of similar size. The Kanban
Method keeps single-item flow and WIP limits even when iterations are used. It values finishing over
starting, and uses board columns with entry and exit policies. Flow metrics are *lead time* (added to
the board until done), *cycle time* (time actually processing) and *response/reaction time* (waiting
until work starts).

**Implication for alma-gantt.**
- Kanban boards stay a non-goal (`SPEC.md` §4). But a hybrid plan needs a way to *reserve time* for
  flow work: `approach = "flow"` container with `wip_limit`, `team`, and either a fixed span (start/end)
  or a span derived from `items × avg_cycle_time / wip`. It is drawn as a single bar with a
  throughput label, not as item bars.
- Flow metrics are imported (CSV with created/started/finished dates) to compute lead, cycle and
  response time for forecasting (5.6). They are not edited in alma.

**Source.** PMBOK 6, p. 195 del PDF (PMBOK6-0374); Agile Practice Guide, §A3.4 (APG-0197, APG-0199,
APG-0200); Agile Practice Guide, §5.4.1 (APG-0131, APG-0132, APG-0134).

### 5.9 Agile measurement vs predictive baselines

**What it is.** The APG argues that agile prefers empirical, value-based measures (finished features)
over predictive ones (% done, traffic lights). It warns about "watermelon" projects that look green
until late and about "90 % done" reports. Baselines are often an artifact of attempted prediction;
agile limits estimation to the next few weeks and replans after each iteration or flow cycle. PMBOK 6
describes agile schedule control as: compare work delivered and accepted with what was expected for the
elapsed cycle, hold retrospectives, reprioritize the backlog, track velocity, and manage changes.

**Implication for alma-gantt.**
- In hybrid plans, *baseline the predictive skeleton and the agile containers' spans and release
  milestones*, not individual stories. `alma baseline save` excludes `kind = story` by default
  (`--include-stories` to override).
- Progress for `iterative` containers defaults to accepted points / planned points (empirical), not
  elapsed-time % complete. `progress_rule = "accepted_points"`.
- `L-AGL-08` (`info`): an `iterative` container whose progress is entered manually as a percentage
  while story data exists.

**Source.** Agile Practice Guide, §5.4 (APG-0123, APG-0124); Agile Practice Guide, §5.4.1 (APG-0126,
APG-0127); PMBOK 6, §6.6 (PMBOK6-0474, PMBOK6-0475).

### 5.10 Showing agile work on a schedule (synthesis)

**What it is.** None of the sources prescribes a single Gantt notation for agile work. Taken together
they imply the following. Iterations are fixed timeboxes on a calendar (5.3). Releases are high-level
timelines ending in a releasable product (5.4). Customers understand "which feature lands at the end of
which iteration" (5.4). Predictive and agile parts coexist and connect (5.1). Stories are not scheduled
individually ahead of time (5.5, 5.9).

**Implication for alma-gantt (recommended rendering).**
- Sprint bands across the full timeline height, one row per cadence, labelled s01, s02...
- Agile container bars spanning their iterations, with an inline iteration grid (tick at each
  boundary) and, optionally, points planned vs done per iteration.
- Release milestones (diamonds) at iteration ends, linked to predictive tasks by normal arrows.
- Optional roadmap lane: features placed in their target iteration.
- Stories never as individual bars in the Gantt (only in a list/backlog view or in burn charts).
- Lint rules adapt by `approach` (open-ends suppressed inside containers, applied at container edges).
- This answers the trigger in `SPEC.md` §1 ("visible Scrum sprints"), so the sprints are not faked as
  tasks.

**Source.** Synthesis of PMBOK 6, §6.5.2.8 (PMBOK6-0461); Agile Practice Guide, §3.1.4 (APG-0044);
Agile Practice Guide, §5.2.8 (APG-0117); Agile Practice Guide, §3.1.6 (APG-0048).

---

## Annex A. Consolidated table of proposed lint rules

Severity is the default. Everything is configurable under `[rules]`. "Phase" means a rule can be
stricter once `settings.phase = "baselined"`.

| Code | Severity | What it checks | Concept |
|---|---|---|---|
| L-WBS-01 | warn | WBS element with exactly one child (100 % rule symptom) | 1.2 |
| L-WBS-02 | warn | Plan with fewer than two WBS levels | 1.2, 1.8 |
| L-WBS-03 | info | WBS element (not activity) named with a leading verb | 1.4, 1.8 |
| L-WBS-04 | warn | Work package without exactly one accountable owner (or owner is a unit, not a person) | 1.8 |
| L-WBS-05 | warn | Work package effort outside 8–80 h (configurable) | 1.5 |
| L-WBS-06 | warn | Work package (or leaf) longer than the reporting period | 1.5 |
| L-WBS-07 | info | Work package missing dictionary fields (description, deliverable, acceptance) | 1.10 |
| L-WBS-08 | info | No project-management / LOE element in the WBS | 1.2, 2.3 |
| L-WBS-09 | warn | Duplicate WBS element names (likely duplicated work) | 1.2, 1.8 |
| L-WBS-10 | info | No final milestone (no clear end point) | 1.9 |
| L-WBS-11 | warn | Planning package starts inside the rolling-wave horizon | 1.12 |
| L-WBS-12 | warn / error | Work package outside any control account (warn); nested control accounts (error) | 1.7 |
| L-WBS-13 | warn | Summary carries its own work in addition to its children | 1.2 |
| L-WBS-14 | info (off) | Leaf with several deliverables or mixed external prerequisites (split hint) | 1.5 |
| L-WBS-15 | info | Control account with fewer than N work packages | 1.7 |
| L-WBS-16 | info | No closeout element | 1.9 |
| L-SCH-01 | error | Milestone with non-zero duration, effort or assignments | 2.4 |
| L-SCH-02 | warn / info | Milestone without criterion or owner (warn if mandatory) | 2.4 |
| L-SCH-03 | warn | Work package with nothing to schedule (no activities, duration or effort) | 1.6 |
| L-SCH-04 | warn | Activity without owner or assignment | 2.2 |
| L-SCH-05 | error (warn in draft) | Negative total float | 4.3 |
| L-SCH-06 | warn | Hard constraint (MSO/MFO/SNLT/FNLT) on an activity with predecessors | 4.5 |
| L-SCH-07 | info | Too many near-critical paths | 4.3 |
| L-SCH-08 | info | Total float above X % of project duration (missing logic) | 4.4 |
| L-SCH-09 | error | LOE/hammock with its own predecessors or successors | 2.3 |
| L-SCH-10 | info | External milestone with no internal checkpoint before it | 2.4 |
| L-SCH-11 | info | SNET constraint used instead of a predecessor | 4.5, 2.8 |
| L-SCH-12 | info | High-risk / long-lead critical activity with no downstream reserve | 4.6 |
| L-SCH-13 | info | Risk response or payment item not represented by an activity/milestone | 4.17 |
| L-DEP-01 | warn | Open start (no predecessor) | 2.8 |
| L-DEP-02 | warn | Open finish (no successor) | 2.8 |
| L-DEP-03 | info | Open end hidden by SS-only successors or FF-only predecessors | 2.8 |
| L-DEP-04 | warn | More than one link between the same pair | 2.5 |
| L-DEP-05 | info | SF link used | 2.5 |
| L-DEP-06 | info | Lead (negative lag) used | 2.7 |
| L-DEP-11 | warn | Lag longer than threshold or than the predecessor (model the wait as an activity) | 2.7 |
| L-DEP-12 | warn | Lag without rationale | 2.7 |
| L-DEP-07 | info | External dependency without assumption/rationale | 2.6 |
| L-DEP-08 | info | Discretionary dependency without rationale | 2.6 |
| L-DEP-09 | warn | Dependency attached to a WBS summary element | 1.3 |
| L-DEP-10 | info (off) | Convergence/divergence hotspot on a near-critical path | 2.9 |
| (E012) | error | Dependency cycle (already in `SPEC.md`) | 2.5 |
| L-EST-01 | info | Estimate without basis | 3.5 |
| L-EST-02 | error | Three-point values not ordered o ≤ m ≤ p | 3.4 |
| L-EST-03 | warn | Duration, effort and units inconsistent | 3.1 |
| L-EST-04 | info | Mandatory milestones or deadlines with no contingency reserve on their driving paths | 3.6 |
| L-EST-05 | info (off) | Individual padding (high percentiles) without project-level reserve | 3.6 |
| L-RES-01 | warn | Over-allocation remaining after leveling/smoothing (already in `SPEC.md`) | 4.7, 4.8 |
| L-BAS-01 | warn | Progress recorded but no baseline | 4.14 |
| L-BAS-02 | error | Progress present but no status date | 4.16 |
| L-BAS-03 | warn | Variance above control threshold | 4.13, 4.16 |
| L-BAS-04 | error | Actuals after status date or progress against unmet mandatory logic | 4.16 |
| L-BAS-05 | info | Rebaseline without change reference | 1.13 |
| L-AGL-01 | error | Overlapping iterations in one cadence | 5.3 |
| L-AGL-02 | warn | Iterations of unequal length in one cadence (no reason given) | 5.3 |
| L-AGL-03 | info | Gap between consecutive iterations | 5.3 |
| L-AGL-04 | warn | Item assigned to an iteration but scheduled outside it | 5.3 |
| L-AGL-05 | info | Release milestone not at the end of its last iteration, or release without iterations | 5.4 |
| L-AGL-06 | info | Iteration longer than 1 month or shorter than 1 week | 5.3 |
| L-AGL-07 | warn | Committed points above iteration capacity | 5.6 |
| L-AGL-08 | info | Manual % complete on an agile container that has story data | 5.9 |
| L-PRS-01 | info | Detailed Gantt page too dense for the chosen font size | 4.12 |

---

## Annex B. Formulas the engine must implement

Notation. For activity *i*: duration *dᵢ* (working time on its calendar), early start/finish *ESᵢ, EFᵢ*,
late start/finish *LSᵢ, LFᵢ*. A link *i → j* has type ∈ {FS, SS, FF, SF} and lag *L* (negative =
lead; working or elapsed). "+" and "−" on dates mean working-time arithmetic on the relevant calendar
(the lag's calendar for *L*, the activity's for *d*). Formulas use the *period* convention (a task
occupies [ES, EF) and EF = ES + d), as in the professor's example. The UI shows the finish as the last
working day (EF − 1 working unit). Constraints and deadlines are applied after the logic terms.

**F-SCH-01 Effort → duration (fixed-effort tasks)**
D = E / (Σₖ uₖ · hₖ), then laid on the calendar. *E* = effort (person-hours), *uₖ* = units
(dedication, 1.0 = full time) of resource *k*, *hₖ* = working hours per day of resource *k*. FEP02's
example: 160 / (2 × 8) = 10 working days. (FEP02, p. 71 del PDF; PMBOK 6, §6.4.)

**F-SCH-02 Parametric**
E (or D) = Q · r_h, or Q / p. *Q* = quantity of work, *r_h* = hours per unit, *p* = productivity
(units per hour). (PMBOK 6, §6.4.2.3.)

**F-SCH-03 Three-point, triangular**
tE = (tO + tM + tP) / 3. (PMBOK 6, §6.4.2.4; FEP02, p. 75 del PDF.)

**F-SCH-04 Three-point, beta (PERT)**
tE = (tO + 4 tM + tP) / 6. (PMBOK 6, §7.2.2.5, cost form; FEP02, p. 75 del PDF, duration form.)

**F-SCH-05 PERT standard deviation and intervals**
σ = (tP − tO) / 6. Intervals tE ± kσ with k = 1, 2, 3 ≈ 68.3 %, 95.4 %, 99.7 % (normal
approximation). (FEP02, p. 75–76 del PDF.)
*Extension, not in these sources:* path expected duration Σ tE and path σ = √(Σ σ²) along one path,
assuming independence. Label it as an approximation that ignores merge bias.

**F-SCH-06 Forward pass** (activity *j*, over all predecessors *i*):
- FS: ESⱼ ≥ EFᵢ + L
- SS: ESⱼ ≥ ESᵢ + L
- FF: EFⱼ ≥ EFᵢ + L  ⇒  ESⱼ ≥ EFᵢ + L − dⱼ
- SF: EFⱼ ≥ ESᵢ + L  ⇒  ESⱼ ≥ ESᵢ + L − dⱼ

ESⱼ = max(project start or SNET/MSO date, all terms above); EFⱼ = ESⱼ + dⱼ. Activities with no
predecessors start at the project start (open start, flagged by L-DEP-01).
(FEP02, p. 79 del PDF, FS case; PMBOK 6, §6.3.2.1 for the meaning of each type.)

**F-SCH-07 Backward pass** (activity *i*, over all successors *j*):
- FS: LFᵢ ≤ LSⱼ − L
- SS: LSᵢ ≤ LSⱼ − L  ⇒  LFᵢ ≤ LSⱼ − L + dᵢ
- FF: LFᵢ ≤ LFⱼ − L
- SF: LSᵢ ≤ LFⱼ − L  ⇒  LFᵢ ≤ LFⱼ − L + dᵢ

LFᵢ = min(late anchor, FNLT/MFO/deadline, all terms above); LSᵢ = LFᵢ − dᵢ. The *late anchor* is the
project early finish, or a project deadline if `late_anchor = "deadline"` (4.1). Activities with no
successors take the late anchor (open finish, flagged by L-DEP-02).

**F-SCH-08 Total float**
TFᵢ = LSᵢ − ESᵢ = LFᵢ − EFᵢ. With SS-only or FF-only links the two ends can differ. Report
TF = min(LS − ES, LF − EF) and flag the case (L-DEP-03). Negative when a late constraint is violated.
(FEP02, p. 79 del PDF; PMBOK 6, §6.5.2.2.)

**F-SCH-09 Free float** (activity *i*, over successors *j*; for activities without successors, against
the late anchor):
FFᵢ = min over j of
- FS: ESⱼ − L − EFᵢ
- SS: ESⱼ − L − ESᵢ
- FF: EFⱼ − L − EFᵢ
- SF: EFⱼ − L − ESᵢ

clipped to ≥ 0 on feasible plans. (PMBOK 6, §6.5.2.2.)

**F-SCH-10 Critical set**
Critical(i) ⇔ TFᵢ ≤ θ (θ = `critical_threshold`, default 0), or, under `longest_path`, *i* lies on a
chain of driving links from a start node to the late anchor with minimum TF. Near-critical: θ <
TFᵢ ≤ θ_near. (PMBOK 6, §6.5.2.2; FEP02, p. 78 del PDF.)

**F-SCH-11 Driving link** (for `explain`)
Link *i → j* is driving if its term in F-SCH-06 equals ESⱼ (or EFⱼ for FF/SF). Ties are broken by source
order (deterministic).

**F-SCH-12 Earned schedule metrics** (glossary definitions)
SV = EV − PV; SPI = EV / PV. *EV* = earned value, *PV* = planned value at the status date.
(PMBOK 6, p. 707 del PDF, glossary.)

**F-SCH-13 Agile EV (finished features)**
EV_agile(t) = BAC_container × (accepted points by *t* / planned points of the container). (Agile
Practice Guide, §5.4.1, APG-0135/0136; the exact formula is our operationalization.)

**F-SCH-14 Velocity and forecast**
V = Σ points of stories accepted in the iteration. Remaining iterations N = ⌈P_rem / V̄⌉ (V̄ = mean,
or a percentile, of recent velocities). Remaining time by cycle time T = n_rem × CT̄ (working days)
for one item in progress at a time; with WIP *w*, T ≈ n_rem × CT̄ / w. (Agile Practice Guide, §5.4.1,
APG-0127, APG-0128, APG-0131. The WIP division is a standard flow approximation, not in the source.)

**F-SCH-15 Ideal burndown**
R_ideal(t) = C × (1 − t / T). *C* = points committed at iteration start, *T* = iteration length in
working days, *t* = working days elapsed. Trend line = least-squares fit on daily actual remaining,
extrapolated to t = T. (PMBOK 6, §6.6.2.1.)

**F-SCH-16 Flow metrics**
Lead time = done − added to board. Cycle time = done − work started. Response (reaction) time = work
started − added (or ready). (Agile Practice Guide, §5.4.1, APG-0131, APG-0134.)

**F-SCH-17 Crash slope** (for `alma compress --strategy crash`)
s = (crash cost − normal cost) / (normal duration − crash duration). Crash the critical activity with
the lowest *s* first, one unit at a time, and recompute the critical paths after each step. (PMBOK 6,
§6.5.2.6 states the "least incremental cost" goal; the slope form is the standard operationalization.)

---

## Annex C. Glossary EN ↔ ES

Spanish terms follow the PMBOK 6 Spanish edition usage where known, and the course slides (FEP02)
otherwise.

| English | Español |
|---|---|
| Work breakdown structure (WBS) | Estructura de desglose del trabajo (EDT) |
| WBS dictionary | Diccionario de la EDT |
| WBS component / element | Componente / elemento de la EDT |
| Work package | Paquete de trabajo |
| Planning package | Paquete de planificación |
| Control account | Cuenta de control |
| Code of accounts | Código de cuentas |
| 100 % rule | Regla del 100 % |
| Decomposition | Descomposición |
| Deliverable | Entregable |
| Rolling wave planning | Planificación gradual (por oleadas) |
| Progressive elaboration | Elaboración progresiva |
| Scope baseline | Línea base del alcance |
| Level of effort (LOE) | Nivel de esfuerzo (actividad de soporte) |
| Apportioned effort | Esfuerzo prorrateado |
| Activity / activity list / activity attributes | Actividad / lista de actividades / atributos de la actividad |
| Milestone / milestone list / milestone chart | Hito / lista de hitos / cronograma de hitos |
| Precedence diagramming method (PDM) | Método de diagramación por precedencia (PDM) |
| Finish-to-start (FS), start-to-start (SS), finish-to-finish (FF), start-to-finish (SF) | Fin a inicio, inicio a inicio, fin a fin, inicio a fin |
| Predecessor / successor | Predecesora / sucesora |
| Mandatory (hard logic) / discretionary (soft logic) dependency | Dependencia obligatoria (lógica dura) / discrecional (lógica blanda) |
| External / internal dependency | Dependencia externa / interna |
| Lead / lag | Adelanto / retraso |
| Path convergence / divergence | Convergencia / divergencia de rutas |
| Project schedule network diagram | Diagrama de red del cronograma |
| Effort / duration | Esfuerzo / duración |
| Analogous / parametric / three-point / bottom-up estimating | Estimación análoga / paramétrica / por tres valores / ascendente |
| Optimistic, most likely, pessimistic | Optimista, más probable, pesimista |
| Basis of estimates | Base de las estimaciones |
| Contingency reserve / management reserve | Reserva para contingencias / reserva de gestión |
| Critical path method (CPM) | Método de la ruta crítica |
| Critical path / near-critical path | Ruta crítica / ruta casi crítica |
| Forward pass / backward pass | Recorrido hacia adelante / hacia atrás |
| Early start/finish, late start/finish | Inicio/final temprano, inicio/final tardío |
| Total float / free float / negative float | Holgura total / holgura libre / holgura negativa |
| Schedule constraint / imposed date | Restricción del cronograma / fecha impuesta |
| Resource leveling / resource smoothing | Nivelación de recursos / estabilización (suavizado) de recursos |
| Schedule compression: crashing / fast tracking | Compresión del cronograma: intensificación / ejecución rápida |
| What-if scenario analysis / simulation (Monte Carlo) | Análisis de escenarios "¿qué pasa si…?" / simulación (Monte Carlo) |
| Schedule model / project schedule / scheduling tool | Modelo de programación / cronograma del proyecto / herramienta de programación |
| Schedule baseline | Línea base del cronograma |
| Schedule data | Datos del cronograma |
| Project calendar / resource calendar | Calendario del proyecto / calendario de recursos |
| Bar chart (Gantt chart) | Diagrama de barras (diagrama / carta Gantt) |
| Summary schedule / detailed schedule / master schedule | Cronograma resumido / detallado / maestro |
| Schedule management plan | Plan de gestión del cronograma |
| Control threshold | Umbral de control |
| Status date (data date) | Fecha de corte (fecha de estado) |
| Schedule variance (SV) / schedule performance index (SPI) | Variación del cronograma / índice de desempeño del cronograma |
| Earned value / planned value | Valor ganado / valor planificado |
| Hybrid life cycle | Ciclo de vida híbrido |
| Predictive / iterative / incremental / agile life cycle | Ciclo de vida predictivo / iterativo / incremental / ágil |
| Iteration / sprint / timebox / cadence | Iteración / sprint / caja de tiempo (timebox) / cadencia |
| Release planning / product roadmap | Planificación de lanzamientos (releases) / hoja de ruta del producto |
| Product backlog / backlog refinement | Backlog (lista priorizada) del producto / refinamiento del backlog |
| User story / epic / story point | Historia de usuario / épica / punto de historia |
| Velocity / capacity | Velocidad / capacidad |
| Burndown chart / burnup chart | Gráfica de trabajo pendiente (burndown) / gráfica de trabajo completado (burnup) |
| On-demand (pull-based) scheduling | Programación bajo demanda (de tipo pull) |
| Kanban board / work in progress (WIP) limit | Tablero Kanban / límite de trabajo en curso (WIP) |
| Lead time / cycle time / response time | Tiempo de entrega (lead time) / tiempo de ciclo / tiempo de respuesta |
| Responsibility assignment matrix (RAM, RACI) | Matriz de asignación de responsabilidades (RAM, RACI) |

---

## Annex D. Gaps: what `SPEC.md` misses or gets wrong according to these sources

Ordered by impact on the engine and the file format, which are the hardest to change after M0.

**G-01. The WBS and the activity network are conflated.** `SPEC.md` §6 says "tasks[] (tree = WBS)"
with `kind = task | summary | milestone`. The sources separate the WBS (deliverables, down to work
packages, no sequence) from activities (actions under a work package, sequenced and estimated). They
also define control accounts, planning packages and LOE as distinct component types. Without a
`wbs_role`, alma cannot produce a correct WBS diagram or dictionary (it would include activities),
apply the 8/80 and reporting-period rules to the right level, or report EVM per control account.
*Fix:* add `wbs_role` (1.6, 1.7) and `kind = loe | reserve | external | story` (2.3, 3.6, 2.6, 5.5).

**G-02. Dependencies have no origin metadata, and summary-task links are allowed silently.**
`deps[]` holds only `pred/type/lag`. The sources require mandatory vs discretionary and internal vs
external, plus documentation (rationale, assumptions) for discretionary links, external links and lags.
Fast-tracking and negative-float analysis depend on knowing which links may move. Separately,
`SPEC.md` §7.2 expands dependencies on summary tasks, which goes against "the WBS has no sequence"
(FEP02-0053). *Fix:* extend `deps[]` (2.6, 2.7); keep the expansion for import but add L-DEP-09.

**G-03. No estimating model and no reserves.** Only "duration | effort". Missing: scheduling type
(fixed duration/effort/units) and consistency rules; three-point inputs (needed later for Monte Carlo,
so they must be captured from M0); parametric inputs; basis of estimate and confidence; contingency vs
management reserve, with their different relationship to the baseline. *Fix:* the `estimate` table and
`kind = reserve` (3.1–3.6).

**G-04. Milestones are too thin.** `kind = milestone` only. The sources expect mandatory vs optional
(contractual), external vs internal, an objective criterion and an owner. Contract dates need a soft
`deadline` rather than a hard constraint that overrides logic. *Fix:* 2.4, 4.5.

**G-05. Backward pass anchoring and float semantics are underspecified.** `SPEC.md` does not say what
the late dates are computed from (project early finish or a deadline). It defines critical as "float ≤
0" but does not mention negative float as a reportable state with analysis, several critical paths,
near-critical paths, or the driving path to a given milestone. Free float is listed but not defined
per link type. *Fix:* 4.1–4.3 and Annex B (F-SCH-06 to F-SCH-11). Also add the property 0 ≤ FF ≤ TF to §12.

**G-06. Leveling and smoothing are not named apart, and compression is missing.** §7.4's "within
float only" is resource smoothing in PMBOK terms. "Allow end-date slip" is leveling. There is no
crashing or fast-tracking support and no what-if / alternative-schedule concept, although PMBOK lists
alternative schedules (leveled/unleveled, best/worst, with/without imposed dates) as schedule data.
*Fix:* `alma level --mode smooth|level`, `alma compress`, `alma whatif`, `[[scenario]]` (4.7–4.10).

**G-07. The agile model is a flat list of iterations.** `iterations[]: id, name, start, end`, used
for "sprints/releases". The sources distinguish a *cadence* of equal timeboxes, *releases* (3–6-month
timelines ending in a releasable product) and the *backlog* (stories, points). They also define
velocity and capacity, burndown/burnup and agile EV on finished features. Iteration-based and
flow-based work are different. And a hybrid plan needs a per-branch `approach`, so that predictive lint
rules (open ends, logic) are not applied to stories. *Fix:* `[[cadence]]`, `[[release]]`,
`approach = predictive|iterative|flow`, story items, velocity forecast, burn charts (5.1–5.10).

**G-08. No schedule management plan settings.** Units of measure, accuracy range, reporting period,
control thresholds and % complete rules are what make progress and variance meaningful. None is in
`settings`. *Fix:* `[settings.schedule]` (4.13), used by lint (L-WBS-06, L-BAS-03) and by EV.

**G-09. Progress and control are underspecified.** "percent, actual start/finish (M4)". The sources
use actual and remaining duration, *physical* % complete (separate from duration %), progress
measurement rules, variance on float as well as dates, trend analysis, forecasts, and contractor
status. The engine also needs a rule for out-of-sequence progress and for scheduling remaining work
from the status date. *Fix:* 4.16.

**G-10. Baselines are snapshots without approval or scope.** PMBOK baselines are *approved* and change
only through change control. The scope baseline includes the WBS and dictionary. *Fix:* baseline
metadata (`approved_by/on`, `change_ref`), snapshot of scope and codes, scope diff (1.13, 4.14).

**G-11. Presentation formats are incomplete.** `SPEC.md` covers the Gantt and the WBS tree. The sources
add the milestone chart (the format for management and clients), a summary schedule rolled up to a WBS
level, and the network diagram (activity-on-node with the ES/EF/LS/LF/TF box, pure logic or
time-scaled). They also note that a plain Gantt hides relationships, so arrows should be on by default
in detailed views. *Fix:* export presets (4.12) and `--view network` (2.9).

**G-12. Traceability links are missing.** The professor's chain (requirement → milestone → payment) and
his "risk responses must be in the schedule" test need `requirement_refs`, `payment_ref` and `risk_ref`
on schedule nodes, plus a `trace` query (4.17). `SPEC.md` §3 promises "all numbers traceable", but the
model has no field that carries traceability.

**Minor notes.**
- The sources disagree on a control account's minimum size (PMBOK 6: two or more work packages; WBS
  Practice Standard: one or more). Make it a rule parameter (1.7).
- PMBOK 6 gives only the triangular formula for *duration* (§6.4.2.4) and both formulas for *cost*
  (§7.2.2.5); FEP02 gives both for duration. Offer both; default to beta to match the course.
- Critical chain is not developed in PMBOK 6 or FEP02. The only related idea in scope is FEP02's advice
  to aggregate reserve at project level. Keep critical-chain buffers out of the M0–M4 spec; the
  `reserve` task kind covers the aggregated-reserve practice.
- `SPEC.md` §4 lists "Kanban boards as a primary view" as a non-goal. That is compatible with the
  sources, provided flow work can still reserve time on the plan (5.8).
