# 0014 — Cost reserves and the cost stack

- **Status:** accepted
- **Date:** 2026-11-08

## Context
The knowledge notes (resources §4.1–§4.3, Appendix D gap 1) treat reserves as the main thing
SPEC was missing. Contingency covers identified risks and lives inside the cost baseline.
Management reserve covers the unforeseen and lives outside the baseline, inside the budget.
Without that split, BAC, PV and "budget" are ambiguous. Time reserves already exist as buffer
tasks (D7). `[[reserve]]` was a reserved key.

## Decision
- **Format:** `[[reserve]]` with `id` (global namespace), `kind = contingency | management`
  (default contingency), `amount` (money string), `scope` (a task id; default the whole
  project) and `basis`. `risks` stays reserved until `[[risk]]` exists.
- **Cost stack (F-COST-09, F-COST-10):** work and fixed costs + contingency = cost baseline
  (the `total` of `alma cost`, and BAC once frozen); + management reserve = project budget.
  `alma cost` returns the stack. `alma scurve` returns `budget`, a ceiling to draw over the curve.
- **Time-phasing:** contingency accrues as a lump at the finish of its scope (the project
  finish when it has no scope). Of the notes' two options (spread like its scope, or a lump at
  the scope's finish), this is the conservative one. Spreading is an option for later.
  Management reserve never accrues: it is not in PV.
- **Roll-up:** contingency scoped to a task counts in that task's cost and its ancestors', as
  the notes' stack does (activity contingency becomes part of the work package).
- **Lint:** contingency without `basis` is `L-RISK-01` (warning). A cushion nobody can
  justify is the first thing an evaluator cuts.
- **Commands:** `alma reserve add <id> --amount A [--kind] [--scope] [--basis]` and
  `alma reserve rm <id>` (batch ops `reserve.add`, `reserve.rm`, hence MCP tools). Mutation
  reports show `cost_delta` for contingency, not for management reserve.

## Consequences
Proposals made with alma can state contingency and management reserve separately, and the
numbers add up exactly. Drawing on a reserve (`alma reserve draw`) and releasing management
reserve into the baseline through a change (`L-BAS-08`) come with progress tracking (M4).
Funding limits (`[[funding]]`, `L-COST-04`) and risk-based sizing (`[[risk]]`, EMV,
`L-RISK-02`) can be added on top without changing this model.
