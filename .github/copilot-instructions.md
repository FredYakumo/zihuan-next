# Copilot Instructions

## Overview

`zihuan-next` is a Rust node-graph workflow engine for event-driven bot pipelines. The graph describes **data flow** between processing steps — complexity (algorithms, agentic loops, control flow) is encapsulated inside individual nodes, keeping the graph topology simple. When a new complex problem arises, build a new node rather than adding complexity to the graph canvas.

The backend is a single Rust binary (Salvo HTTP server) that serves a browser-based editor (Vite + TypeScript + Litegraph.js) and exposes REST + WebSocket APIs.

For crate layout, build/run/test commands, and module-specific rules, see [document/dev-guides/node-system.md](../document/dev-guides/node-system.md), [document/dev-guides/code-conventions.md](../document/dev-guides/code-conventions.md), and [document/dev-guides/ui-architecture.md](../document/dev-guides/ui-architecture.md). Always consult `document/` before writing or modifying code that touches an unfamiliar area — do not infer file paths or APIs from this file.

## High-Level Rules

- Keep changes focused.
- Preserve current architecture and naming unless the task requires otherwise.
- One node per file.
- Preserve DAG-based graph behavior. Keep graph topology simple; encapsulate complexity in nodes.
- Frontend (TypeScript/Litegraph.js) handles presentation; Rust backend handles graph execution and state.
- Keep message parsing and storage behavior resilient.
- Do not write unit tests by default. Only add tests when the user explicitly indicates a feature is complex enough to warrant them, and place them in the dedicated test location for that crate/module.
- Do not add useless comments. Skip comments when the code is already self-explanatory; only write a comment when the *why* is non-obvious (hidden constraint, subtle invariant, deliberate workaround).
- Reuse existing functionality whenever possible. Shared utility functions must live in their dedicated location — search before writing a new helper; do not duplicate logic. Refer to `document/dev-guides/` for the canonical location of utilities and node placement.
- Common/shared type definitions, and type definitions that may cause circular references, must be placed in `zihuan_core`.
- Otherwise, code and types must stay in the package that owns the functional responsibility, with high cohesion and low coupling.

## Rust Style Preferences

Follow these practical Rust style preferences unless an existing local module already uses a different established pattern:

- Organize imports in three groups: `std`, external crates, then `crate`/workspace imports.
- Use explicit, domain-driven names for functions, helpers, constants, and temporary variables. Prefer names that describe the data or action directly.
- For parsing, validation, and business branching, prefer readable `match`/`if let` code and intermediate variables over compressed combinator-heavy expressions.
- Keep row parsing, graph input normalization, and other transformation-heavy code linear. Prefer plain loops when they communicate the workflow more clearly than iterator chains.
- Extract repeated conversion logic into small local helpers instead of duplicating date parsing, row lookup, or field normalization code inline.
- Keep important constants, aliases, and static regex/configuration near the top of the module.
- Prefer explicit struct literals with named fields; only use `..Default::default()` when the remaining defaults are intentional and still readable.
- Include concrete context in errors and logs, especially field names, node inputs, database columns, and source values.
- Avoid abstraction for its own sake. Add generic helpers, traits, or macros only when they remove clear duplication without hiding business meaning.

For node file placement, node registration, naming conventions, and other code-level details, look up [document/dev-guides/code-conventions.md](../document/dev-guides/code-conventions.md) and [document/dev-guides/node-system.md](../document/dev-guides/node-system.md).

## Code Search

When navigating the codebase, prefer the configured LSP MCP tools:

- **Rust**: `rust-analyzer` MCP server (via `rust-analyzer-mcp`).
- **TypeScript**: `typescript` MCP server (via `@mizchi/lsmcp`).

These provide accurate symbol search, goto-definition, and find-references without relying on text-only grep.

## Detailed References

All architecture, crate layout, command reference, and module rules live under `document/`. Start with [document/dev-guides/node-system.md](../document/dev-guides/node-system.md), [document/dev-guides/code-conventions.md](../document/dev-guides/code-conventions.md), and [document/dev-guides/ui-architecture.md](../document/dev-guides/ui-architecture.md).
