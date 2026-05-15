# AGENTS.md

This file provides project-level instructions for Codex and other coding agents working in this repository.

## Overview

`zihuan-next` is a Rust node-graph workflow engine for event-driven bot pipelines. The graph describes **data flow** between processing steps — complexity (algorithms, agentic loops, control flow) is encapsulated inside individual nodes, keeping the graph topology simple. When a new complex problem arises, build a new node rather than adding complexity to the graph canvas.

The backend is a single Rust binary (Salvo HTTP server) that serves a browser-based editor (Vite + TypeScript + Litegraph.js) and exposes REST + WebSocket APIs. The Brain tool-call loop engine (`zihuan_agent`) is shared by both graph nodes and service-hosted agents.

For crate layout, build/run/test commands, infra setup, schema migration, and module-specific rules, see [document/dev-guides/node-system.md](document/dev-guides/node-system.md), [document/dev-guides/code-conventions.md](document/dev-guides/code-conventions.md), and [document/dev-guides/ui-architecture.md](document/dev-guides/ui-architecture.md). Always consult `document/` before writing or modifying code that touches an unfamiliar area — do not infer file paths or APIs from this file.

## Working Style

- Keep changes focused. Do not mix unrelated refactors into feature or bug-fix work.
- Preserve existing architecture and naming unless the task requires a deliberate change.
- Prefer small, local edits over broad rewrites.
- When instructions conflict, prefer the behavior described by the current code and `document/` over older agent notes.

## Rust Style Preferences

When writing Rust in this repository, follow these style preferences unless the local module already has a stronger convention:

- Group imports in three blocks: `std`, third-party crates, then `crate`/workspace imports. Keep the grouping visually clean with one blank line between blocks.
- Prefer direct, domain-specific names. Function names should describe the action or conversion being performed, such as `parse_timestamp_field`, `load_records_from_file`, or `build_node_runtime_state`.
- Prefer explicit control flow over clever chaining for business logic. Use `match`, `if let`, and intermediate local variables freely when handling `Option`, `Result`, row parsing, or multi-branch data conversion.
- Keep data-loading and transformation code linear and readable. For row-by-row parsing, batched inserts, or graph input normalization, prefer straightforward loops over dense iterator pipelines when the loop carries business meaning.
- Extract repeated parsing or conversion logic into small helpers close to the call site. Date parsing, row access, schema field conversion, and similar boundary logic should not be duplicated inline.
- Define module-level constants, type aliases, and static configuration near the top of the file when they shape the module behavior.
- Build structs with explicit named fields. Use `..Default::default()` only when the defaulted fields are intentional and still leave the constructed value easy to read.
- Error messages should carry concrete business context such as field names, node inputs, external column names, or source values. Avoid vague failure text when adding new parsing or integration code.
- Prefer pragmatic readability over abstraction. Do not introduce a generic helper, trait, or macro unless it removes real duplication that appears in more than one place.

- **Error handling:** Avoid using `if let Err(` or similar verbose error handling patterns. Prefer the `?` operator for propagating errors whenever possible, to keep code concise and idiomatic. Excessive manual error unwrapping makes code resemble Go and should be minimized.

## Core Rules

- One node per file.
- The graph must remain a DAG. Keep the graph topology simple; encapsulate complexity in nodes.
- Frontend (TypeScript/Litegraph.js) handles presentation; Rust backend handles graph execution and state.
- Keep message parsing and storage resilient.
- Do not write unit tests by default. Only add tests when the user explicitly indicates a feature is complex enough to warrant them, and place them in the dedicated test location for that crate/module.
- Do not add useless comments. Skip comments when the code is already self-explanatory; only write a comment when the *why* is non-obvious (hidden constraint, subtle invariant, deliberate workaround).
- Reuse existing functionality whenever possible. Shared utility functions must live in their dedicated location — search before writing a new helper; do not duplicate logic. Refer to `document/dev-guides/` for the canonical location of utilities and node placement.
- Common/shared type definitions, and type definitions that may cause circular references, must be placed in `zihuan_core`.
- Otherwise, code and types must stay in the package that owns the functional responsibility, with high cohesion and low coupling.

For node file placement, node registration, naming conventions, validation expectations, database/schema rules, and other code-level details, look up [document/dev-guides/code-conventions.md](document/dev-guides/code-conventions.md) and [document/dev-guides/node-system.md](document/dev-guides/node-system.md).

## Code Search

When navigating the codebase, prefer the configured LSP MCP tools:

- **Rust**: `rust-analyzer` MCP server (via `rust-analyzer-mcp`).
- **TypeScript**: `typescript` MCP server (via `@mizchi/lsmcp`).

These provide accurate symbol search, goto-definition, and find-references without relying on text-only grep.

## Detailed References

All architecture, crate layout, command reference, and module rules live under `document/`. Start with [document/dev-guides/node-system.md](document/dev-guides/node-system.md), [document/dev-guides/code-conventions.md](document/dev-guides/code-conventions.md), and [document/dev-guides/ui-architecture.md](document/dev-guides/ui-architecture.md).
