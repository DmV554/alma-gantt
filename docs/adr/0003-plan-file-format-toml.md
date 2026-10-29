# 0003 — Plan files are TOML edited losslessly with toml_edit

- **Status:** accepted
- **Date:** 2026-10-29

## Context
The plan must live in git with small, reviewable diffs and survive edits from the GUI, the CLI,
agents and humans with a text editor. A custom DSL reads well but makes lossless round-trip from
a GUI hard.

## Decision
One `*.alma.toml` file per project, flat `[[task]]` tables with a `parent` key, keys in a fixed
order, ISO dates, stable slug IDs. Read and written with `toml_edit` so comments and formatting
survive. Computed values (dates, float, costs) are never stored in the file. Format in SPEC.md §8.

## Consequences
Round-trip tests are mandatory: reading and writing a formatted file is byte-identical, and a
single edit changes only the affected lines. `alma fmt` defines the canonical layout.
