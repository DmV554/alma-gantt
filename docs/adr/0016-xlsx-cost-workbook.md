# 0016 — XLSX cost workbook with rust_xlsxwriter

- **Status:** accepted
- **Date:** 2026-11-08

## Context
SPEC.md §5 (M2) asks for an XLSX export of costs per period. Proposals and steering committees
work in spreadsheets, and the knowledge notes (§5.1, §10) ask for cost per period with category
rows and total and cumulative rows. CSV covers single tables; a workbook carries several
sheets with number and date formats.

## Decision
- Use **`rust_xlsxwriter` 0.102.0** (MIT OR Apache-2.0, actively maintained, no default
  features). Its dependencies (`zip`, `flate2`, `zlib-rs`, …) are MIT, Apache-2.0 or Zlib. It is
  pinned like every other dependency. Only `alma-export` uses it: `alma_export::xlsx` turns plain
  tables (text, money, number, date cells) into a workbook. `alma-ops` builds the tables.
- **Deterministic:** the document properties carry a fixed creation date (2000-01-01). The same
  plan therefore gives the same bytes, which a test checks.
- **Sheets:**
  1. *Cost per period*. Rows: one per resource, one per fixed-cost category, and contingency.
     Columns: one per period (the first day of each). Then a Total row and a Cumulative row,
     plus Planned value (from the active baseline) and Funded rows when the plan has them.
     Notes above the table state the currency, the basis (forecast of the current schedule,
     contingency included) and the management reserve.
  2. *Tasks*: WBS, id, name, kind, dates, duration, float, critical, cost.
  3. *Resources*: rate, capacity, booked work, cost.
  4. *Risks* (when there are any): probability, impact, EMV, covering reserves.
- **Interface:** `alma export costs.xlsx [--period month|week|day] [--baseline ID]`, and MCP
  `export` with an `.xlsx` path. A workbook cannot go to stdout.

## Consequences
Plans export to the format reviewers expect, with the numbers alma computes and nothing
recalculated in the spreadsheet. A cash-flow sheet (inflows, NPV, IRR; notes §10) can be added
as another sheet later.
