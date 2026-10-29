# 0001 — Write our own scheduling engine

- **Status:** accepted
- **Date:** 2026-10-29

## Context
utf8proj (Rust, MIT/Apache) already offers CPM, leveling and calendars from a text format, but it is
pre-1.0, has a single maintainer and no interactive editor. alma-gantt aims to be the most robust
open-source Gantt tool, with an engine whose behavior we fully control (explainability, the agent
contract, the knowledge-base refinements in SPEC.md §7.6).

## Decision
`alma-sched` is our own engine. We do not depend on utf8proj or any other scheduling crate.
utf8proj, TaskJuggler and MPXJ are references for semantics and optional differential-testing oracles.

## Consequences
More work up front on CPM, calendars and leveling, offset by full control and no upstream risk.
Correctness must be proven by our own property, golden and differential tests (SPEC.md §12).
