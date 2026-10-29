# 0002 — Desktop UI with Tauri 2 and Svelte 5; engine shared via WASM

- **Status:** accepted
- **Date:** 2026-10-29

## Context
The target is a top-tier interface. Pure-Rust GUI options (GPUI, Slint, egui, Iced, Dioxus) are
either pre-1.0 with breaking changes or harder to bring to a polished product look. A web front-end
gives the richest ecosystem and a free web build. Comparison in SPEC.md §10.2.

## Decision
Tauri 2 shell with a Svelte 5 + TypeScript front-end. All logic stays in Rust; the browser build
uses the same crates compiled to WASM. The timeline is our own Canvas renderer fed by `alma-layout`;
no third-party JS Gantt library.

## Consequences
WebView differences (WebKitGTK on Linux) must be tested early. The front-end never re-implements
scheduling: it calls the command layer. A native GPUI client can be reconsidered after v1.
