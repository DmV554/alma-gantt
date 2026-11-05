# 0010 — MCP server on the official Rust SDK, tools generated from the batch schema

- **Status:** accepted
- **Date:** 2026-11-05

## Context
SPEC.md §5 (M0.5) asks for `alma mcp`, exposing the same operations as the CLI as MCP tools,
with the same schemas. The MCP specification changes often: the 2026-07-28 revision removed the
`initialize` handshake in favour of stateless requests (`server/discover`), while many clients
still use the handshake of 2024-11-05 … 2025-11-25.

## Decision
- New crate `alma-mcp` on `rmcp` 3.9 (the official Rust SDK, Apache-2.0), stdio transport only,
  features `server` and `transport-io`. The SDK negotiates every protocol version from
  2024-11-05 to 2026-07-28, including the stateless lifecycle. The CLI hosts it as `alma mcp`.
- Like the CLI, the server only calls `alma-ops` (ADR 0004). File helpers that both need
  (plan discovery, git, atomic byte writes, the guide) live in `alma_ops::files`.
- Mutation tools are generated from `alma schema`: one tool per batch operation (`task.add` →
  `task_add`), its input being that operation plus `file` and `dry_run`. `apply` runs a batch.
  Query tools mirror the CLI queries. Every tool returns the JSON the CLI prints with `--json`,
  as text and as structured content; failures are tool results with `isError` and `E-*` codes,
  never protocol errors.
- `render` returns the Gantt chart as an MCP image, so a multimodal model can look at its plan.
- Writes are serialized with a lock; tool work runs on a blocking thread.

## Consequences
A new batch operation becomes an MCP tool with no extra code; a test fails if one lacks a tool.
`rmcp` brings `tokio` into the workspace (only `alma-mcp` and the CLI use it). Protocol tests
cover the handshake versions and both SDK lifecycles against the real binary.
