# AGENTS.md

This file provides project-level instructions for Codex and other coding agents working in this repository.

## Overview

`zihuan-next` is a Rust node-graph workflow engine for event-driven bot pipelines. The graph describes **data flow** between processing steps — complexity (algorithms, agentic loops, control flow) is encapsulated inside individual nodes, keeping the graph topology simple. When a new complex problem arises, build a new node rather than adding complexity to the graph canvas.

The backend is a single Rust binary (Salvo HTTP server) that serves a browser-based editor (Vite + TypeScript + Litegraph.js) and exposes REST + WebSocket APIs.

For crate layout, build/run/test commands, infra setup, schema migration, and module-specific rules, see [document/dev-guides/README.md](document/dev-guides/README.md). Always consult `document/` before writing or modifying code that touches an unfamiliar area — do not infer file paths or APIs from this file.

## Working Style

- Keep changes focused. Do not mix unrelated refactors into feature or bug-fix work.
- Preserve existing architecture and naming unless the task requires a deliberate change.
- Prefer small, local edits over broad rewrites.
- When instructions conflict, prefer the behavior described by the current code and `document/` over older agent notes.

## Core Rules

- One node per file.
- The graph must remain a DAG. Keep the graph topology simple; encapsulate complexity in nodes.
- Frontend (TypeScript/Litegraph.js) handles presentation; Rust backend handles graph execution and state.
- Keep message parsing and storage resilient.
- Do not write unit tests by default. Only add tests when the user explicitly indicates a feature is complex enough to warrant them, and place them in the dedicated test location for that crate/module.
- Do not add useless comments. Skip comments when the code is already self-explanatory; only write a comment when the *why* is non-obvious (hidden constraint, subtle invariant, deliberate workaround).
- Reuse existing functionality whenever possible. Shared utility functions must live in their dedicated location — search before writing a new helper; do not duplicate logic. Refer to `document/dev-guides/` for the canonical location of utilities and node placement.

For node file placement, node registration, naming conventions, validation expectations, database/schema rules, and other code-level details, look up [document/dev-guides/code-conventions.md](document/dev-guides/code-conventions.md) and [document/dev-guides/README.md](document/dev-guides/README.md).

## Code Search

When navigating the codebase, prefer the configured LSP MCP tools:

- **Rust**: `rust-analyzer` MCP server (via `rust-analyzer-mcp`).
- **TypeScript**: `typescript` MCP server (via `@mizchi/lsmcp`).

These provide accurate symbol search, goto-definition, and find-references without relying on text-only grep.

## Detailed References

All architecture, crate layout, command reference, and module rules live under `document/`. Start at [document/dev-guides/README.md](document/dev-guides/README.md).