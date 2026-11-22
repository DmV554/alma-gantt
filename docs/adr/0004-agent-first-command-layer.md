# 0004 — Agent-first: one command layer for CLI, MCP and GUI

- **Status:** accepted
- **Date:** 2026-10-29

## Context
LLM agents will do most of the planning work, with humans reviewing and editing by hand. If the CLI,
MCP server and GUI each implemented edits, they would drift apart.

## Decision
`alma-ops` defines every operation as a typed command with a JSON schema. The CLI, the MCP server
and the GUI only call `alma-ops`. Every command honors the contract in SPEC.md §8a: `--json`,
pagination, atomicity, `--dry-run`, actionable diagnostics, idempotency, determinism, no prompts.

## Consequences
Each new command ships with tests for JSON output, dry run, atomicity (a failed command leaves the
file byte-identical) and idempotency. A feature is not done until an agent can use it from the CLI.
