# AGENTS.md

This file provides project-level instructions for Codex and other coding agents working in this repository.

## Overview

`zihuan-next` is a Rust + Slint UI node-graph workflow engine for building event-driven bot pipelines. Nodes are composable DAG components connected through typed ports. Workflows are saved and loaded as JSON and can run headless or in the GUI editor.

## Working Style

- Keep changes focused. Do not mix unrelated refactors into feature or bug-fix work.
- Preserve existing architecture and naming unless the task requires a deliberate change.
- Prefer small, local edits over broad rewrites.
- When instructions conflict, prefer the behavior described by the current code and docs over older Copilot notes.
- Detailed module-specific rules live under `document/`.

## Build And Run

```bash
# Build
cargo build
cargo build --release

# Run GUI mode
cargo run
cargo run -- --graph-json example.json

# Run headless
cargo run -- --graph-json input.json --no-gui
cargo run -- --graph-json input.json --save-graph-json output.json --no-gui

# Validate graph JSON before running (exits 0=ok/warn, 1=errors, 2=load failure)
cargo run -- --graph-json input.json --validate

# Tests
cargo test
cargo test test_name
cargo test -- --ignored

# Infra
docker compose -f docker/docker-compose.yaml up -d
alembic upgrade head
alembic revision --autogenerate -m "description"
```

## Core Rules

- One node per file.
- The graph must remain a DAG.
- Register new node types in `src/node/registry.rs`.
- Keep Slint responsible for presentation and Rust responsible for orchestration.
- Keep message parsing and storage resilient.

## Detailed References

- `document/dev-guides/README.md`
- `document/dev-guides/node-system.md`
- `document/dev-guides/ui-architecture.md`
- `document/dev-guides/qq-message.md`
- `document/dev-guides/qq_message_storage.md`
- `document/dev-guides/runtime-utils.md`

## Database And Schema Changes

- Database schema models live in `database/models/`.
- After schema changes, generate migrations with Alembic instead of hand-waving the DB state.
- Do not change persistence schema without updating the relevant migration path and examples.

## Validation Expectations

After Rust changes:

- Run `cargo check` for lightweight verification when possible.
- Run `cargo test` when the change affects behavior covered by tests.

After UI changes:

- Run `cargo check`.
- Manually verify menu, tabs, pan/zoom, node drag/resize, box selection, edge selection, dialogs, and these node UIs:
  - `string_data`
  - `message_list_data`
  - `qq_message_list_data`
  - `message_event_type_filter`
  - `preview_message_list`

## Useful References

- `README.md`
- `document/dev-guides/README.md`
- `document/node/dynamic-port-nodes.md`
- `document/node/node-graph-json.md`
- `document/node/node-development.md`
- `document/program-execute-flow.md`
- `example.json`
